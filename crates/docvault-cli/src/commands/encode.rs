use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{avi, cli::{EncodeArgs, resolve_password}, pixel_io};
use docvault_core::encoder::encode_multi;

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn run(args: EncodeArgs) -> Result<()> {
    let password = resolve_password(args.password, "Encryption password: ")?;

    if args.files.len() == 1 {
        encode_single(&args.files[0], &password, args.output.as_deref())
    } else {
        encode_batch(&args.files, &password, args.output.as_deref())
    }
}

// ─── Single-file encode ───────────────────────────────────────────────────────

fn encode_single(input: &Path, password: &str, output_dir: Option<&Path>) -> Result<()> {
    let spinner = make_spinner();
    spinner.set_message(format!(
        "Encrypting {}…",
        input.file_name().unwrap_or_default().to_string_lossy()
    ));

    let out_path = run_encode(input, password, output_dir, Some(&spinner))?;

    let in_size = std::fs::metadata(input)?.len();
    let out_size = std::fs::metadata(&out_path)?.len();

    spinner.finish_with_message(format!(
        "✔  {} → {}  ({} → {})",
        input.file_name().unwrap_or_default().to_string_lossy(),
        out_path.file_name().unwrap_or_default().to_string_lossy(),
        fmt_size(in_size),
        fmt_size(out_size),
    ));

    Ok(())
}

// ─── Batch encode ─────────────────────────────────────────────────────────────

fn encode_batch(files: &[PathBuf], password: &str, output_dir: Option<&Path>) -> Result<()> {
    let mp = MultiProgress::new();
    let total = files.len();
    let mut successes = 0usize;
    let mut failures = 0usize;

    for (idx, input) in files.iter().enumerate() {
        let pb = mp.add(make_spinner());
        pb.set_message(format!(
            "[{}/{}] Encrypting {}…",
            idx + 1,
            total,
            input.file_name().unwrap_or_default().to_string_lossy()
        ));

        match run_encode(input, password, output_dir, None) {
            Ok(out_path) => {
                pb.finish_with_message(format!(
                    "  ✔  [{}/{}] {} → {}",
                    idx + 1,
                    total,
                    input.file_name().unwrap_or_default().to_string_lossy(),
                    out_path.file_name().unwrap_or_default().to_string_lossy(),
                ));
                successes += 1;
            }
            Err(e) => {
                pb.finish_with_message(format!(
                    "  ✗  [{}/{}] {}: {e}",
                    idx + 1,
                    total,
                    input.file_name().unwrap_or_default().to_string_lossy(),
                ));
                failures += 1;
            }
        }
    }

    // Summary line
    let summary = if failures == 0 {
        format!("\n✔  {successes}/{total} files encoded successfully")
    } else {
        format!("\n⚠  {successes}/{total} succeeded, {failures} failed")
    };
    eprintln!("{summary}");

    if failures > 0 {
        anyhow::bail!("{failures} file(s) failed to encode");
    }
    Ok(())
}

// ─── Core encode logic ────────────────────────────────────────────────────────

/// Encode a single file and write the output.  Returns the output path.
fn run_encode(
    input: &Path,
    password: &str,
    output_dir: Option<&Path>,
    pb: Option<&ProgressBar>,
) -> Result<PathBuf> {
    // Read input
    let file_bytes = std::fs::read(input)
        .with_context(|| format!("Cannot read '{}'", input.display()))?;

    let filename = input
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mime = mime_guess::from_path(input)
        .first_or_octet_stream()
        .to_string();

    // Encrypt + pack into RGBA frames
    let encoded = encode_multi(&file_bytes, &filename, &mime, password)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("Failed to encrypt '{filename}'"))?;

    // Convert each RGBA frame → PNG bytes
    if let Some(pb) = pb {
        pb.set_message(format!("Writing {} frame(s)…", encoded.frame_count));
    }

    let mut frame_pngs: Vec<Vec<u8>> = Vec::with_capacity(encoded.frame_count as usize);
    for i in 0..encoded.frame_count {
        let rgba = encoded
            .get_frame_rgba(i)
            .ok_or_else(|| anyhow::anyhow!("Frame {i} missing from encode result"))?;
        let png = pixel_io::encode_frame_to_png(rgba, encoded.width, encoded.height)
            .with_context(|| format!("Failed to encode frame {i} as PNG"))?;
        frame_pngs.push(png);
    }

    // Determine output path
    let stem = input
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let out_dir = output_dir.unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")));
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Cannot create output directory '{}'", out_dir.display()))?;

    let (out_path, is_video) = if frame_pngs.len() == 1 {
        (out_dir.join(format!("{stem}.vault.png")), false)
    } else {
        (out_dir.join(format!("{stem}.vault.avi")), true)
    };

    // Write output file
    if is_video {
        let avi_bytes = avi::write_avi(&frame_pngs, encoded.width, encoded.height);
        std::fs::write(&out_path, &avi_bytes)
            .with_context(|| format!("Cannot write '{}'", out_path.display()))?;
    } else {
        std::fs::write(&out_path, &frame_pngs[0])
            .with_context(|| format!("Cannot write '{}'", out_path.display()))?;
    }

    Ok(out_path)
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

pub fn fmt_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
