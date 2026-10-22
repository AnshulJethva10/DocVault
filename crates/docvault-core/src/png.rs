//! Lossless PNG frame encode/decode shared by the CLI and the browser build.
//!
//! The web app used to round-trip frames through a 2D canvas, which privacy
//! modes (Firefox `resistFingerprinting`, LibreWolf, Tor/Mullvad Browser,
//! Brave farbling) deliberately perturb — silently corrupting vaults. Doing
//! the PNG work here keeps every pixel exact on every platform.

use std::io::Cursor;

/// Max bytes the PNG decoder may allocate for one frame. A 1024×1024 RGBA16
/// frame needs 8 MiB, so this rejects decompression bombs with room to spare.
const DECODE_LIMIT_BYTES: usize = 32 * 1024 * 1024;

/// Encode tightly packed RGB8 pixels as a PNG.
pub fn encode_rgb(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() != expected {
        return Err(format!("RGB buffer is {} bytes, expected {}", rgb.len(), expected));
    }
    let mut out = Vec::with_capacity(expected + expected / 64 + 1024);
    let mut encoder = ::png::Encoder::new(&mut out, width, height);
    encoder.set_color(::png::ColorType::Rgb);
    encoder.set_depth(::png::BitDepth::Eight);
    // Ciphertext is incompressible: skip filter heuristics, keep fast deflate
    // so the zero padding in the last frame still shrinks.
    encoder.set_compression(::png::Compression::Fast);
    encoder.set_filter(::png::Filter::NoFilter);
    let mut writer = encoder.write_header().map_err(|e| format!("PNG encode failed: {e}"))?;
    writer.write_image_data(rgb).map_err(|e| format!("PNG encode failed: {e}"))?;
    writer.finish().map_err(|e| format!("PNG encode failed: {e}"))?;
    Ok(out)
}

/// Decode a PNG into tightly packed RGB8 pixels.
///
/// Accepts any lossless re-save of a vault frame (RGBA, 16-bit, palette or
/// greyscale) and normalises it back to RGB8. Rejects images that are not
/// `expected_width × expected_height` before decoding any pixel data.
pub fn decode_rgb(png: &[u8], expected_width: u32, expected_height: u32) -> Result<Vec<u8>, String> {
    let mut decoder = ::png::Decoder::new_with_limits(
        Cursor::new(png),
        ::png::Limits { bytes: DECODE_LIMIT_BYTES },
    );
    decoder.set_transformations(::png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| format!("Not a readable PNG: {e}"))?;

    let (width, height) = (reader.info().width, reader.info().height);
    if (width, height) != (expected_width, expected_height) {
        return Err(format!(
            "Image is {width}×{height}, but DocVault frames are {expected_width}×{expected_height} \
             (was it resized or re-compressed by another app?)"
        ));
    }

    let size = reader.output_buffer_size().ok_or("PNG is too large to decode")?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("PNG decode failed: {e}"))?;
    buf.truncate(info.buffer_size());

    let pixels = width as usize * height as usize;
    if buf.len() != pixels * info.color_type.samples() {
        return Err("Unexpected PNG row layout".to_string());
    }
    Ok(match info.color_type {
        ::png::ColorType::Rgb => buf,
        ::png::ColorType::Rgba => buf.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect(),
        ::png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g]).collect(),
        ::png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0]]).collect(),
        ::png::ColorType::Indexed => return Err("Unexpected palette PNG after expansion".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_round_trip_is_exact() {
        let rgb: Vec<u8> = (0..16 * 8 * 3).map(|i| (i * 37 % 256) as u8).collect();
        let png = encode_rgb(&rgb, 16, 8).unwrap();
        assert_eq!(decode_rgb(&png, 16, 8).unwrap(), rgb);
    }

    #[test]
    fn rgba_resave_is_normalised_to_rgb() {
        let mut out = Vec::new();
        let mut enc = ::png::Encoder::new(&mut out, 2, 1);
        enc.set_color(::png::ColorType::Rgba);
        enc.set_depth(::png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[1, 2, 3, 255, 4, 5, 6, 255]).unwrap();
        w.finish().unwrap();
        assert_eq!(decode_rgb(&out, 2, 1).unwrap(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn wrong_dimensions_are_rejected_before_decoding() {
        let png = encode_rgb(&[0u8; 4 * 4 * 3], 4, 4).unwrap();
        let err = decode_rgb(&png, 1024, 1024).unwrap_err();
        assert!(err.contains("4×4"), "{err}");
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(decode_rgb(b"definitely not a png", 1, 1).is_err());
    }
}
