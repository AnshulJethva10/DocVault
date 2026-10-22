#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::metadata::{VaultMetadata, METADATA_SIZE, MIN_SUPPORTED_VERSION, VERSION};

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

    if metadata.version < MIN_SUPPORTED_VERSION || metadata.version > VERSION {
        return Err(format!("Unsupported DocVault format version: {}", metadata.version));
    }

    // v2+ authenticates the raw header bytes; v1 files predate that and use no AAD
    let aad: &[u8] = if metadata.version >= 2 { &rgb_bytes[..METADATA_SIZE] } else { &[] };

    let key = crypto::derive_key(password.as_bytes(), &metadata.salt)?;

    // AES-256-GCM ciphertext = plaintext + 16-byte GCM tag
    let ciphertext_len = metadata.file_size as usize + 16;
    let ciphertext_start = METADATA_SIZE;
    let ciphertext_end = ciphertext_start + ciphertext_len;

    if rgb_bytes.len() < ciphertext_end {
        return Err("Image data truncated: not enough bytes for ciphertext".to_string());
    }

    let ciphertext = &rgb_bytes[ciphertext_start..ciphertext_end];
    let plaintext = crypto::decrypt(ciphertext, &key, &metadata.iv, aad)?;

    let file_size = metadata.file_size as usize;
    if plaintext.len() < file_size {
        return Err("Decrypted data shorter than expected file size".to_string());
    }

    Ok(DecodeResult {
        file_bytes: plaintext[..file_size].to_vec(),
        filename:   metadata.safe_filename(),
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

    /// Overwrite the stored filename inside an encoded frame, the way an
    /// attacker would edit the PNG pixels without knowing the password.
    fn tamper_filename(frames_rgba: &mut [u8], name: &str) {
        let mut rgb: Vec<u8> = frames_rgba
            .chunks_exact(4)
            .take(METADATA_SIZE / 3 + 1)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let off = 4 + 1 + 8 + 32 + 12;
        let mut field = [0u8; 128];
        field[..name.len()].copy_from_slice(name.as_bytes());
        rgb[off..off + 128].copy_from_slice(&field);
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            frames_rgba[i * 4..i * 4 + 3].copy_from_slice(px);
        }
    }

    #[test]
    fn tampered_metadata_fails_authentication() {
        let mut enc = encoder::encode_multi(b"payload", "doc.txt", "text/plain", "pw")
            .expect("encode failed");
        tamper_filename(&mut enc.frames_data, "../../evil.sh");
        let err = decode_multi(&enc.frames_data, enc.frame_count, enc.width, enc.height, "pw");
        assert!(err.is_err(), "tampered header must not decrypt");
    }

    /// Build a v1 (pre-AAD) vault frame by hand to check backward compatibility.
    fn encode_v1(data: &[u8], filename: &str, password: &str) -> Vec<u8> {
        let salt = [7u8; 32];
        let iv = [9u8; 12];
        let key = crypto::derive_key(password.as_bytes(), &salt).unwrap();
        let ciphertext = crypto::encrypt(data, &key, &iv, &[]).unwrap();
        let mut meta = VaultMetadata::new(filename, "text/plain", data.len() as u64, salt, iv);
        meta.version = 1;
        let mut rgb = meta.to_bytes();
        rgb.extend_from_slice(&ciphertext);
        let mut rgba = vec![0u8; 1024 * 1024 * 4];
        for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
            for (c, channel) in px[..3].iter_mut().enumerate() {
                *channel = rgb.get(i * 3 + c).copied().unwrap_or(0);
            }
            px[3] = 255;
        }
        rgba
    }

    #[test]
    fn v1_files_still_decode() {
        let rgba = encode_v1(b"legacy contents", "old.txt", "pw");
        let result = decode(&rgba, 1024, 1024, "pw").expect("v1 decode failed");
        assert_eq!(result.file_bytes, b"legacy contents");
        assert_eq!(result.filename, "old.txt");
    }

    #[test]
    fn v1_traversal_filename_is_sanitized() {
        // v1 headers are unauthenticated, so the filename must be cleaned on decode
        let rgba = encode_v1(b"x", "../../.bashrc", "pw");
        let result = decode(&rgba, 1024, 1024, "pw").expect("v1 decode failed");
        assert_eq!(result.filename, ".bashrc");
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
