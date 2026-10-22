#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::crypto;
use crate::encoder::{BYTES_PER_FRAME, FRAME_HEIGHT, FRAME_WIDTH, PIXELS_PER_FRAME};
use crate::metadata::{
    self, VaultMetadata, MAX_PBKDF2_ITERATIONS, METADATA_SIZE, MIN_PBKDF2_ITERATIONS,
    MIN_SUPPORTED_VERSION, VERSION,
};

const TRUNCATED: &str = "Image data truncated: not enough bytes for ciphertext";

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

/// Reject frames that are not the fixed vault size before touching their pixels.
fn check_dimensions(width: u32, height: u32) -> Result<(), String> {
    if (width, height) != (FRAME_WIDTH, FRAME_HEIGHT) {
        return Err(format!(
            "Image is {width}×{height}, but DocVault frames are {FRAME_WIDTH}×{FRAME_HEIGHT} \
             (was it resized or re-compressed by another app?)"
        ));
    }
    Ok(())
}

/// Extract packed RGB from `frame_count` concatenated RGBA frames (alpha discarded).
fn extract_rgb_from_rgba(rgba_pixels: &[u8], frame_count: usize) -> Result<Vec<u8>, String> {
    let expected_rgba_len = frame_count
        .checked_mul(PIXELS_PER_FRAME * 4)
        .ok_or("Too many frames")?;
    if rgba_pixels.len() < expected_rgba_len {
        return Err(format!(
            "Pixel data too short: expected {} bytes for {} frame(s), got {}",
            expected_rgba_len,
            frame_count,
            rgba_pixels.len()
        ));
    }
    let mut rgb = Vec::with_capacity(frame_count * BYTES_PER_FRAME);
    for p in rgba_pixels[..expected_rgba_len].chunks_exact(4) {
        rgb.extend_from_slice(&p[..3]);
    }
    Ok(rgb)
}

/// Header checks that need no password: magic and supported version.
fn check_header(metadata: &VaultMetadata) -> Result<(), String> {
    if !metadata.validate_magic() {
        return Err("Not a DocVault encoded image (invalid magic bytes)".to_string());
    }
    if metadata.version > VERSION {
        return Err(format!(
            "This vault was created by a newer version of DocVault (format v{}); \
             please update DocVault to open it",
            metadata.version
        ));
    }
    if metadata.version < MIN_SUPPORTED_VERSION {
        return Err(format!("Unsupported DocVault format version: {}", metadata.version));
    }
    Ok(())
}

/// Core decode from the concatenated RGB of every frame.
///
/// Every header field is validated *before* the (deliberately slow) key
/// derivation, and all arithmetic on header values is checked: the header is
/// attacker-controlled until the GCM tag has been verified. Decryption runs in
/// place and the plaintext is shifted to the front of `rgb`, so no second
/// copy of the file is made.
fn open_vault(mut rgb: Vec<u8>, frames_supplied: usize, password: &str) -> Result<DecodeResult, String> {
    if rgb.len() < METADATA_SIZE {
        return Err("Image too small to contain DocVault metadata".to_string());
    }
    let metadata = VaultMetadata::from_bytes(&rgb[..METADATA_SIZE])?;
    check_header(&metadata)?;

    let expected_frames = metadata.total_frames as usize;
    if expected_frames != frames_supplied {
        return Err(format!(
            "This vault has {expected_frames} frame(s) but {frames_supplied} were provided; \
             make sure the complete vault file is loaded"
        ));
    }

    let iterations = metadata.iterations();
    if !(MIN_PBKDF2_ITERATIONS..=MAX_PBKDF2_ITERATIONS).contains(&iterations) {
        return Err("Corrupted vault: invalid key-derivation settings".to_string());
    }

    // AES-256-GCM ciphertext = plaintext + 16-byte GCM tag
    let ciphertext_len = usize::try_from(metadata.file_size)
        .ok()
        .and_then(|n| n.checked_add(crypto::TAG_LEN))
        .ok_or(TRUNCATED)?;
    let ciphertext_end = METADATA_SIZE.checked_add(ciphertext_len).ok_or(TRUNCATED)?;
    if rgb.len() < ciphertext_end {
        return Err(TRUNCATED.to_string());
    }

    let key = crypto::derive_key(password.as_bytes(), &metadata.salt, iterations)?;

    let (header_bytes, body) = rgb.split_at_mut(METADATA_SIZE);
    // v2+ authenticates the raw header bytes; v1 files predate that and use no AAD
    let aad: &[u8] = if metadata.version >= 2 { header_bytes } else { &[] };
    let plain_len = crypto::decrypt_in_place(&mut body[..ciphertext_len], &key, &metadata.iv, aad)?;

    let (filename, mime_type, offset) = if metadata.version >= 3 {
        let (name, mime, offset) = metadata::parse_inner_header(&body[..plain_len])?;
        (metadata::sanitize_filename(&name), mime, offset)
    } else {
        (metadata.safe_filename(), metadata.mime_type_str(), 0)
    };

    let start = METADATA_SIZE + offset;
    let end = METADATA_SIZE + plain_len;
    rgb.copy_within(start..end, 0);
    rgb.truncate(end - start);

    Ok(DecodeResult { file_bytes: rgb, filename, mime_type })
}

// ─── Public pure-Rust API ─────────────────────────────────────────────────────

/// Decode a single-frame vault image (RGBA pixel data) back to the original file.
pub fn decode(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
    password: &str,
) -> Result<DecodeResult, String> {
    check_dimensions(width, height)?;
    let rgb = extract_rgb_from_rgba(rgba_pixels, 1)?;
    open_vault(rgb, 1, password)
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
    check_dimensions(width, height)?;
    let rgb = extract_rgb_from_rgba(frames_rgba, frame_count as usize)?;
    open_vault(rgb, frame_count as usize, password)
}

/// Incremental decoder fed one PNG frame at a time.
///
/// The first frame's header is checked as soon as it arrives, so a file that
/// is not a vault (or is from a newer version) is rejected before the rest of
/// a large video is decoded. Only packed RGB is kept, never RGBA.
#[derive(Default)]
pub struct PngVaultDecoder {
    rgb: Vec<u8>,
    frames: usize,
    expected_frames: usize,
}

impl PngVaultDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode one PNG frame and append its pixels.
    pub fn push_frame(&mut self, png: &[u8]) -> Result<(), String> {
        if self.frames > 0 && self.frames >= self.expected_frames {
            return Err(format!("Too many frames: this vault has {}", self.expected_frames));
        }
        let frame = crate::png::decode_rgb(png, FRAME_WIDTH, FRAME_HEIGHT)?;
        if self.frames == 0 {
            let metadata = VaultMetadata::from_bytes(&frame[..METADATA_SIZE])?;
            check_header(&metadata)?;
            self.expected_frames = metadata.total_frames as usize;
            self.rgb = Vec::with_capacity(self.expected_frames.saturating_mul(BYTES_PER_FRAME));
        }
        self.rgb.extend_from_slice(&frame);
        self.frames += 1;
        Ok(())
    }

    /// Verify the password and return the original file.
    pub fn finish(&mut self, password: &str) -> Result<DecodeResult, String> {
        if self.frames == 0 {
            return Err("No frames were provided".to_string());
        }
        let frames = std::mem::take(&mut self.frames);
        open_vault(std::mem::take(&mut self.rgb), frames, password)
    }
}

/// Decode a vault from its PNG frames (one PNG per frame, in order).
pub fn decode_png_frames<T: AsRef<[u8]>>(pngs: &[T], password: &str) -> Result<DecodeResult, String> {
    let mut decoder = PngVaultDecoder::new();
    for png in pngs {
        decoder.push_frame(png.as_ref())?;
    }
    decoder.finish(password)
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

/// WASM export: decode PNG frames without a canvas.
/// `new PngFrameDecoder()`, `push_frame(pngBytes)` per frame, then `finish(password)`.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub struct PngFrameDecoder(PngVaultDecoder);

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl PngFrameDecoder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(PngVaultDecoder::new())
    }

    pub fn push_frame(&mut self, png: &[u8]) -> Result<(), JsValue> {
        self.0.push_frame(png).map_err(|e| JsValue::from_str(&e))
    }

    pub fn finish(&mut self, password: &str) -> Result<DecodeResult, JsValue> {
        self.0.finish(password).map_err(|e| JsValue::from_str(&e))
    }
}

#[cfg(feature = "wasm")]
impl Default for PngFrameDecoder {
    fn default() -> Self {
        Self::new()
    }
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

    fn rgb_to_frame(rgb: &[u8]) -> Vec<u8> {
        let mut rgba = vec![0u8; PIXELS_PER_FRAME * 4];
        for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
            for (c, channel) in px[..3].iter_mut().enumerate() {
                *channel = rgb.get(i * 3 + c).copied().unwrap_or(0);
            }
            px[3] = 255;
        }
        rgba
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
    fn round_trip_keeps_long_and_unicode_names() {
        let long = format!("{}_FINAL_signed.pdf", "Quarterly_Board_Report_".repeat(5));
        let docx = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        let enc = encoder::encode_multi(b"x", &long, docx, "pw").unwrap();
        let r = decode_multi(&enc.frames_data, 1, 1024, 1024, "pw").unwrap();
        assert_eq!(r.filename, long);
        assert_eq!(r.mime_type, docx);

        let deva = format!("{}.pdf", "दस्तावेज़".repeat(6));
        let enc = encoder::encode_multi(b"x", &deva, "application/pdf", "pw").unwrap();
        let r = decode_multi(&enc.frames_data, 1, 1024, 1024, "pw").unwrap();
        assert_eq!(r.filename, deva);
    }

    #[test]
    fn png_frames_round_trip() {
        let original: Vec<u8> = (0..4_000_000).map(|i| (i * 7 % 256) as u8).collect();
        let pngs = encoder::encode_png_frames(&original, "a.bin", "application/octet-stream", "pw").unwrap();
        assert_eq!(pngs.len(), 2);
        let r = decode_png_frames(&pngs, "pw").unwrap();
        assert_eq!(r.file_bytes, original);
        assert_eq!(r.filename, "a.bin");
    }

    #[test]
    fn missing_frames_are_reported_clearly() {
        let data = vec![1u8; 4 * 1024 * 1024];
        let pngs = encoder::encode_png_frames(&data, "a.bin", "application/octet-stream", "pw").unwrap();
        let err = decode_png_frames(&pngs[..1], "pw").unwrap_err();
        assert!(err.contains("2 frame(s) but 1"), "{err}");
    }

    #[test]
    fn wrong_password_returns_error() {
        let data = b"Secret document contents";
        let enc = encoder::encode_multi(data, "doc.txt", "text/plain", "correct")
            .expect("encode failed");
        let err = decode_multi(&enc.frames_data, enc.frame_count, enc.width, enc.height, "wrong");
        assert_eq!(err.unwrap_err(), crypto::AUTH_FAILED);
    }

    /// Overwrite a header byte range inside an encoded frame, the way an
    /// attacker would edit the PNG pixels without knowing the password.
    fn tamper_header(frames_rgba: &mut [u8], offset: usize, bytes: &[u8]) {
        let mut rgb: Vec<u8> = frames_rgba
            .chunks_exact(4)
            .take(METADATA_SIZE / 3 + 1)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        rgb[offset..offset + bytes.len()].copy_from_slice(bytes);
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            frames_rgba[i * 4..i * 4 + 3].copy_from_slice(px);
        }
    }

    #[test]
    fn tampered_metadata_fails_authentication() {
        let mut enc = encoder::encode_multi(b"payload", "doc.txt", "text/plain", "pw")
            .expect("encode failed");
        tamper_header(&mut enc.frames_data, 4 + 1 + 8 + 32 + 12, b"../../evil.sh");
        let err = decode_multi(&enc.frames_data, enc.frame_count, enc.width, enc.height, "pw");
        assert!(err.is_err(), "tampered header must not decrypt");
    }

    #[test]
    fn huge_file_size_is_rejected_without_panicking() {
        let mut enc = encoder::encode_multi(b"payload", "doc.txt", "text/plain", "pw").unwrap();
        tamper_header(&mut enc.frames_data, 5, &u64::MAX.to_le_bytes());
        let err = decode(&enc.frames_data, 1024, 1024, "pw").unwrap_err();
        assert!(err.contains("truncated"), "{err}");
    }

    #[test]
    fn absurd_iteration_count_is_rejected_before_kdf() {
        let mut enc = encoder::encode_multi(b"payload", "doc.txt", "text/plain", "pw").unwrap();
        tamper_header(&mut enc.frames_data, METADATA_SIZE - 25, &u32::MAX.to_le_bytes());
        let err = decode(&enc.frames_data, 1024, 1024, "pw").unwrap_err();
        assert!(err.contains("key-derivation"), "{err}");
    }

    #[test]
    fn newer_format_asks_user_to_update() {
        let mut enc = encoder::encode_multi(b"payload", "doc.txt", "text/plain", "pw").unwrap();
        tamper_header(&mut enc.frames_data, 4, &[VERSION + 1]);
        let err = decode(&enc.frames_data, 1024, 1024, "pw").unwrap_err();
        assert!(err.contains("newer version"), "{err}");
    }

    #[test]
    fn wrong_dimensions_are_rejected() {
        let err = decode(&[0u8; 16], 2, 2, "pw").unwrap_err();
        assert!(err.contains("2×2"), "{err}");
    }

    /// Build a legacy vault frame by hand to check backward compatibility.
    fn encode_legacy(version: u8, data: &[u8], filename: &str, password: &str) -> Vec<u8> {
        let salt = [7u8; 32];
        let iv = [9u8; 12];
        let mut meta = VaultMetadata::new(filename, "text/plain", data.len() as u64, salt, iv);
        meta.version = version;
        let header = meta.to_bytes();
        let aad: &[u8] = if version >= 2 { &header } else { &[] };
        let key = crypto::derive_key(password.as_bytes(), &salt, crypto::LEGACY_PBKDF2_ITERATIONS).unwrap();
        let ciphertext = crypto::encrypt(data, &key, &iv, aad).unwrap();
        let mut rgb = header.clone();
        rgb.extend_from_slice(&ciphertext);
        rgb_to_frame(&rgb)
    }

    #[test]
    fn v1_files_still_decode() {
        let rgba = encode_legacy(1, b"legacy contents", "old.txt", "pw");
        let result = decode(&rgba, 1024, 1024, "pw").expect("v1 decode failed");
        assert_eq!(result.file_bytes, b"legacy contents");
        assert_eq!(result.filename, "old.txt");
    }

    #[test]
    fn v2_files_still_decode() {
        let rgba = encode_legacy(2, b"v2 contents", "v2.txt", "pw");
        let result = decode(&rgba, 1024, 1024, "pw").expect("v2 decode failed");
        assert_eq!(result.file_bytes, b"v2 contents");
        assert_eq!(result.filename, "v2.txt");
        assert_eq!(result.mime_type, "text/plain");
    }

    #[test]
    fn v1_traversal_filename_is_sanitized() {
        // v1 headers are unauthenticated, so the filename must be cleaned on decode
        let rgba = encode_legacy(1, b"x", "../../.bashrc", "pw");
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
