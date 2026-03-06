use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::metadata::{VaultMetadata, METADATA_SIZE};

/// Fixed canvas dimensions for encoded frames
const FRAME_WIDTH: u32 = 1024;
const FRAME_HEIGHT: u32 = 1024;
/// Total pixels per frame
const PIXELS_PER_FRAME: usize = (FRAME_WIDTH * FRAME_HEIGHT) as usize;
/// Total RGB bytes per frame (3 bytes per pixel)
const BYTES_PER_FRAME: usize = PIXELS_PER_FRAME * 3;

#[wasm_bindgen]
pub struct EncodeResult {
    rgba_pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[wasm_bindgen]
impl EncodeResult {
    #[wasm_bindgen(getter)]
    pub fn rgba_pixels(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(&self.rgba_pixels[..])
    }
}

/// Result for multi-frame encoding.
/// Contains one or more 1024×1024 RGBA frames.
#[wasm_bindgen]
pub struct MultiFrameEncodeResult {
    /// Flat buffer: all frames concatenated (each frame is PIXELS_PER_FRAME * 4 bytes)
    frames_data: Vec<u8>,
    pub frame_count: u32,
    pub width: u32,
    pub height: u32,
}

#[wasm_bindgen]
impl MultiFrameEncodeResult {
    /// Get the RGBA pixel data for a specific frame (0-indexed)
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

/// Convert a slice of RGB bytes into a full 1024×1024 RGBA pixel buffer.
/// Pads with black (0,0,0,255) if data is shorter than a full frame.
fn rgb_to_rgba_frame(rgb_data: &[u8]) -> Vec<u8> {
    let mut rgba = vec![0u8; PIXELS_PER_FRAME * 4];
    for i in 0..PIXELS_PER_FRAME {
        let byte_offset = i * 3;
        let pixel_offset = i * 4;

        rgba[pixel_offset] = if byte_offset < rgb_data.len() {
            rgb_data[byte_offset]
        } else {
            0
        };
        rgba[pixel_offset + 1] = if byte_offset + 1 < rgb_data.len() {
            rgb_data[byte_offset + 1]
        } else {
            0
        };
        rgba[pixel_offset + 2] = if byte_offset + 2 < rgb_data.len() {
            rgb_data[byte_offset + 2]
        } else {
            0
        };
        rgba[pixel_offset + 3] = 255;
    }
    rgba
}

/// Encode a file into one or more 1024×1024 RGBA frames.
///
/// Layout per frame:
/// - Frame 0: First METADATA_SIZE bytes = VaultMetadata, then encrypted data
/// - Frame 1..N: Continuation of encrypted data
/// - All frames are exactly 1024×1024 pixels, padded with black
pub fn encode_multi(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<MultiFrameEncodeResult, String> {
    // Generate random salt and IV
    let salt: [u8; 32] = crypto::generate_random_bytes(32)
        .try_into()
        .map_err(|_| "Failed to generate salt".to_string())?;
    let iv: [u8; 12] = crypto::generate_random_bytes(12)
        .try_into()
        .map_err(|_| "Failed to generate IV".to_string())?;

    // Derive AES-256 key from password + salt
    let key = crypto::derive_key(password.as_bytes(), &salt)?;

    // Encrypt the file bytes
    let ciphertext = crypto::encrypt(file_bytes, &key, &iv)?;

    // Combine metadata + ciphertext
    let total_data_len = METADATA_SIZE + ciphertext.len();

    // Calculate number of frames needed
    let frame_count = ((total_data_len as f64) / (BYTES_PER_FRAME as f64)).ceil() as u16;

    // Build metadata with frame count
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

    // Build the full byte stream
    let mut all_bytes = Vec::with_capacity(total_data_len);
    all_bytes.extend_from_slice(&metadata_bytes);
    all_bytes.extend_from_slice(&ciphertext);

    // Pad to a multiple of 3 for RGB triplet alignment
    while all_bytes.len() % 3 != 0 {
        all_bytes.push(0x00);
    }

    // Split into frames
    let frame_size_bytes = BYTES_PER_FRAME; // RGB bytes per frame
    let mut frames_data = Vec::with_capacity(frame_count as usize * PIXELS_PER_FRAME * 4);

    for i in 0..frame_count as usize {
        let start = i * frame_size_bytes;
        let end = (start + frame_size_bytes).min(all_bytes.len());
        let chunk = if start < all_bytes.len() {
            &all_bytes[start..end]
        } else {
            &[]
        };
        let rgba_frame = rgb_to_rgba_frame(chunk);
        frames_data.extend_from_slice(&rgba_frame);
    }

    Ok(MultiFrameEncodeResult {
        frames_data,
        frame_count: frame_count as u32,
        width: FRAME_WIDTH,
        height: FRAME_HEIGHT,
    })
}

/// Legacy single-frame encode for backward compatibility.
/// Uses the first frame from multi-frame encoding.
pub fn encode(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<EncodeResult, String> {
    let multi = encode_multi(file_bytes, filename, mime_type, password)?;
    // For single-frame case, extract the first (and only) frame
    let frame_size = PIXELS_PER_FRAME * 4;
    let rgba_pixels = multi.frames_data[..frame_size].to_vec();
    Ok(EncodeResult {
        rgba_pixels,
        width: multi.width,
        height: multi.height,
    })
}

/// WASM-exported multi-frame encode function.
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

/// WASM-exported single-frame encode function (legacy).
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
