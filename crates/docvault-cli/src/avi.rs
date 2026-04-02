//! AVI muxer and parser — Rust port of `web/src/utils/aviMuxer.ts`.
//!
//! Produces RIFF/AVI containers using the MPNG (Motion PNG) codec.
//! Every video frame is a complete, lossless PNG image stored inside a
//! `00dc` chunk — the exact same format as the TypeScript implementation,
//! ensuring full cross-compatibility between the CLI and the web app.
//!
//! RIFF/AVI structure:
//!   RIFF 'AVI '
//!   ├── LIST 'hdrl'
//!   │   ├── 'avih'  (AVIMAINHEADER, 56 bytes)
//!   │   └── LIST 'strl'
//!   │       ├── 'strh'  (AVISTREAMHEADER, 56 bytes)
//!   │       └── 'strf'  (BITMAPINFOHEADER, 40 bytes, biCompression='MPNG')
//!   └── LIST 'movi'
//!       ├── '00dc'  (PNG frame 1)
//!       ├── '00dc'  (PNG frame 2)
//!       └── ...

// ─── Muxer ───────────────────────────────────────────────────────────────────

/// Build a valid AVI file from a slice of PNG frame byte buffers.
///
/// The output is byte-for-byte identical to what `createAviFromPngFrames` in
/// `aviMuxer.ts` produces, so files created here can be decoded by the web
/// app and vice versa.
pub fn write_avi(frames: &[Vec<u8>], width: u32, height: u32) -> Vec<u8> {
    assert!(!frames.is_empty(), "Cannot create AVI with no frames");

    let fps: u32 = 1;
    let frame_count = frames.len();
    let max_frame_size = frames.iter().map(|f| f.len()).max().unwrap_or(0);

    // ── Size calculations (must match TypeScript exactly) ──────────────
    const AVIH_DATA: usize = 56;
    const STRH_DATA: usize = 56;
    const STRF_DATA: usize = 40;

    let avih_chunk_total = 8 + AVIH_DATA;
    let strh_chunk_total = 8 + STRH_DATA;
    let strf_chunk_total = 8 + STRF_DATA;

    // LIST 'strl': "strl"(4) + strh + strf
    let strl_content = 4 + strh_chunk_total + strf_chunk_total;
    let strl_total = 8 + strl_content;

    // LIST 'hdrl': "hdrl"(4) + avih + strl
    let hdrl_content = 4 + avih_chunk_total + strl_total;
    let hdrl_total = 8 + hdrl_content;

    // LIST 'movi': "movi"(4) + frame chunks (each = 8 + data [+ 1 pad if odd])
    let mut movi_frame_data: usize = 0;
    for frame in frames {
        movi_frame_data += 8 + frame.len();
        if frame.len() % 2 != 0 {
            movi_frame_data += 1;
        }
    }
    let movi_content = 4 + movi_frame_data;
    let movi_total = 8 + movi_content;

    // RIFF: "AVI "(4) + hdrl + movi
    let riff_content = 4 + hdrl_total + movi_total;
    let total_size = 8 + riff_content;

    let mut b = Vec::with_capacity(total_size);

    // ── Write helpers (all cast to the correct integer type at write time) ─
    macro_rules! fcc   { ($s:expr)  => { b.extend_from_slice($s) } }
    macro_rules! u32le { ($v:expr)  => { b.extend_from_slice(&u32::try_from($v).expect("value fits u32").to_le_bytes()) } }
    macro_rules! i32le { ($v:expr)  => { b.extend_from_slice(&($v as i32).to_le_bytes()) } }
    macro_rules! u16le { ($v:expr)  => { b.extend_from_slice(&($v as u16).to_le_bytes()) } }
    macro_rules! i16le { ($v:expr)  => { b.extend_from_slice(&($v as i16).to_le_bytes()) } }

    // ── RIFF header ────────────────────────────────────────────────────
    fcc!(b"RIFF"); u32le!(riff_content); fcc!(b"AVI ");

    // ── LIST 'hdrl' ────────────────────────────────────────────────────
    fcc!(b"LIST"); u32le!(hdrl_content); fcc!(b"hdrl");

    // avih (AVIMAINHEADER, 56 bytes)
    fcc!(b"avih"); u32le!(AVIH_DATA);
    u32le!(1_000_000u32 / fps);    // dwMicroSecPerFrame
    u32le!(0u32);                   // dwMaxBytesPerSec
    u32le!(0u32);                   // dwPaddingGranularity
    u32le!(0x10u32);                // dwFlags (AVIF_HASINDEX)
    u32le!(frame_count as u32);     // dwTotalFrames
    u32le!(0u32);                   // dwInitialFrames
    u32le!(1u32);                   // dwStreams
    u32le!(max_frame_size);         // dwSuggestedBufferSize
    u32le!(width);                  // dwWidth
    u32le!(height);                 // dwHeight
    u32le!(0u32); u32le!(0u32); u32le!(0u32); u32le!(0u32); // dwReserved[4]

    // LIST 'strl'
    fcc!(b"LIST"); u32le!(strl_content); fcc!(b"strl");

    // strh (AVISTREAMHEADER, 56 bytes)
    fcc!(b"strh"); u32le!(STRH_DATA);
    fcc!(b"vids");              // fccType
    fcc!(b"MPNG");              // fccHandler
    u32le!(0u32);               // dwFlags
    u16le!(0u16);               // wPriority
    u16le!(0u16);               // wLanguage
    u32le!(0u32);               // dwInitialFrames
    u32le!(1u32);               // dwScale
    u32le!(fps);                // dwRate
    u32le!(0u32);               // dwStart
    u32le!(frame_count as u32); // dwLength
    u32le!(max_frame_size);     // dwSuggestedBufferSize
    u32le!(0xFFFF_FFFFu32);     // dwQuality (-1)
    u32le!(0u32);               // dwSampleSize
    // rcFrame: left, top, right, bottom (each i16)
    i16le!(0i16); i16le!(0i16); i16le!(width as i16); i16le!(height as i16);

    // strf (BITMAPINFOHEADER, 40 bytes)
    fcc!(b"strf"); u32le!(STRF_DATA);
    u32le!(40u32);              // biSize
    i32le!(width as i32);       // biWidth
    i32le!(height as i32);      // biHeight (positive = bottom-up; matches TS)
    u16le!(1u16);               // biPlanes
    u16le!(32u16);              // biBitCount (32 = RGBA, matches TypeScript)
    fcc!(b"MPNG");              // biCompression
    u32le!(0u32);               // biSizeImage
    i32le!(0i32);               // biXPelsPerMeter
    i32le!(0i32);               // biYPelsPerMeter
    u32le!(0u32);               // biClrUsed
    u32le!(0u32);               // biClrImportant

    // ── LIST 'movi' ────────────────────────────────────────────────────
    fcc!(b"LIST"); u32le!(movi_content); fcc!(b"movi");

    for frame in frames {
        fcc!(b"00dc");
        u32le!(frame.len() as u32);
        b.extend_from_slice(frame);
        // RIFF word-alignment padding
        if frame.len() % 2 != 0 {
            b.push(0x00);
        }
    }

    b
}

// ─── Parser ──────────────────────────────────────────────────────────────────

/// Extract PNG frame buffers from a DocVault `.vault.avi` file.
///
/// Parses the RIFF/AVI structure, locates the `movi` LIST, and returns all
/// `00dc` video frame chunks as raw PNG byte vectors.
pub fn read_avi(data: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
    if data.len() < 12 {
        anyhow::bail!("File too small to be a valid RIFF/AVI");
    }

    if &data[0..4] != b"RIFF" {
        anyhow::bail!("Not a valid RIFF file (missing RIFF header)");
    }
    if &data[8..12] != b"AVI " {
        anyhow::bail!("Not a valid AVI file (missing AVI  FourCC)");
    }

    let mut pos = 12usize; // skip RIFF id (4) + RIFF size (4) + "AVI "(4)
    let mut png_frames: Vec<Vec<u8>> = Vec::new();

    while pos + 8 <= data.len() {
        let chunk_id = &data[pos..pos + 4];
        pos += 4;
        let chunk_size = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if chunk_id == b"LIST" {
            if pos + 4 > data.len() {
                break;
            }
            let list_type = &data[pos..pos + 4];

            if list_type == b"movi" {
                // chunk_size covers "movi"(4) + all frame chunks
                let movi_end = pos + chunk_size;
                pos += 4; // skip "movi" FourCC

                while pos + 8 <= movi_end.min(data.len()) {
                    let frame_id = &data[pos..pos + 4];
                    pos += 4;
                    let frame_size =
                        u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
                    pos += 4;

                    if frame_id == b"00dc" {
                        if pos + frame_size > data.len() {
                            anyhow::bail!(
                                "AVI frame data truncated at offset {pos} \
                                 (need {frame_size} bytes, have {})",
                                data.len() - pos
                            );
                        }
                        png_frames.push(data[pos..pos + frame_size].to_vec());
                    }

                    pos += frame_size;
                    if frame_size % 2 != 0 {
                        pos += 1; // word-alignment padding
                    }
                }
                break; // found movi — no need to scan further
            } else {
                // Skip this LIST (hdrl, INFO, etc.)
                pos += chunk_size; // chunk_size covers list_type + content
                if chunk_size % 2 != 0 {
                    pos += 1;
                }
            }
        } else {
            // Skip non-LIST chunk (idx1, JUNK, etc.)
            pos += chunk_size;
            if chunk_size % 2 != 0 {
                pos += 1;
            }
        }
    }

    if png_frames.is_empty() {
        anyhow::bail!("No video frames (00dc chunks) found in AVI file");
    }

    Ok(png_frames)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_png(seed: u8) -> Vec<u8> {
        // Build a 1×1 RGB PNG — just need valid PNG magic + IHDR + IDAT + IEND
        // Use the image crate for a real PNG
        let img = image::RgbImage::from_raw(1, 1, vec![seed, seed, seed]).unwrap();
        let dyn_img = image::DynamicImage::ImageRgb8(img);
        let mut buf = Vec::new();
        dyn_img
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn single_frame_roundtrip() {
        let frame = dummy_png(42);
        let avi = write_avi(&[frame.clone()], 1, 1);

        // Must start with RIFF
        assert_eq!(&avi[0..4], b"RIFF");
        assert_eq!(&avi[8..12], b"AVI ");

        let frames_out = read_avi(&avi).expect("read_avi failed");
        assert_eq!(frames_out.len(), 1);
        assert_eq!(frames_out[0], frame);
    }

    #[test]
    fn multi_frame_roundtrip() {
        let frames: Vec<Vec<u8>> = (0..5).map(|i| dummy_png(i * 50)).collect();
        let avi = write_avi(&frames, 1, 1);
        let frames_out = read_avi(&avi).expect("read_avi failed");

        assert_eq!(frames_out.len(), 5);
        for (i, (orig, decoded)) in frames.iter().zip(frames_out.iter()).enumerate() {
            assert_eq!(orig, decoded, "frame {i} differs");
        }
    }

    #[test]
    fn odd_frame_size_gets_padding() {
        // A 1-byte "frame" (odd size) must be padded to word-align
        let weird_frame = vec![0xFFu8];
        let avi = write_avi(&[weird_frame.clone()], 1, 1);
        // After "00dc" + size(1) + data(1) there should be a 0x00 pad byte
        // The parser should still reconstruct the exact original bytes
        let frames_out = read_avi(&avi).expect("read_avi failed");
        assert_eq!(frames_out[0], weird_frame);
    }

    #[test]
    fn read_non_riff_returns_error() {
        let result = read_avi(b"this is not an avi file at all");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("RIFF"));
    }

    #[test]
    fn riff_non_avi_returns_error() {
        let mut fake = b"RIFF\x00\x00\x00\x00WAV ".to_vec();
        fake.extend_from_slice(&[0u8; 100]);
        let result = read_avi(&fake);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("AVI"));
    }
}
