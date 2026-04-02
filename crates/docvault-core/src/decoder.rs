#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::metadata::{VaultMetadata, METADATA_SIZE};

// ─── DecodeResult ─────────────────────────────────────────────────────────────

/// Result of a successful decode operation.
/// WASM: fields exposed via js_sys getters.
/// Native: fields are plain `pub`.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct DecodeResult {
    file_bytes: Vec<u8>,
    filename: String,
    mime_type: String,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl DecodeResult {
    #[wasm_bindgen(getter)]
    pub fn file_bytes(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(&self.file_bytes[..])
    }
    #[wasm_bindgen(getter)]
    pub fn filename(&self) -> String {
        self.filename.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn mime_type(&self) -> String {
        self.mime_type.clone()
    }
}

#[cfg(not(feature = "wasm"))]
#[derive(Debug)]
pub struct DecodeResult {
    pub file_bytes: Vec<u8>,
    pub filename: String,
    pub mime_type: String,
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Extract RGB bytes from a single RGBA frame (discards the alpha channel).
fn extract_rgb_from_rgba(rgba_pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let pixel_count = (width * height) as usize;
    let expected_rgba_len = pixel_count * 4;

    if rgba_pixels.len() < expected_rgba_len {
        return Err(format!(
            "Pixel data too short: expected {} bytes, got {}",
            expected_rgba_len,
            rgba_pixels.len()
        ));
    }

    let mut rgb_bytes = Vec::with_capacity(pixel_count * 3);
    for i in 0..pixel_count {
        let offset = i * 4;
        rgb_bytes.push(rgba_pixels[offset]);     // R
        rgb_bytes.push(rgba_pixels[offset + 1]); // G
        rgb_bytes.push(rgba_pixels[offset + 2]); // B
    }
    Ok(rgb_bytes)
}

/// Core decode from concatenated RGB bytes (shared by single and multi-frame paths).
fn decode_from_rgb(rgb_bytes: &[u8], password: &str) -> Result<DecodeResult, String> {
    if rgb_bytes.len() < METADATA_SIZE {
        return Err("Image too small to contain DocVault metadata".to_string());
    }

    let metadata = VaultMetadata::from_bytes(&rgb_bytes[..METADATA_SIZE])?;

    if !metadata.validate_magic() {
        return Err("Not a DocVault encoded image (invalid magic bytes)".to_string());
    }

    if metadata.version != 1 {
        return Err(format!("Unsupported DocVault format version: {}", metadata.version));
    }

    let key = crypto::derive_key(password.as_bytes(), &metadata.salt)?;

    // AES-256-GCM ciphertext = plaintext + 16-byte GCM tag
    let ciphertext_len = metadata.file_size as usize + 16;
    let ciphertext_start = METADATA_SIZE;
    let ciphertext_end = ciphertext_start + ciphertext_len;

    if rgb_bytes.len() < ciphertext_end {
        return Err("Image data truncated: not enough bytes for ciphertext".to_string());
    }

    let ciphertext = &rgb_bytes[ciphertext_start..ciphertext_end];
    let plaintext = crypto::decrypt(ciphertext, &key, &metadata.iv)?;

    let file_size = metadata.file_size as usize;
    if plaintext.len() < file_size {
        return Err("Decrypted data shorter than expected file size".to_string());
    }

    Ok(DecodeResult {
        file_bytes: plaintext[..file_size].to_vec(),
        filename:   metadata.filename_str(),
        mime_type:  metadata.mime_type_str(),
    })
}

// ─── Public pure-Rust API ─────────────────────────────────────────────────────

/// Decode a single-frame vault image (RGBA pixel data) back to the original file.
pub fn decode(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
    password: &str,
) -> Result<DecodeResult, String> {
    let rgb_bytes = extract_rgb_from_rgba(rgba_pixels, width, height)?;
    decode_from_rgb(&rgb_bytes, password)
}

/// Decode a multi-frame vault video back to the original file.
///
/// `frames_rgba` is a flat buffer with all frames concatenated.
/// Each frame is `width × height × 4` bytes of RGBA pixel data.
pub fn decode_multi(
    frames_rgba: &[u8],
    frame_count: u32,
    width: u32,
    height: u32,
    password: &str,
) -> Result<DecodeResult, String> {
    let frame_rgba_size = (width * height * 4) as usize;
    let expected_len = frame_count as usize * frame_rgba_size;

    if frames_rgba.len() < expected_len {
        return Err(format!(
            "Frame data too short: expected {} bytes for {} frames, got {}",
            expected_len, frame_count, frames_rgba.len()
        ));
    }

    let mut all_rgb = Vec::new();
    for i in 0..frame_count as usize {
        let start = i * frame_rgba_size;
        let end = start + frame_rgba_size;
        let frame_rgb = extract_rgb_from_rgba(&frames_rgba[start..end], width, height)?;
        all_rgb.extend_from_slice(&frame_rgb);
    }

    decode_from_rgb(&all_rgb, password)
}

// ─── WASM-exported wrappers ───────────────────────────────────────────────────

/// WASM export: single-frame decode.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn decode_file(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
    password: &str,
) -> Result<DecodeResult, JsValue> {
    decode(rgba_pixels, width, height, password)
        .map_err(|e| JsValue::from_str(&e))
}

/// WASM export: multi-frame decode.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn decode_file_multi(
    frames_rgba: &[u8],
    frame_count: u32,
    width: u32,
    height: u32,
    password: &str,
) -> Result<DecodeResult, JsValue> {
    decode_multi(frames_rgba, frame_count, width, height, password)
        .map_err(|e| JsValue::from_str(&e))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(all(test, not(feature = "wasm")))]
mod tests {
    use super::*;
    use crate::encoder;

    fn round_trip(data: &[u8], password: &str) -> DecodeResult {
        let enc = encoder::encode_multi(data, "test.bin", "application/octet-stream", password)
            .expect("encode failed");
        let frame_count = enc.frame_count;
        decode_multi(&enc.frames_data, frame_count, enc.width, enc.height, password)
            .expect("decode failed")
    }

    #[test]
    fn round_trip_small_file() {
        let original = b"Hello, this is a small test file!";
        let result = round_trip(original, "correct-password");
        assert_eq!(result.file_bytes, original);
        assert_eq!(result.filename, "test.bin");
    }

    #[test]
    fn round_trip_large_file() {
        let original: Vec<u8> = (0..4_000_000).map(|i| (i % 251) as u8).collect();
        let result = round_trip(&original, "large-file-password");
        assert_eq!(result.file_bytes, original);
    }

    #[test]
    fn wrong_password_returns_error() {
        let data = b"Secret document contents";
        let enc = encoder::encode_multi(data, "doc.txt", "text/plain", "correct")
            .expect("encode failed");
        let err = decode_multi(&enc.frames_data, enc.frame_count, enc.width, enc.height, "wrong");
        assert!(err.is_err(), "wrong password must return an error");
        let msg = err.unwrap_err();
        assert!(
            msg.contains("Invalid password") || msg.contains("corrupted"),
            "error message should mention invalid password, got: {msg}"
        );
    }

    #[test]
    fn invalid_magic_returns_error() {
        // Build a valid RGBA frame filled with zeros (no DCVT magic)
        let fake_rgba = vec![0u8; 1024 * 1024 * 4];
        let err = decode(&fake_rgba, 1024, 1024, "password");
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("magic"), "should mention magic bytes");
    }
}
