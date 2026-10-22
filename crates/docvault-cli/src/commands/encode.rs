use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{avi, cli::{EncodeArgs, resolve_password, write_output}};
use docvault_core::encoder::{encode_png_frames, FRAME_HEIGHT, FRAME_WIDTH};

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn run(args: EncodeArgs) -> Result<()> {
    // Catch typos in paths before asking for (and confirming) a password
    for file in &args.files {
        if !file.is_file() {
            anyhow::bail!("File not found: '{}'", file.display());
        }
    }
    let password = resolve_password(
        args.password,
        args.password_file.as_deref(),
        "Encryption password: ",
        true,
    )?;
    let output_dir = args.output.as_deref();
    let jobs = plan_outputs(&args.files, output_dir);

    if jobs.len() == 1 {
        encode_single(&jobs[0], &password, args.force)
    } else {
        encode_batch(&jobs, &password, args.force)
    }
}

// ─── Output planning ──────────────────────────────────────────────────────────

/// One input file and where its vault should be written (without extension).
struct EncodeJob {
    input: PathBuf,
    out_dir: PathBuf,
    stem: String,
}

/// Assign each input a distinct output stem.
///
/// `report.pdf` and `report.docx` would otherwise both become
/// `report.vault.png` and the second would overwrite the first, so later
/// duplicates get a ` (2)`, ` (3)`, … suffix. Comparison is case-insensitive
/// because macOS and Windows filesystems are.
fn plan_outputs(files: &[PathBuf], output_dir: Option<&Path>) -> Vec<EncodeJob> {
    let mut taken: HashSet<String> = HashSet::new();
    files
        .iter()
        .map(|input| {
            let out_dir = output_dir
                .map(Path::to_path_buf)
                .unwrap_or_else(|| input.parent().unwrap_or(Path::new(".")).to_path_buf());
            let base = input
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let mut stem = base.clone();
            let mut n = 2;
            while !taken.insert(out_dir.join(&stem).to_string_lossy().to_lowercase()) {
                stem = format!("{base} ({n})");
                n += 1;
            }
            EncodeJob { input: input.clone(), out_dir, stem }
        })
        .collect()
}

// ─── Single-file encode ───────────────────────────────────────────────────────

fn encode_single(job: &EncodeJob, password: &str, force: bool) -> Result<()> {
    let input = job.input.as_path();
    let spinner = make_spinner();
    spinner.set_message(format!(
        "Encrypting {}…",
        input.file_name().unwrap_or_default().to_string_lossy()
    ));

    let out_path = match run_encode(job, password, force, Some(&spinner)) {
        Ok(path) => path,
        Err(e) => {
            spinner.finish_and_clear();
            return Err(e);
        }
    };

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

fn encode_batch(jobs: &[EncodeJob], password: &str, force: bool) -> Result<()> {
    let mp = MultiProgress::new();
    let total = jobs.len();
    let mut successes = 0usize;
    let mut failures = 0usize;

    for (idx, job) in jobs.iter().enumerate() {
        let input = &job.input;
        let pb = mp.add(make_spinner());
        pb.set_message(format!(
            "[{}/{}] Encrypting {}…",
            idx + 1,
            total,
            input.file_name().unwrap_or_default().to_string_lossy()
        ));

        match run_encode(job, password, force, None) {
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
    job: &EncodeJob,
    password: &str,
    force: bool,
    pb: Option<&ProgressBar>,
) -> Result<PathBuf> {
    let input = job.input.as_path();
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

    // Encrypt + pack into lossless PNG frames
    let frame_pngs = encode_png_frames(&file_bytes, &filename, &mime, password)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("Failed to encrypt '{filename}'"))?;
    drop(file_bytes);

    if let Some(pb) = pb {
        pb.set_message(format!("Writing {} frame(s)…", frame_pngs.len()));
    }

    // Determine output path
    let stem = &job.stem;
    let out_dir = job.out_dir.as_path();
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("Cannot create output directory '{}'", out_dir.display()))?;

    let (out_path, is_video) = if frame_pngs.len() == 1 {
        (out_dir.join(format!("{stem}.vault.png")), false)
    } else {
        (out_dir.join(format!("{stem}.vault.avi")), true)
    };

    // Write output file
    if is_video {
        let avi_bytes = avi::write_avi(&frame_pngs, FRAME_WIDTH, FRAME_HEIGHT)?;
        write_output(&out_path, &avi_bytes, force)?;
    } else {
        write_output(&out_path, &frame_pngs[0], force)?;
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
