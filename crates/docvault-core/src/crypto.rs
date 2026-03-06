use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::num::NonZeroU32;

/// PBKDF2 iteration count — OWASP 2023 recommendation for SHA-256
const PBKDF2_ITERATIONS: u32 = 310_000;

/// Derive a 32-byte AES key from a password and salt using PBKDF2-HMAC-SHA256
pub fn derive_key(password: &[u8], salt: &[u8]) -> Result<Vec<u8>, String> {
    let mut key = vec![0u8; 32];
    let iterations = NonZeroU32::new(PBKDF2_ITERATIONS)
        .ok_or_else(|| "Invalid iteration count".to_string())?;
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        salt,
        password,
        &mut key,
    );
    Ok(key)
}

/// Encrypt plaintext using AES-256-GCM.
/// Returns ciphertext with the 16-byte GCM authentication tag appended.
pub fn encrypt(plaintext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, String> {
    let unbound_key = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|e| format!("Failed to create AES key: {}", e))?;
    let less_safe_key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::try_assume_unique_for_key(iv)
        .map_err(|e| format!("Invalid nonce: {}", e))?;

    // ring encrypts in place and appends the tag
    let mut in_out = plaintext.to_vec();
    less_safe_key
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    Ok(in_out)
}

/// Decrypt ciphertext (with appended GCM tag) using AES-256-GCM.
/// Returns original plaintext on success.
pub fn decrypt(ciphertext: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>, String> {
    let unbound_key = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|e| format!("Failed to create AES key: {}", e))?;
    let less_safe_key = LessSafeKey::new(unbound_key);

    let nonce = Nonce::try_assume_unique_for_key(iv)
        .map_err(|e| format!("Invalid nonce: {}", e))?;

    let mut in_out = ciphertext.to_vec();
    let plaintext = less_safe_key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| "Invalid password or corrupted file".to_string())?;

    Ok(plaintext.to_vec())
}

/// Generate cryptographically secure random bytes using ring's SystemRandom
pub fn generate_random_bytes(len: usize) -> Vec<u8> {
    let rng = SystemRandom::new();
    let mut buf = vec![0u8; len];
    rng.fill(&mut buf).expect("Failed to generate random bytes");
    buf
}
