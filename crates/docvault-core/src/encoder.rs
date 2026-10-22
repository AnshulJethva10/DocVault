#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::metadata::{self, VaultMetadata, METADATA_SIZE, PBKDF2_ITERATIONS};

/// Fixed canvas dimensions for encoded frames.
pub const FRAME_WIDTH: u32 = 1024;
pub const FRAME_HEIGHT: u32 = 1024;
/// Total pixels per frame.
pub const PIXELS_PER_FRAME: usize = (FRAME_WIDTH * FRAME_HEIGHT) as usize;
/// Total RGB bytes per frame (3 bytes per pixel).
pub const BYTES_PER_FRAME: usize = PIXELS_PER_FRAME * 3;

const TOO_LARGE: &str = "File is too large to encode";

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

// ─── PngEncodeResult (WASM) ───────────────────────────────────────────────────

/// Encoded vault as one lossless PNG per frame, ready to save or mux into AVI.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct PngEncodeResult {
    frames: Vec<Vec<u8>>,
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl PngEncodeResult {
    #[wasm_bindgen(getter)]
    pub fn frame_count(&self) -> u32 {
        self.frames.len() as u32
    }

    /// PNG file bytes for frame `index` (0-indexed).
    pub fn get_frame_png(&self, index: u32) -> Result<js_sys::Uint8Array, JsValue> {
        self.frames
            .get(index as usize)
            .map(|f| js_sys::Uint8Array::from(&f[..]))
            .ok_or_else(|| JsValue::from_str(&format!("Frame index {index} out of range")))
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Encrypt a file into the vault's packed RGB stream, padded with zeros to a
/// whole number of frames. Returns the stream and its frame count.
///
/// Layout: `VaultMetadata` (276 bytes, authenticated as AAD) ‖ AES-GCM
/// ciphertext of (inner header ‖ file) ‖ 16-byte tag ‖ zero padding.
///
/// Everything happens in one buffer allocated at its final size, so peak
/// memory is the input plus one copy rather than several.
fn seal(file_bytes: &[u8], filename: &str, mime_type: &str, password: &str) -> Result<(Vec<u8>, u16), String> {
    let inner = metadata::inner_header(filename, mime_type);
    let payload_len = inner.len().checked_add(file_bytes.len()).ok_or(TOO_LARGE)?;
    let used = METADATA_SIZE
        .checked_add(payload_len)
        .and_then(|n| n.checked_add(crypto::TAG_LEN))
        .ok_or(TOO_LARGE)?;
    // The frame count is part of the authenticated header, so fix it before encrypting
    let frame_count = u16::try_from(used.div_ceil(BYTES_PER_FRAME))
        .map_err(|_| format!("{TOO_LARGE}: a vault holds at most {} frames", u16::MAX))?;
    let total_len = (frame_count as usize).checked_mul(BYTES_PER_FRAME).ok_or(TOO_LARGE)?;

    let salt: [u8; 32] = crypto::generate_random_bytes(32)
        .try_into()
        .map_err(|_| "Failed to generate salt")?;
    let iv: [u8; 12] = crypto::generate_random_bytes(12)
        .try_into()
        .map_err(|_| "Failed to generate IV")?;
    let key = crypto::derive_key(password.as_bytes(), &salt, PBKDF2_ITERATIONS)?;

    let header = VaultMetadata::new_v3(payload_len as u64, salt, iv, frame_count, PBKDF2_ITERATIONS);

    let mut buf = Vec::with_capacity(total_len);
    buf.extend_from_slice(&header.to_bytes());
    buf.extend_from_slice(&inner);
    buf.extend_from_slice(file_bytes);
    let (aad, payload) = buf.split_at_mut(METADATA_SIZE);
    let tag = crypto::encrypt_in_place(payload, &key, &iv, aad)?;
    buf.extend_from_slice(&tag);
    buf.resize(total_len, 0);
    Ok((buf, frame_count))
}

/// Expand packed RGB into opaque RGBA.
fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for p in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
    }
    rgba
}

// ─── Public pure-Rust API (usable from both WASM and native) ─────────────────

/// Encode a file into one or more 1024×1024 RGBA frames.
///
/// - Frame 0: first `METADATA_SIZE` bytes = `VaultMetadata`, followed by ciphertext
/// - Frame 1..N: continuation of ciphertext
/// - All frames are exactly 1024×1024 pixels; the last is zero-padded
pub fn encode_multi(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<MultiFrameEncodeResult, String> {
    let (rgb, frame_count) = seal(file_bytes, filename, mime_type, password)?;
    Ok(MultiFrameEncodeResult {
        frames_data: rgb_to_rgba(&rgb),
        frame_count: frame_count as u32,
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
    })
}

/// Encode a file into lossless RGB PNGs, one per frame.
///
/// This is the preferred path for every front end: it never routes pixels
/// through a browser canvas, which privacy modes are allowed to alter.
pub fn encode_png_frames(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<Vec<Vec<u8>>, String> {
    let (rgb, _) = seal(file_bytes, filename, mime_type, password)?;
    rgb.chunks_exact(BYTES_PER_FRAME)
        .map(|frame| crate::png::encode_rgb(frame, FRAME_WIDTH, FRAME_HEIGHT))
        .collect()
}

/// Single-frame encode. Fails if the file needs more than one frame —
/// returning only the first frame would silently lose the rest of the file.
pub fn encode(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<EncodeResult, String> {
    let multi = encode_multi(file_bytes, filename, mime_type, password)?;
    if multi.frame_count != 1 {
        return Err(format!(
            "File needs {} frames and cannot fit in a single image; use multi-frame encoding",
            multi.frame_count
        ));
    }
    Ok(EncodeResult { rgba_pixels: multi.frames_data, width: multi.width, height: multi.height })
}

// ─── WASM-exported wrappers ───────────────────────────────────────────────────

/// WASM export: encode straight to PNG frames (no canvas involved).
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn encode_file_png(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<PngEncodeResult, JsValue> {
    encode_png_frames(file_bytes, filename, mime_type, password)
        .map(|frames| PngEncodeResult { frames })
        .map_err(|e| JsValue::from_str(&e))
}

/// WASM export: multi-frame encode to raw RGBA.
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

    #[test]
    fn single_frame_encode_refuses_multi_frame_files() {
        let data = vec![0u8; 4 * 1024 * 1024];
        let err = encode(&data, "big.bin", "application/octet-stream", "pw").err().unwrap();
        assert!(err.contains("frames"), "{err}");
    }

    #[test]
    fn frame_boundary_sizes_get_the_right_frame_count() {
        // inner header for "f" + "x/y" is 2 + 1 + 1 + 3 = 7 bytes
        let fits = BYTES_PER_FRAME - METADATA_SIZE - crypto::TAG_LEN - 7;
        let one = encode_multi(&vec![1u8; fits], "f", "x/y", "pw").unwrap();
        assert_eq!(one.frame_count, 1);
        let two = encode_multi(&vec![1u8; fits + 1], "f", "x/y", "pw").unwrap();
        assert_eq!(two.frame_count, 2);
    }

    #[test]
    fn header_does_not_reveal_filename() {
        let r = encode_multi(b"x", "Payroll_2026.xlsx", "application/vnd.ms-excel", "pw").unwrap();
        let rgb: Vec<u8> = r.frames_data.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let needle = b"Payroll";
        assert!(!rgb.windows(needle.len()).any(|w| w == needle));
    }
}
