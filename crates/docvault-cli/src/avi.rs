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

/// AVIF_HASINDEX: the file ends with an `idx1` chunk.
const AVIF_HASINDEX: u32 = 0x10;
/// AVIIF_KEYFRAME: every MPNG frame is independently decodable.
const AVIIF_KEYFRAME: u32 = 0x10;
const IDX1_ENTRY: usize = 16;

/// Build a valid AVI file from a slice of PNG frame byte buffers.
///
/// The output is byte-for-byte identical to what `createAviFromPngFrames` in
/// `aviMuxer.ts` produces, so files created here can be decoded by the web
/// app and vice versa. An `idx1` index is written so media players that rely
/// on it (the header advertises AVIF_HASINDEX) can seek and count frames.
pub fn write_avi(frames: &[Vec<u8>], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    if frames.is_empty() {
        anyhow::bail!("Cannot create AVI with no frames");
    }
    const TOO_BIG: &str = "Vault is too large for an AVI file (4 GiB limit)";

    let fps: u32 = 1;
    let frame_count = u32::try_from(frames.len()).map_err(|_| anyhow::anyhow!(TOO_BIG))?;
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
    let movi_frame_data = frames
        .iter()
        .try_fold(0usize, |acc, f| acc.checked_add(8 + f.len() + f.len() % 2))
        .ok_or_else(|| anyhow::anyhow!(TOO_BIG))?;
    let movi_content = 4 + movi_frame_data;
    let movi_total = 8 + movi_content;

    // idx1: one 16-byte entry per frame
    let idx1_content = IDX1_ENTRY * frames.len();
    let idx1_total = 8 + idx1_content;

    // RIFF: "AVI "(4) + hdrl + movi + idx1
    let riff_content = 4 + hdrl_total + movi_total + idx1_total;
    if u32::try_from(riff_content).is_err() {
        anyhow::bail!(TOO_BIG);
    }
    let total_size = 8 + riff_content;

    let mut b = Vec::with_capacity(total_size);

    // ── Write helpers (sizes were checked against u32 above) ───────────
    macro_rules! fcc   { ($s:expr)  => { b.extend_from_slice($s) } }
    macro_rules! u32le { ($v:expr)  => { b.extend_from_slice(&($v as u32).to_le_bytes()) } }
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
    u32le!(AVIF_HASINDEX);          // dwFlags
    u32le!(frame_count);            // dwTotalFrames
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
    u32le!(frame_count);        // dwLength
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
        u32le!(frame.len());
        b.extend_from_slice(frame);
        // RIFF word-alignment padding
        if !frame.len().is_multiple_of(2) {
            b.push(0x00);
        }
    }

    // ── idx1 (offsets are relative to the 'movi' FourCC) ───────────────
    fcc!(b"idx1"); u32le!(idx1_content);
    let mut offset = 4usize;
    for frame in frames {
        fcc!(b"00dc");
        u32le!(AVIIF_KEYFRAME);
        u32le!(offset);
        u32le!(frame.len());
        offset += 8 + frame.len() + frame.len() % 2;
    }

    debug_assert_eq!(b.len(), total_size);
    Ok(b)
}

// ─── Parser ──────────────────────────────────────────────────────────────────

/// Read a RIFF chunk header at `pos`: (FourCC, size, data start).
fn chunk_header(data: &[u8], pos: usize) -> Option<(&[u8], usize, usize)> {
    let id = data.get(pos..pos.checked_add(4)?)?;
    let size = u32::from_le_bytes(data.get(pos + 4..pos + 8)?.try_into().ok()?) as usize;
    Some((id, size, pos + 8))
}

/// Offset just past a chunk's data, including the RIFF word-alignment pad byte.
fn chunk_end(start: usize, size: usize) -> Option<usize> {
    start.checked_add(size)?.checked_add(size % 2)
}

/// Extract PNG frame buffers from a DocVault `.vault.avi` file.
///
/// Parses the RIFF/AVI structure, locates the `movi` LIST, and returns all
/// `00dc` video frame chunks as slices into `data` (no copies). Every size
/// field is untrusted, so all offsets are bounds- and overflow-checked.
pub fn read_avi(data: &[u8]) -> anyhow::Result<Vec<&[u8]>> {
    if data.len() < 12 {
        anyhow::bail!("File too small to be a valid RIFF/AVI");
    }

    if &data[0..4] != b"RIFF" {
        anyhow::bail!("Not a valid RIFF file (missing RIFF header)");
    }
    if &data[8..12] != b"AVI " {
        anyhow::bail!("Not a valid AVI file (missing AVI  FourCC)");
    }
    let corrupt = || anyhow::anyhow!("AVI structure is corrupted (chunk sizes run past the end of the file)");

    let mut pos = 12usize; // skip RIFF id (4) + RIFF size (4) + "AVI "(4)
    let mut png_frames: Vec<&[u8]> = Vec::new();

    while let Some((chunk_id, chunk_size, body)) = chunk_header(data, pos) {
        if chunk_id == b"LIST" && data.get(body..body + 4) == Some(b"movi") {
            // chunk_size covers "movi"(4) + all frame chunks
            let movi_end = body.checked_add(chunk_size).ok_or_else(corrupt)?.min(data.len());
            pos = body + 4; // skip "movi" FourCC

            while pos + 8 <= movi_end {
                let (frame_id, frame_size, frame_start) = chunk_header(data, pos).ok_or_else(corrupt)?;
                let frame_end = frame_start.checked_add(frame_size).ok_or_else(corrupt)?;
                if frame_id == b"00dc" {
                    let frame = data.get(frame_start..frame_end).ok_or_else(|| {
                        anyhow::anyhow!(
                            "AVI frame data truncated at offset {frame_start} \
                             (need {frame_size} bytes, have {})",
                            data.len() - frame_start
                        )
                    })?;
                    png_frames.push(frame);
                }
                pos = chunk_end(frame_start, frame_size).ok_or_else(corrupt)?;
            }
            break; // found movi — no need to scan further
        }
        // Skip any other chunk or LIST (hdrl, INFO, idx1, JUNK, ...)
        pos = chunk_end(body, chunk_size).ok_or_else(corrupt)?;
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
        let avi = write_avi(std::slice::from_ref(&frame), 1, 1).unwrap();

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
        let avi = write_avi(&frames, 1, 1).unwrap();
        let frames_out = read_avi(&avi).expect("read_avi failed");

        assert_eq!(frames_out.len(), 5);
        for (i, (orig, decoded)) in frames.iter().zip(frames_out.iter()).enumerate() {
            assert_eq!(orig.as_slice(), *decoded, "frame {i} differs");
        }
    }

    #[test]
    fn odd_frame_size_gets_padding() {
        // A 1-byte "frame" (odd size) must be padded to word-align
        let weird_frame = vec![0xFFu8];
        let avi = write_avi(std::slice::from_ref(&weird_frame), 1, 1).unwrap();
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

    #[test]
    fn writes_an_idx1_index() {
        let frames = vec![dummy_png(1), vec![0xAA; 3]];
        let avi = write_avi(&frames, 1, 1).unwrap();
        let idx = avi.windows(4).rposition(|w| w == b"idx1").expect("idx1 chunk");
        assert_eq!(u32::from_le_bytes(avi[idx + 4..idx + 8].try_into().unwrap()), 32);
        // Second entry's offset = 4 ("movi") + 8 + len(frame 0) + pad
        let second = u32::from_le_bytes(avi[idx + 8 + 16 + 8..idx + 8 + 16 + 12].try_into().unwrap());
        assert_eq!(second as usize, 4 + 8 + frames[0].len() + frames[0].len() % 2);
        assert_eq!(read_avi(&avi).unwrap().len(), 2);
    }

    #[test]
    fn huge_chunk_sizes_do_not_panic() {
        let mut avi = write_avi(&[dummy_png(9)], 1, 1).unwrap();
        // Corrupt the hdrl LIST size to u32::MAX
        avi[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_avi(&avi).is_err());

        let mut avi = write_avi(&[dummy_png(9)], 1, 1).unwrap();
        let movi = avi.windows(4).position(|w| w == b"movi").unwrap();
        // Corrupt the first frame's size
        avi[movi + 8..movi + 12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_avi(&avi).is_err());
    }
}
