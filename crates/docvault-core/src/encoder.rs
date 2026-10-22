#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::metadata::{VaultMetadata, METADATA_SIZE};

/// Fixed canvas dimensions for encoded frames.
const FRAME_WIDTH: u32 = 1024;
const FRAME_HEIGHT: u32 = 1024;
/// Total pixels per frame.
const PIXELS_PER_FRAME: usize = (FRAME_WIDTH * FRAME_HEIGHT) as usize;
/// Total RGB bytes per frame (3 bytes per pixel).
const BYTES_PER_FRAME: usize = PIXELS_PER_FRAME * 3;

// ─── EncodeResult ─────────────────────────────────────────────────────────────

/// Result for a single-frame encode.
/// WASM build: exposes rgba_pixels as a Uint8Array getter.
/// Native build: exposes rgba_pixels as a plain public field.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct EncodeResult {
    rgba_pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl EncodeResult {
    #[wasm_bindgen(getter)]
    pub fn rgba_pixels(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(&self.rgba_pixels[..])
    }
}

#[cfg(not(feature = "wasm"))]
pub struct EncodeResult {
    pub rgba_pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

// ─── MultiFrameEncodeResult ───────────────────────────────────────────────────

/// Result for multi-frame encoding — one or more 1024×1024 RGBA frames.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct MultiFrameEncodeResult {
    /// Flat buffer: all frames concatenated (each frame = PIXELS_PER_FRAME × 4 bytes).
    frames_data: Vec<u8>,
    pub frame_count: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl MultiFrameEncodeResult {
    /// Get the RGBA pixel data for a specific frame (0-indexed).
    pub fn get_frame(&self, index: u32) -> Result<js_sys::Uint8Array, JsValue> {
        if index >= self.frame_count {
            return Err(JsValue::from_str(&format!(
                "Frame index {} out of range (total: {})",
                index, self.frame_count
            )));
        }
        let frame_size = PIXELS_PER_FRAME * 4;
        let start = index as usize * frame_size;
        let end = start + frame_size;
        Ok(js_sys::Uint8Array::from(&self.frames_data[start..end]))
    }
}

#[cfg(not(feature = "wasm"))]
pub struct MultiFrameEncodeResult {
    /// Flat buffer: all frames concatenated (each frame = PIXELS_PER_FRAME × 4 bytes).
    pub frames_data: Vec<u8>,
    pub frame_count: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(not(feature = "wasm"))]
impl MultiFrameEncodeResult {
    /// Get the RGBA pixel data for a specific frame (0-indexed).
    /// Returns a slice into `frames_data` for zero-copy access.
    pub fn get_frame_rgba(&self, index: u32) -> Option<&[u8]> {
        if index >= self.frame_count {
            return None;
        }
        let frame_size = PIXELS_PER_FRAME * 4;
        let start = index as usize * frame_size;
        let end = start + frame_size;
        Some(&self.frames_data[start..end])
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Convert a slice of RGB bytes into a full 1024×1024 RGBA pixel buffer.
/// Pads with black (0, 0, 0, 255) if data is shorter than a full frame.
fn rgb_to_rgba_frame(rgb_data: &[u8]) -> Vec<u8> {
    let mut rgba = vec![0u8; PIXELS_PER_FRAME * 4];
    for i in 0..PIXELS_PER_FRAME {
        let byte_offset = i * 3;
        let pixel_offset = i * 4;

        rgba[pixel_offset]     = if byte_offset     < rgb_data.len() { rgb_data[byte_offset]     } else { 0 };
        rgba[pixel_offset + 1] = if byte_offset + 1 < rgb_data.len() { rgb_data[byte_offset + 1] } else { 0 };
        rgba[pixel_offset + 2] = if byte_offset + 2 < rgb_data.len() { rgb_data[byte_offset + 2] } else { 0 };
        rgba[pixel_offset + 3] = 255; // alpha = opaque
    }
    rgba
}

// ─── Public pure-Rust API (usable from both WASM and native) ─────────────────

/// Encode a file into one or more 1024×1024 RGBA frames.
///
/// Layout:
/// - Frame 0: First `METADATA_SIZE` bytes = `VaultMetadata`, followed by ciphertext
/// - Frame 1..N: Continuation of ciphertext
/// - All frames are exactly 1024×1024 pixels, zero-padded to fill RGB triplets
pub fn encode_multi(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<MultiFrameEncodeResult, String> {
    let salt: [u8; 32] = crypto::generate_random_bytes(32)
        .try_into()
        .map_err(|_| "Failed to generate salt")?;
    let iv: [u8; 12] = crypto::generate_random_bytes(12)
        .try_into()
        .map_err(|_| "Failed to generate IV")?;

    let key = crypto::derive_key(password.as_bytes(), &salt)?;

    // AES-256-GCM ciphertext = plaintext + 16-byte tag, so the frame count is
    // known before encrypting — the metadata must be final before it is used as AAD.
    let total_data_len = METADATA_SIZE + file_bytes.len() + 16;
    let frame_count = ((total_data_len as f64) / (BYTES_PER_FRAME as f64)).ceil() as u16;

    let metadata = VaultMetadata::new_with_frames(
        filename,
        mime_type,
        file_bytes.len() as u64,
        salt,
        iv,
        frame_count,
    );
    let metadata_bytes = metadata.to_bytes();
    debug_assert_eq!(metadata_bytes.len(), METADATA_SIZE);

    // Authenticate the metadata header so it cannot be altered undetected
    let ciphertext = crypto::encrypt(file_bytes, &key, &iv, &metadata_bytes)?;
    debug_assert_eq!(METADATA_SIZE + ciphertext.len(), total_data_len);

    let mut all_bytes = Vec::with_capacity(total_data_len);
    all_bytes.extend_from_slice(&metadata_bytes);
    all_bytes.extend_from_slice(&ciphertext);

    // Pad to a multiple of 3 for RGB triplet alignment
    while all_bytes.len() % 3 != 0 {
        all_bytes.push(0x00);
    }

    let mut frames_data = Vec::with_capacity(frame_count as usize * PIXELS_PER_FRAME * 4);
    for i in 0..frame_count as usize {
        let start = i * BYTES_PER_FRAME;
        let end = (start + BYTES_PER_FRAME).min(all_bytes.len());
        let chunk = if start < all_bytes.len() { &all_bytes[start..end] } else { &[] };
        frames_data.extend_from_slice(&rgb_to_rgba_frame(chunk));
    }

    Ok(MultiFrameEncodeResult {
        frames_data,
        frame_count: frame_count as u32,
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
    })
}

/// Single-frame encode — convenience wrapper over `encode_multi`.
pub fn encode(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<EncodeResult, String> {
    let multi = encode_multi(file_bytes, filename, mime_type, password)?;
    let frame_size = PIXELS_PER_FRAME * 4;
    let rgba_pixels = multi.frames_data[..frame_size].to_vec();
    Ok(EncodeResult { rgba_pixels, width: multi.width, height: multi.height })
}

// ─── WASM-exported wrappers ───────────────────────────────────────────────────

/// WASM export: multi-frame encode.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn encode_file_multi(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<MultiFrameEncodeResult, JsValue> {
    encode_multi(file_bytes, filename, mime_type, password)
        .map_err(|e| JsValue::from_str(&e))
}

/// WASM export: single-frame encode (legacy).
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn encode_file(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<EncodeResult, JsValue> {
    encode(file_bytes, filename, mime_type, password)
        .map_err(|e| JsValue::from_str(&e))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(all(test, not(feature = "wasm")))]
mod tests {
    use super::*;

    #[test]
    fn encode_multi_produces_rgba_frames() {
        let data = b"Hello, DocVault!";
        let result = encode_multi(data, "test.txt", "text/plain", "password123").unwrap();
        assert_eq!(result.frame_count, 1, "small file should fit in 1 frame");
        assert_eq!(result.width, 1024);
        assert_eq!(result.height, 1024);
        let expected_frame_size = 1024 * 1024 * 4;
        assert_eq!(result.frames_data.len(), expected_frame_size);
    }

    #[test]
    fn encode_multi_large_file_produces_multiple_frames() {
        // > 3 MB → must produce > 1 frame
        let data = vec![0xABu8; 4 * 1024 * 1024];
        let result = encode_multi(&data, "big.bin", "application/octet-stream", "pw").unwrap();
        assert!(result.frame_count > 1, "4 MB file should produce multiple frames");
    }

    #[test]
    fn get_frame_rgba_returns_correct_slice() {
        let data = vec![0x42u8; 4 * 1024 * 1024];
        let result = encode_multi(&data, "big.bin", "application/octet-stream", "pw").unwrap();
        let frame = result.get_frame_rgba(0).expect("frame 0 should exist");
        assert_eq!(frame.len(), 1024 * 1024 * 4);
        assert!(result.get_frame_rgba(result.frame_count).is_none());
    }
}
