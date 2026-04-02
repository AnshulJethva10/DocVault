use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::{path::Path, time::Duration};

use crate::{avi, cli::{DecodeArgs, resolve_password}, pixel_io};
use docvault_core::decoder::{decode, decode_multi};

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn run(args: DecodeArgs) -> Result<()> {
    let password = resolve_password(args.password, "Decryption password: ")?;

    let input = &args.file;
    let output_dir = args.output.as_deref();

    // Validate input exists
    if !input.exists() {
        anyhow::bail!("File not found: '{}'", input.display());
    }

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

    let out_path = out_dir.join(&decode_result.filename);

    std::fs::write(&out_path, &decode_result.file_bytes)
        .with_context(|| format!("Cannot write '{}'", out_path.display()))?;

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

struct Decoded {
    file_bytes: Vec<u8>,
    filename:   String,
}

fn decode_png_vault(path: &Path, password: &str) -> Result<Decoded> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Cannot read '{}'", path.display()))?;
    decode_png_bytes(&bytes, password)
}

fn decode_png_bytes(bytes: &[u8], password: &str) -> Result<Decoded> {
    if !is_png(bytes) {
        anyhow::bail!("File does not have a valid PNG signature");
    }

    let (rgba, width, height) = pixel_io::decode_png_to_rgba(bytes)
        .context("Failed to decode PNG pixel data")?;

    let result = decode(&rgba, width, height, password)
        .map_err(|e| {
            if e.contains("Invalid password") || e.contains("corrupted") || e.contains("mac") {
                anyhow::anyhow!("Wrong password or corrupted vault file")
            } else {
                anyhow::anyhow!("{e}")
            }
        })
        .context("Decryption failed")?;

    Ok(Decoded {
        file_bytes: result.file_bytes,
        filename:   result.filename,
    })
}

fn decode_avi_vault(path: &Path, password: &str) -> Result<Decoded> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Cannot read '{}'", path.display()))?;
    decode_avi_bytes(&bytes, password)
}

fn decode_avi_bytes(bytes: &[u8], password: &str) -> Result<Decoded> {
    if !is_riff_avi(bytes) {
        anyhow::bail!("File does not have a valid RIFF/AVI signature");
    }

    // Extract PNG frames from the AVI container
    let png_frames = avi::read_avi(bytes).context("Failed to parse AVI container")?;
    let frame_count = png_frames.len() as u32;

    // Decode each PNG frame to RGBA and concatenate into a flat buffer
    let mut flat_rgba: Vec<u8> = Vec::new();
    let mut frame_width = 1024u32;
    let mut frame_height = 1024u32;

    for (i, png) in png_frames.iter().enumerate() {
        let (rgba, w, h) = pixel_io::decode_png_to_rgba(png)
            .with_context(|| format!("Failed to decode PNG frame {i}"))?;
        frame_width = w;
        frame_height = h;
        flat_rgba.extend_from_slice(&rgba);
    }

    let result = decode_multi(&flat_rgba, frame_count, frame_width, frame_height, password)
        .map_err(|e| {
            if e.contains("Invalid password") || e.contains("corrupted") || e.contains("mac") {
                anyhow::anyhow!("Wrong password or corrupted vault file")
            } else {
                anyhow::anyhow!("{e}")
            }
        })
        .context("Decryption failed")?;

    Ok(Decoded {
        file_bytes: result.file_bytes,
        filename:   result.filename,
    })
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
