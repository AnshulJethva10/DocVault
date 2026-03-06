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

/// Extract RGB bytes from a single RGBA frame (discards alpha channel).
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

/// Internal decode from concatenated RGB bytes.
fn decode_from_rgb(rgb_bytes: &[u8], password: &str) -> Result<DecodeResult, String> {
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
    let ciphertext_len = metadata.file_size as usize + 16;
    let ciphertext_start = METADATA_SIZE;
    let ciphertext_end = ciphertext_start + ciphertext_len;

    if rgb_bytes.len() < ciphertext_end {
        return Err("Image data truncated: not enough bytes for ciphertext".to_string());
    }

    let ciphertext = &rgb_bytes[ciphertext_start..ciphertext_end];

    // Decrypt
    let plaintext = crypto::decrypt(ciphertext, &key, &metadata.iv)?;

    // Trim to original file size
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

/// Decode a single-frame encoded image (RGBA pixel data) back to the original file.
pub fn decode(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
    password: &str,
) -> Result<DecodeResult, String> {
    let rgb_bytes = extract_rgb_from_rgba(rgba_pixels, width, height)?;
    decode_from_rgb(&rgb_bytes, password)
}

/// Decode a multi-frame encoded video back to the original file.
/// `frames_rgba` is a flat buffer containing all frames concatenated.
/// Each frame is `width * height * 4` bytes of RGBA pixel data.
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

    // Concatenate RGB bytes from all frames
    let mut all_rgb = Vec::new();
    for i in 0..frame_count as usize {
        let start = i * frame_rgba_size;
        let end = start + frame_rgba_size;
        let frame_rgb = extract_rgb_from_rgba(&frames_rgba[start..end], width, height)?;
        all_rgb.extend_from_slice(&frame_rgb);
    }

    decode_from_rgb(&all_rgb, password)
}

/// WASM-exported single-frame decode function.
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

/// WASM-exported multi-frame decode function.
/// `frames_rgba` is a flat buffer with all frames concatenated.
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
