//! Integration tests for the `docvault` CLI binary.
//!
//! These tests exercise the full encode→decode round-trip by running the
//! compiled binary via `assert_cmd` and checking exit codes and file contents.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn docvault() -> Command {
    Command::cargo_bin("docvault").unwrap()
}

// ─── Round-trip: small file (→ .vault.png) ───────────────────────────────────

#[test]
fn encode_decode_small_file_png() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("hello.txt");
    let vault = dir.path().join("hello.vault.png");
    let restored = dir.path().join("hello.txt");

    let original = b"Hello, DocVault CLI! This is a small test.";
    fs::write(&input, original).unwrap();

    // Encode
    docvault()
        .args(["encode", input.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "testpw-123")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("vault.png").or(predicate::str::is_empty()))
        .stderr(
            predicate::str::contains("vault.png")
                .or(predicate::str::contains("✔"))
                .or(predicate::str::is_empty()),
        );

    assert!(vault.exists(), ".vault.png file must be created");

    // Decode (file will be restored to hello.txt — original was removed so decode writes fresh)
    fs::remove_file(&input).unwrap(); // remove original so decode produces a fresh file

    docvault()
        .args(["decode", vault.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "testpw-123")
        .current_dir(dir.path())
        .assert()
        .success();

    assert!(restored.exists(), "restored file must exist");
    let content = fs::read(&restored).unwrap();
    assert_eq!(content, original, "restored bytes must match original");
}

// ─── Round-trip: large file (→ .vault.avi) ───────────────────────────────────

#[test]
fn encode_decode_large_file_avi() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("large.bin");
    let vault = dir.path().join("large.vault.avi");

    // 4 MB of patterned bytes (will not compress away)
    let original: Vec<u8> = (0..4_000_000).map(|i| (i * 7 % 251) as u8).collect();
    fs::write(&input, &original).unwrap();

    // Encode
    docvault()
        .args(["encode", input.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "large-file-pw")
        .current_dir(dir.path())
        .assert()
        .success();

    assert!(vault.exists(), ".vault.avi must be created for large files");

    // Decode
    let out_dir = TempDir::new().unwrap();
    docvault()
        .args([
            "decode",
            vault.to_str().unwrap(),
            "--output",
            out_dir.path().to_str().unwrap(),
        ])
        .env("DOCVAULT_PASSWORD", "large-file-pw")
        .assert()
        .success();

    let restored = out_dir.path().join("large.bin");
    assert!(restored.exists(), "restored file must exist");
    let content = fs::read(&restored).unwrap();
    assert_eq!(content.len(), original.len(), "restored file must be same size");
    assert_eq!(content, original, "restored bytes must match original");
}

// ─── Custom output directory ──────────────────────────────────────────────────

#[test]
fn encode_respects_output_dir() {
    let src_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();

    let input = src_dir.path().join("note.txt");
    fs::write(&input, b"output dir test").unwrap();

    docvault()
        .args([
            "encode",
            input.to_str().unwrap(),
            "--output",
            out_dir.path().to_str().unwrap(),
        ])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .success();

    assert!(
        out_dir.path().join("note.vault.png").exists(),
        "vault file must be in the specified output directory"
    );
    assert!(
        !src_dir.path().join("note.vault.png").exists(),
        "vault file must NOT appear in the source directory"
    );
}

// ─── Wrong password → non-zero exit ──────────────────────────────────────────

#[test]
fn decode_wrong_password_exits_with_error() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"top secret content").unwrap();

    // Encode with correct password
    docvault()
        .args(["encode", input.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "correct")
        .current_dir(dir.path())
        .assert()
        .success();

    let vault = dir.path().join("secret.vault.png");

    // Decode with wrong password — must fail
    docvault()
        .args(["decode", vault.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "wrong-password")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Wrong password")
                .or(predicate::str::contains("Decryption"))
                .or(predicate::str::contains("error")),
        );
}

// ─── Non-vault PNG → clear error ─────────────────────────────────────────────

#[test]
fn decode_plain_png_returns_clear_error() {
    let dir = TempDir::new().unwrap();

    // Create a real (non-vault) PNG using the image crate, write via an in-memory buffer
    let img = image::RgbImage::from_raw(4, 4, vec![0u8; 4 * 4 * 3]).unwrap();
    let dyn_img = image::DynamicImage::ImageRgb8(img);
    let fake_vault = dir.path().join("fake.vault.png");
    let mut png_buf = Vec::new();
    dyn_img
        .write_to(
            &mut std::io::Cursor::new(&mut png_buf),
            image::ImageFormat::Png,
        )
        .unwrap();
    std::fs::write(&fake_vault, &png_buf).unwrap();

    docvault()
        .args(["decode", fake_vault.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "any-password")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("magic")
                .or(predicate::str::contains("DocVault"))
                .or(predicate::str::contains("Decryption"))
                .or(predicate::str::contains("error")),
        );
}

// ─── Batch encode ─────────────────────────────────────────────────────────────

#[test]
fn batch_encode_all_files_succeed() {
    let src_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();

    let files: Vec<_> = (1..=3)
        .map(|i| {
            let path = src_dir.path().join(format!("doc{i}.txt"));
            fs::write(&path, format!("content of document {i}").as_bytes()).unwrap();
            path
        })
        .collect();

    let mut cmd = docvault();
    cmd.arg("encode");
    for f in &files {
        cmd.arg(f.to_str().unwrap());
    }
    cmd.args(["--output", out_dir.path().to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "batch-pw")
        .assert()
        .success();

    for i in 1..=3 {
        assert!(
            out_dir.path().join(format!("doc{i}.vault.png")).exists(),
            "doc{i}.vault.png must exist"
        );
    }
}

// ─── Overwrite protection ─────────────────────────────────────────────────────

#[test]
fn decode_refuses_to_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("keep.txt");
    fs::write(&input, b"original").unwrap();

    docvault()
        .args(["encode", input.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .success();

    // Change the file on disk; decode must not clobber it
    fs::write(&input, b"newer local edits").unwrap();
    let vault = dir.path().join("keep.vault.png");

    docvault()
        .args(["decode", vault.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
    assert_eq!(fs::read(&input).unwrap(), b"newer local edits");

    docvault()
        .args(["decode", vault.to_str().unwrap(), "--force"])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .success();
    assert_eq!(fs::read(&input).unwrap(), b"original");
}

#[test]
fn encode_refuses_to_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("a.txt");
    fs::write(&input, b"data").unwrap();
    let vault = dir.path().join("a.vault.png");
    fs::write(&vault, b"existing vault").unwrap();

    docvault()
        .args(["encode", input.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
    assert_eq!(fs::read(&vault).unwrap(), b"existing vault");

    docvault()
        .args(["encode", input.to_str().unwrap(), "--force"])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .success();
    assert_ne!(fs::read(&vault).unwrap(), b"existing vault");
}

// ─── Batch name collisions ────────────────────────────────────────────────────

#[test]
fn batch_encode_same_stem_keeps_both_files() {
    let src_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let pdf = src_dir.path().join("report.pdf");
    let docx = src_dir.path().join("report.docx");
    fs::write(&pdf, b"pdf bytes").unwrap();
    fs::write(&docx, b"docx bytes").unwrap();

    docvault()
        .args(["encode", pdf.to_str().unwrap(), docx.to_str().unwrap()])
        .args(["--output", out_dir.path().to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .success();

    let first = out_dir.path().join("report.vault.png");
    let second = out_dir.path().join("report (2).vault.png");
    assert!(first.exists() && second.exists(), "both vaults must be written");

    // Each vault restores its own original file
    let restore_dir = TempDir::new().unwrap();
    for vault in [&first, &second] {
        docvault()
            .args(["decode", vault.to_str().unwrap()])
            .args(["--output", restore_dir.path().to_str().unwrap()])
            .env("DOCVAULT_PASSWORD", "pw")
            .assert()
            .success();
    }
    assert_eq!(fs::read(restore_dir.path().join("report.pdf")).unwrap(), b"pdf bytes");
    assert_eq!(fs::read(restore_dir.path().join("report.docx")).unwrap(), b"docx bytes");
}

// ─── Password file, permissions, atomic writes ───────────────────────────────

#[test]
fn password_file_round_trip_restores_owner_only_file() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("notes.txt");
    let pw_file = dir.path().join("pw.txt");
    fs::write(&input, b"private notes").unwrap();
    fs::write(&pw_file, b"from-a-file\n").unwrap();

    docvault()
        .args(["encode", input.to_str().unwrap(), "--password-file", pw_file.to_str().unwrap()])
        .env_remove("DOCVAULT_PASSWORD")
        .assert()
        .success();
    fs::remove_file(&input).unwrap();

    // The trailing newline must not be part of the password
    docvault()
        .args(["decode", dir.path().join("notes.vault.png").to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "from-a-file")
        .assert()
        .success();
    assert_eq!(fs::read(&input).unwrap(), b"private notes");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&input).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "restored plaintext must be owner-only");
    }

    // No temporary files are left next to the output
    let leftovers: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
        .collect();
    assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
}

#[test]
fn wrong_password_reports_exactly_that() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("s.txt");
    fs::write(&input, b"x").unwrap();
    docvault().args(["encode", input.to_str().unwrap()]).env("DOCVAULT_PASSWORD", "a").assert().success();
    docvault()
        .args(["decode", dir.path().join("s.vault.png").to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "b")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Wrong password or corrupted vault file"));
}

#[test]
fn missing_input_fails_before_asking_for_a_password() {
    // No password source at all: if the CLI prompted first it would fail on the
    // missing terminal instead of reporting the missing file.
    docvault()
        .args(["decode", "does-not-exist.vault.png"])
        .env_remove("DOCVAULT_PASSWORD")
        .assert()
        .failure()
        .stderr(predicate::str::contains("File not found"));
    docvault()
        .args(["encode", "does-not-exist.pdf"])
        .env_remove("DOCVAULT_PASSWORD")
        .assert()
        .failure()
        .stderr(predicate::str::contains("File not found"));
}

#[test]
fn truncated_avi_reports_missing_frames() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("big.bin");
    fs::write(&input, vec![7u8; 4_000_000]).unwrap();
    docvault().args(["encode", input.to_str().unwrap()]).env("DOCVAULT_PASSWORD", "pw").assert().success();

    let vault = dir.path().join("big.vault.avi");
    let bytes = fs::read(&vault).unwrap();
    // Cut the file in the middle of the second frame
    fs::write(&vault, &bytes[..bytes.len() * 3 / 4]).unwrap();
    docvault()
        .args(["decode", vault.to_str().unwrap()])
        .env("DOCVAULT_PASSWORD", "pw")
        .assert()
        .failure()
        .stderr(predicate::str::contains("truncated").or(predicate::str::contains("frame")));
}
