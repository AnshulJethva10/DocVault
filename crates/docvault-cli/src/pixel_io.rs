//! PNG frame I/O — converts between RGBA pixel buffers (used by docvault-core)
//! and raw PNG bytes (stored on disk or inside AVI frames).
//!
//! Round-trip:
//!   encode: RGBA buffer → strip alpha → RGB PNG bytes
//!   decode: PNG bytes → image decode → add alpha (255) → RGBA buffer

use std::io::Cursor;

/// Encode a 1024×1024 RGBA pixel buffer (from `encode_multi`) into PNG bytes.
///
/// The alpha channel is discarded — it is always 255 from the encoder —
/// and the result is an RGB PNG, which is lossless and identical to what
/// the web app renders to its canvas.
pub fn encode_frame_to_png(rgba: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let pixel_count = (width * height) as usize;
    if rgba.len() < pixel_count * 4 {
        anyhow::bail!(
            "RGBA buffer too small: expected {} bytes, got {}",
            pixel_count * 4,
            rgba.len()
        );
    }

    // Strip alpha — encoder always sets alpha=255
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    for chunk in rgba[..pixel_count * 4].chunks_exact(4) {
        rgb.push(chunk[0]); // R
        rgb.push(chunk[1]); // G
        rgb.push(chunk[2]); // B
        // chunk[3] = alpha, always 255, discarded
    }

    let img = image::RgbImage::from_raw(width, height, rgb)
        .ok_or_else(|| anyhow::anyhow!("Failed to create RgbImage from pixel data"))?;

    let dyn_img = image::DynamicImage::ImageRgb8(img);
    let mut png_bytes = Vec::new();
    dyn_img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)?;

    Ok(png_bytes)
}

/// Decode PNG bytes into an RGBA pixel buffer suitable for `decoder::decode`.
///
/// Returns `(rgba_bytes, width, height)`.
/// The decoder's `extract_rgb_from_rgba` reads only R, G, B channels — the
/// alpha channel filled here (255) is stripped and never affects the output.
pub fn decode_png_to_rgba(png_bytes: &[u8]) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(png_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode PNG: {e}"))?;

    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();

    Ok((rgba.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: RGBA → PNG bytes → RGBA should preserve RGB channels.
    #[test]
    fn png_roundtrip_preserves_rgb() {
        // Build a 1×1 RGBA buffer: R=10, G=20, B=30, A=255
        let rgba_in: Vec<u8> = vec![10, 20, 30, 255];
        let png = encode_frame_to_png(&rgba_in, 1, 1).expect("encode failed");

        let (rgba_out, w, h) = decode_png_to_rgba(&png).expect("decode failed");
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        // RGB channels preserved; alpha will be 255
        assert_eq!(rgba_out[0], 10, "R");
        assert_eq!(rgba_out[1], 20, "G");
        assert_eq!(rgba_out[2], 30, "B");
        assert_eq!(rgba_out[3], 255, "A");
    }

    /// Full 1024×1024 frame round-trip — verifies size and spot values.
    #[test]
    fn full_frame_roundtrip() {
        // Create a frame where every pixel = (R=i%256, G=77, B=200, A=255)
        let mut rgba_in = vec![0u8; 1024 * 1024 * 4];
        for (i, chunk) in rgba_in.chunks_exact_mut(4).enumerate() {
            chunk[0] = (i % 256) as u8;
            chunk[1] = 77;
            chunk[2] = 200;
            chunk[3] = 255;
        }

        let png = encode_frame_to_png(&rgba_in, 1024, 1024).expect("encode failed");
        let (rgba_out, w, h) = decode_png_to_rgba(&png).expect("decode failed");

        assert_eq!((w, h), (1024, 1024));
        // Spot-check pixel 0
        assert_eq!(rgba_out[0], 0);
        assert_eq!(rgba_out[1], 77);
        assert_eq!(rgba_out[2], 200);
        // Spot-check pixel 1000
        let off = 1000 * 4;
        assert_eq!(rgba_out[off], (1000 % 256) as u8);
        assert_eq!(rgba_out[off + 1], 77);
        assert_eq!(rgba_out[off + 2], 200);
    }

    #[test]
    fn decode_invalid_bytes_returns_error() {
        let result = decode_png_to_rgba(b"not a png at all");
        assert!(result.is_err());
    }
}
