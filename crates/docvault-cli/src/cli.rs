use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// DocVault — encrypt and disguise files as PNG images or AVI videos.
///
/// Files encrypted with AES-256-GCM are encoded as pixel data inside a PNG
/// image (≤ ~3 MB) or an AVI video using the lossless MPNG codec (larger
/// files). All operations run locally — nothing ever leaves your machine.
///
/// Password priority:
///   1. --password flag
///   2. DOCVAULT_PASSWORD environment variable
///   3. Interactive prompt (hidden input, not stored in shell history)
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

    /// Output directory for the restored file (default: same directory as vault file).
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Overwrite the restored file if it already exists.
    #[arg(short, long)]
    pub force: bool,
}

/// Resolve the password: use the provided value, or prompt interactively.
///
/// With `confirm`, the interactive prompt asks twice so a typo cannot lock
/// the user out of a freshly encoded file.
pub fn resolve_password(
    provided: Option<String>,
    prompt: &str,
    confirm: bool,
) -> anyhow::Result<String> {
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

/// Create `path` for writing, refusing to replace an existing file unless `force`.
pub fn write_output(path: &std::path::Path, bytes: &[u8], force: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true);
    if force {
        opts.create(true).truncate(true);
    } else {
        // create_new fails atomically if the file exists — no check/write race
        opts.create_new(true);
    }
    let mut file = opts.open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow::anyhow!("'{}' already exists (use --force to overwrite)", path.display())
        } else {
            anyhow::Error::new(e).context(format!("Cannot write '{}'", path.display()))
        }
    })?;
    file.write_all(bytes)
        .with_context(|| format!("Cannot write '{}'", path.display()))
}
