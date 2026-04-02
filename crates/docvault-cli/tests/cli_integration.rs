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
