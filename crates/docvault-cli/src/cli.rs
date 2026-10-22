use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

/// DocVault — encrypt and disguise files as PNG images or AVI videos.
///
/// Files encrypted with AES-256-GCM are encoded as pixel data inside a PNG
/// image (≤ ~3 MB) or an AVI video using the lossless MPNG codec (larger
/// files). All operations run locally — nothing ever leaves your machine.
///
/// Password priority:
///   1. --password-file (first line of the file)
///   2. --password flag
///   3. DOCVAULT_PASSWORD environment variable
///   4. Interactive prompt (hidden input, not stored in shell history)
#[derive(Parser)]
#[command(name = "docvault", version, author)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Encrypt and disguise one or more files.
    ///
    /// Small files (≤ ~3 MB) produce a .vault.png image.
    /// Larger files produce a .vault.avi video (MPNG lossless codec).
    Encode(EncodeArgs),

    /// Decrypt and restore a .vault.png or .vault.avi file.
    Decode(DecodeArgs),
}

#[derive(Parser, Debug)]
pub struct EncodeArgs {
    /// File(s) to encrypt. Shell glob expansion is supported (e.g. *.pdf).
    #[arg(required = true, num_args = 1..)]
    pub files: Vec<PathBuf>,

    /// Encryption password.
    /// Can also be set via the DOCVAULT_PASSWORD environment variable.
    /// If omitted an interactive hidden prompt is shown.
    #[arg(short, long, env = "DOCVAULT_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,

    /// Read the password from the first line of this file (use '-' for stdin).
    /// Safer than --password, which is visible to other users in `ps`.
    #[arg(long, value_name = "FILE")]
    pub password_file: Option<PathBuf>,

    /// Output directory for vault files (default: same directory as input).
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Overwrite vault files that already exist.
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Parser, Debug)]
pub struct DecodeArgs {
    /// Vault file to decrypt (.vault.png or .vault.avi).
    pub file: PathBuf,

    /// Decryption password.
    /// Can also be set via the DOCVAULT_PASSWORD environment variable.
    /// If omitted an interactive hidden prompt is shown.
    #[arg(short, long, env = "DOCVAULT_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,

    /// Read the password from the first line of this file (use '-' for stdin).
    /// Safer than --password, which is visible to other users in `ps`.
    #[arg(long, value_name = "FILE")]
    pub password_file: Option<PathBuf>,

    /// Output directory for the restored file (default: same directory as vault file).
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Overwrite the restored file if it already exists.
    #[arg(short, long)]
    pub force: bool,
}

/// Resolve the password from a file, the flag/env value, or an interactive prompt.
///
/// With `confirm`, the interactive prompt asks twice so a typo cannot lock
/// the user out of a freshly encoded file.
pub fn resolve_password(
    provided: Option<String>,
    password_file: Option<&Path>,
    prompt: &str,
    confirm: bool,
) -> anyhow::Result<String> {
    if let Some(path) = password_file {
        let pw = read_password_file(path)?;
        if pw.is_empty() {
            anyhow::bail!("Password file '{}' is empty", path.display());
        }
        return Ok(pw);
    }
    match provided {
        Some(pw) if !pw.is_empty() => Ok(pw),
        _ => {
            let pw = rpassword::prompt_password(prompt)?;
            if pw.is_empty() {
                anyhow::bail!("Password cannot be empty");
            }
            if confirm {
                let again = rpassword::prompt_password("Confirm password: ")?;
                if again != pw {
                    anyhow::bail!("Passwords do not match");
                }
            }
            Ok(pw)
        }
    }
}

/// First line of `path` (or stdin for `-`), without the line ending.
fn read_password_file(path: &Path) -> anyhow::Result<String> {
    use anyhow::Context;
    use std::io::BufRead;

    let mut line = String::new();
    if path == Path::new("-") {
        std::io::stdin().lock().read_line(&mut line).context("Cannot read password from stdin")?;
    } else {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Cannot read password file '{}'", path.display()))?;
        std::io::BufReader::new(file)
            .read_line(&mut line)
            .with_context(|| format!("Cannot read password file '{}'", path.display()))?;
    }
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

/// Write `bytes` to `path` atomically, refusing to replace an existing file unless `force`.
///
/// Data goes to a temporary file in the same directory, is flushed to disk,
/// and is then renamed into place, so an interrupted run never leaves a
/// truncated file behind. The file is created owner-only (0600 on Unix):
/// restored documents are the sensitive plaintext.
pub fn write_output(path: &Path, bytes: &[u8], force: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;

    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if !force && path.exists() {
        anyhow::bail!("'{}' already exists (use --force to overwrite)", path.display());
    }
    let mut tmp = tempfile::Builder::new()
        .prefix(".docvault-")
        .suffix(".part")
        .tempfile_in(dir)
        .with_context(|| format!("Cannot write to '{}'", dir.display()))?;
    tmp.write_all(bytes)
        .and_then(|_| tmp.as_file().sync_all())
        .with_context(|| format!("Cannot write '{}'", path.display()))?;

    let persisted = if force { tmp.persist(path) } else { tmp.persist_noclobber(path) };
    persisted.map(drop).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow::anyhow!("'{}' already exists (use --force to overwrite)", path.display())
        } else {
            anyhow::Error::new(e.error).context(format!("Cannot write '{}'", path.display()))
        }
    })
}
