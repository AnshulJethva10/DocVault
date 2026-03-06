use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::metadata::{VaultMetadata, METADATA_SIZE};

/// Fixed canvas width for encoded images
const CANVAS_WIDTH: u32 = 1024;

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

/// Encode a file into RGBA pixel data suitable for rendering onto a canvas.
///
/// Layout:
/// - First 92 pixels (276 bytes): VaultMetadata
/// - Remaining pixels: encrypted file bytes packed as RGB triplets
/// - Alpha channel is always 255
/// - Padding with 0x00 if total bytes not divisible by 3
pub fn encode(
    file_bytes: &[u8],
    filename: &str,
    mime_type: &str,
    password: &str,
) -> Result<EncodeResult, String> {
    // Generate random salt (32 bytes) and IV/nonce (12 bytes)
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

    // Build metadata
    let metadata = VaultMetadata::new(filename, mime_type, file_bytes.len() as u64, salt, iv);
    let metadata_bytes = metadata.to_bytes();
    debug_assert_eq!(metadata_bytes.len(), METADATA_SIZE);

    // Combine metadata + ciphertext into one byte stream
    let mut all_bytes = Vec::with_capacity(metadata_bytes.len() + ciphertext.len());
    all_bytes.extend_from_slice(&metadata_bytes);
    all_bytes.extend_from_slice(&ciphertext);

    // Pad to a multiple of 3 for RGB triplet alignment
    while all_bytes.len() % 3 != 0 {
        all_bytes.push(0x00);
    }

    // Calculate dimensions
    let total_pixels = all_bytes.len() / 3;
    let width = CANVAS_WIDTH;
    let height = ((total_pixels as f64) / (width as f64)).ceil() as u32;
    let height = height.max(1); // at least 1 row

    // Convert bytes to RGBA pixel array
    let pixel_count = (width * height) as usize;
    let mut rgba = vec![0u8; pixel_count * 4];

    for i in 0..pixel_count {
        let byte_offset = i * 3;
        let pixel_offset = i * 4;

        // R, G, B from data bytes (or 0 if beyond data)
        rgba[pixel_offset] = if byte_offset < all_bytes.len() {
            all_bytes[byte_offset]
        } else {
            0
        };
        rgba[pixel_offset + 1] = if byte_offset + 1 < all_bytes.len() {
            all_bytes[byte_offset + 1]
        } else {
            0
        };
        rgba[pixel_offset + 2] = if byte_offset + 2 < all_bytes.len() {
            all_bytes[byte_offset + 2]
        } else {
            0
        };
        // Alpha always 255
        rgba[pixel_offset + 3] = 255;
    }

    Ok(EncodeResult {
        rgba_pixels: rgba,
        width,
        height,
    })
}

/// WASM-exported encode function.
/// Takes file bytes, filename, MIME type, and password.
/// Returns serialized EncodeResult as JsValue.
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
