use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::metadata::{VaultMetadata, METADATA_SIZE};

#[wasm_bindgen]
pub struct DecodeResult {
    file_bytes: Vec<u8>,
    filename: String,
    mime_type: String,
}

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

/// Decode an encoded image (RGBA pixel data) back to the original file.
///
/// Steps:
/// 1. Extract RGB bytes from RGBA pixels (discard alpha)
/// 2. Read first 276 bytes as VaultMetadata
/// 3. Validate magic bytes
/// 4. Derive AES key from password + metadata salt
/// 5. Extract ciphertext from remaining bytes
/// 6. Decrypt and trim to original file size
pub fn decode(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
    password: &str,
) -> Result<DecodeResult, String> {
    let pixel_count = (width * height) as usize;
    let expected_rgba_len = pixel_count * 4;

    if rgba_pixels.len() < expected_rgba_len {
        return Err(format!(
            "Pixel data too short: expected {} bytes, got {}",
            expected_rgba_len,
            rgba_pixels.len()
        ));
    }

    // Extract RGB bytes from RGBA (discard alpha channel)
    let mut rgb_bytes = Vec::with_capacity(pixel_count * 3);
    for i in 0..pixel_count {
        let offset = i * 4;
        rgb_bytes.push(rgba_pixels[offset]);     // R
        rgb_bytes.push(rgba_pixels[offset + 1]); // G
        rgb_bytes.push(rgba_pixels[offset + 2]); // B
    }

    // Check we have enough data for metadata
    if rgb_bytes.len() < METADATA_SIZE {
        return Err("Image too small to contain DocVault metadata".to_string());
    }

    // Extract and parse metadata
    let metadata = VaultMetadata::from_bytes(&rgb_bytes[..METADATA_SIZE])?;

    if !metadata.validate_magic() {
        return Err("Not a DocVault encoded image (invalid magic bytes)".to_string());
    }

    if metadata.version != 1 {
        return Err(format!(
            "Unsupported DocVault format version: {}",
            metadata.version
        ));
    }

    // Derive AES key from password + salt
    let key = crypto::derive_key(password.as_bytes(), &metadata.salt)?;

    // The ciphertext starts right after metadata
    // AES-256-GCM ciphertext length = original_size + 16 (GCM tag)
    let ciphertext_len = metadata.file_size as usize + 16; // 16-byte GCM auth tag
    let ciphertext_start = METADATA_SIZE;
    let ciphertext_end = ciphertext_start + ciphertext_len;

    if rgb_bytes.len() < ciphertext_end {
        return Err("Image data truncated: not enough bytes for ciphertext".to_string());
    }

    let ciphertext = &rgb_bytes[ciphertext_start..ciphertext_end];

    // Decrypt
    let plaintext = crypto::decrypt(ciphertext, &key, &metadata.iv)?;

    // Trim to original file size (should already be correct, but belt and suspenders)
    let file_size = metadata.file_size as usize;
    if plaintext.len() < file_size {
        return Err("Decrypted data shorter than expected file size".to_string());
    }

    Ok(DecodeResult {
        file_bytes: plaintext[..file_size].to_vec(),
        filename: metadata.filename_str(),
        mime_type: metadata.mime_type_str(),
    })
}

/// WASM-exported decode function.
/// Takes RGBA pixel data, dimensions, and password.
/// Returns serialized DecodeResult as JsValue.
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
