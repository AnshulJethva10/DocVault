use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::{path::Path, time::Duration};

use crate::{avi, cli::{DecodeArgs, resolve_password, write_output}};
use docvault_core::{crypto::AUTH_FAILED, decoder::{decode_png_frames, DecodeResult}};

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn run(args: DecodeArgs) -> Result<()> {
    let input = &args.file;
    let output_dir = args.output.as_deref();

    // Validate input before prompting, so a typo doesn't cost a password entry
    if !input.is_file() {
        anyhow::bail!("File not found: '{}'", input.display());
    }

    let password = resolve_password(
        args.password,
        args.password_file.as_deref(),
        "Decryption password: ",
        false,
    )?;

    let spinner = make_spinner();
    spinner.set_message(format!(
        "Decrypting {}…",
        input.file_name().unwrap_or_default().to_string_lossy()
    ));

    // Determine format from extension, then verify with magic bytes
    let ext = input
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let name = input
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let decode_result = if ext == "png" || name.ends_with(".vault.png") {
        decode_png_vault(input, &password)
    } else if ext == "avi" || name.ends_with(".vault.avi") {
        decode_avi_vault(input, &password)
    } else {
        // Try PNG first (check magic), then AVI
        let bytes = std::fs::read(input)
            .with_context(|| format!("Cannot read '{}'", input.display()))?;
        if is_png(&bytes) {
            decode_png_bytes(&bytes, &password)
        } else if is_riff_avi(&bytes) {
            decode_avi_bytes(&bytes, &password)
        } else {
            anyhow::bail!(
                "'{}' is not a recognised vault format (.vault.png or .vault.avi)",
                input.display()
            )
        }
    }?;

    // Determine output path
    let out_dir = output_dir.unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")));
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Cannot create output directory '{}'", out_dir.display()))?;

    // The core already sanitizes the stored name; re-check here so the output
    // path can never leave `out_dir` even if that ever regresses.
    let safe_name = Path::new(&decode_result.filename)
        .file_name()
        .filter(|n| *n == std::ffi::OsStr::new(&decode_result.filename))
        .ok_or_else(|| {
            anyhow::anyhow!("Refusing unsafe filename stored in vault: '{}'", decode_result.filename)
        })?;
    let out_path = out_dir.join(safe_name);

    if let Err(e) = write_output(&out_path, &decode_result.file_bytes, args.force) {
        spinner.finish_and_clear();
        return Err(e);
    }

    let restored_size = decode_result.file_bytes.len() as u64;

    spinner.finish_with_message(format!(
        "✔  {} → {}  ({})",
        input.file_name().unwrap_or_default().to_string_lossy(),
        out_path.file_name().unwrap_or_default().to_string_lossy(),
        crate::commands::encode::fmt_size(restored_size),
    ));

    Ok(())
}

// ─── Format-specific decoders ─────────────────────────────────────────────────

fn decode_png_vault(path: &Path, password: &str) -> Result<DecodeResult> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Cannot read '{}'", path.display()))?;
    decode_png_bytes(&bytes, password)
}

fn decode_png_bytes(bytes: &[u8], password: &str) -> Result<DecodeResult> {
    if !is_png(bytes) {
        anyhow::bail!("File does not have a valid PNG signature");
    }
    decode_png_frames(&[bytes], password).map_err(decryption_error)
}

fn decode_avi_vault(path: &Path, password: &str) -> Result<DecodeResult> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Cannot read '{}'", path.display()))?;
    decode_avi_bytes(&bytes, password)
}

fn decode_avi_bytes(bytes: &[u8], password: &str) -> Result<DecodeResult> {
    if !is_riff_avi(bytes) {
        anyhow::bail!("File does not have a valid RIFF/AVI signature");
    }
    let png_frames = avi::read_avi(bytes).context("Failed to parse AVI container")?;
    decode_png_frames(&png_frames, password).map_err(decryption_error)
}

/// Map a core error to a user-facing one; authentication failures get a fixed message.
fn decryption_error(e: String) -> anyhow::Error {
    if e == AUTH_FAILED {
        anyhow::anyhow!("Wrong password or corrupted vault file").context("Decryption failed")
    } else {
        anyhow::anyhow!("{e}").context("Decryption failed")
    }
}

// ─── Magic byte detection ─────────────────────────────────────────────────────

/// PNG magic: \x89PNG\r\n\x1a\n
fn is_png(data: &[u8]) -> bool {
    data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
}

/// RIFF/AVI magic: "RIFF" at 0, "AVI " at 8
fn is_riff_avi(data: &[u8]) -> bool {
    data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"AVI "
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn make_spinner() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
