use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::num::NonZeroU32;

/// PBKDF2 iteration count used by format v1/v2 vaults.
pub const LEGACY_PBKDF2_ITERATIONS: u32 = 310_000;
/// AES-GCM authentication tag length in bytes.
pub const TAG_LEN: usize = 16;
/// Error returned when authentication fails (wrong password or tampered data).
pub const AUTH_FAILED: &str = "Invalid password or corrupted file";

/// Derive a 32-byte AES key from a password and salt using PBKDF2-HMAC-SHA256
pub fn derive_key(password: &[u8], salt: &[u8], iterations: u32) -> Result<Vec<u8>, String> {
    let mut key = vec![0u8; 32];
    let iterations =
        NonZeroU32::new(iterations).ok_or_else(|| "Invalid iteration count".to_string())?;
    pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, iterations, salt, password, &mut key);
    Ok(key)
}

fn make_key(key: &[u8]) -> Result<LessSafeKey, String> {
    let unbound_key =
        UnboundKey::new(&AES_256_GCM, key).map_err(|e| format!("Failed to create AES key: {}", e))?;
    Ok(LessSafeKey::new(unbound_key))
}

/// Encrypt `buf` in place with AES-256-GCM and return the 16-byte tag.
/// `aad` is authenticated but not encrypted (used for the vault metadata header).
pub fn encrypt_in_place(
    buf: &mut [u8],
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
) -> Result<[u8; TAG_LEN], String> {
    let nonce = Nonce::try_assume_unique_for_key(iv).map_err(|e| format!("Invalid nonce: {}", e))?;
    let tag = make_key(key)?
        .seal_in_place_separate_tag(nonce, Aad::from(aad), buf)
        .map_err(|e| format!("Encryption failed: {}", e))?;
    let mut out = [0u8; TAG_LEN];
    out.copy_from_slice(tag.as_ref());
    Ok(out)
}

/// Decrypt `buf` (ciphertext followed by the 16-byte tag) in place.
/// On success the plaintext occupies `buf[..returned_len]`.
/// `aad` must match the bytes passed to `encrypt_in_place`, or authentication fails.
pub fn decrypt_in_place(buf: &mut [u8], key: &[u8], iv: &[u8], aad: &[u8]) -> Result<usize, String> {
    let nonce = Nonce::try_assume_unique_for_key(iv).map_err(|e| format!("Invalid nonce: {}", e))?;
    let plaintext = make_key(key)?
        .open_in_place(nonce, Aad::from(aad), buf)
        .map_err(|_| AUTH_FAILED.to_string())?;
    Ok(plaintext.len())
}

/// Encrypt plaintext, returning ciphertext with the 16-byte GCM tag appended.
pub fn encrypt(plaintext: &[u8], key: &[u8], iv: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
    // Reserve room for the tag up front so appending it never reallocates
    let mut out = Vec::with_capacity(plaintext.len() + TAG_LEN);
    out.extend_from_slice(plaintext);
    let tag = encrypt_in_place(&mut out, key, iv, aad)?;
    out.extend_from_slice(&tag);
    Ok(out)
}

/// Decrypt ciphertext (with appended GCM tag), returning the original plaintext.
pub fn decrypt(ciphertext: &[u8], key: &[u8], iv: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
    let mut buf = ciphertext.to_vec();
    let len = decrypt_in_place(&mut buf, key, iv, aad)?;
    buf.truncate(len);
    Ok(buf)
}

/// Generate cryptographically secure random bytes using ring's SystemRandom
pub fn generate_random_bytes(len: usize) -> Vec<u8> {
    let rng = SystemRandom::new();
    let mut buf = vec![0u8; len];
    rng.fill(&mut buf).expect("Failed to generate random bytes");
    buf
}
