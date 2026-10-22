/**
 * aviMuxer.ts — Minimal AVI muxer and parser for lossless PNG video frames.
 *
 * Creates valid AVI files using the MPNG (Motion PNG) codec, where each
 * video frame is a complete, lossless PNG image. Playable in VLC, FFmpeg,
 * MPV, and most major video players. Byte-for-byte identical to the Rust
 * implementation in `crates/docvault-cli/src/avi.rs`.
 *
 * RIFF/AVI structure:
 *   RIFF 'AVI '
 *   ├── LIST 'hdrl'
 *   │   ├── 'avih'  (AVIMAINHEADER)
 *   │   └── LIST 'strl'
 *   │       ├── 'strh'  (AVISTREAMHEADER)
 *   │       └── 'strf'  (BITMAPINFOHEADER, biCompression='MPNG')
 *   ├── LIST 'movi'
 *   │   ├── '00dc'  (PNG frame 1)
 *   │   ├── '00dc'  (PNG frame 2)
 *   │   └── ...
 *   └── 'idx1'  (one 16-byte entry per frame)
 */

/** AVIF_HASINDEX: the file ends with an 'idx1' chunk. */
const AVIF_HASINDEX = 0x10;
/** AVIIF_KEYFRAME: every MPNG frame is independently decodable. */
const AVIIF_KEYFRAME = 0x10;
const IDX1_ENTRY = 16;
/** RIFF sizes are 32-bit. */
const MAX_RIFF_SIZE = 0xffffffff;

// ─── Helpers ────────────────────────────────────────────────────────────────

/** Write a FourCC string (4 ASCII chars) into a DataView at the given offset. */
function writeFourCC(view: DataView, offset: number, str: string): void {
  for (let i = 0; i < 4; i++) {
    view.setUint8(offset + i, str.charCodeAt(i));
  }
}

/** Read a FourCC string (4 ASCII chars) from a DataView at the given offset. */
function readFourCC(view: DataView, offset: number): string {
  return String.fromCharCode(
    view.getUint8(offset),
    view.getUint8(offset + 1),
    view.getUint8(offset + 2),
    view.getUint8(offset + 3),
  );
}

/** Size of a chunk's data including the RIFF word-alignment pad byte. */
function padded(size: number): number {
  return size + (size % 2);
}

// ─── AVI Muxer ──────────────────────────────────────────────────────────────

/**
 * Create a valid AVI file from PNG frames.
 *
 * Each frame is stored as a lossless PNG inside a '00dc' chunk using the
 * MPNG (Motion PNG) codec, followed by an 'idx1' index.
 *
 * @param frames  PNG file bytes, one entry per frame
 * @param width   Frame width in pixels
 * @param height  Frame height in pixels
 * @param fps     Frames per second (default: 1)
 */
export function createAviFromPngFrames(
  frames: Uint8Array[],
  width: number,
  height: number,
  fps: number = 1,
): Blob {
  if (frames.length === 0) {
    throw new Error("Cannot create AVI: no PNG frames were provided");
  }

  const frameCount = frames.length;
  const maxFrameSize = frames.reduce((m, f) => Math.max(m, f.byteLength), 0);

  // ── Calculate sizes ──────────────────────────────────────────────────
  const avihDataSize = 56;
  const avihChunkSize = 8 + avihDataSize;
  const strhDataSize = 56;
  const strhChunkSize = 8 + strhDataSize;
  const strfDataSize = 40;
  const strfChunkSize = 8 + strfDataSize;

  const strlContentSize = 4 + strhChunkSize + strfChunkSize; // 'strl' + chunks
  const strlTotalSize = 8 + strlContentSize;
  const hdrlContentSize = 4 + avihChunkSize + strlTotalSize; // 'hdrl' + chunks
  const hdrlTotalSize = 8 + hdrlContentSize;

  let moviFrameDataSize = 0;
  for (const frame of frames) {
    moviFrameDataSize += 8 + padded(frame.byteLength);
  }
  const moviContentSize = 4 + moviFrameDataSize; // 'movi' + frame chunks
  const moviTotalSize = 8 + moviContentSize;

  const idx1ContentSize = IDX1_ENTRY * frameCount;
  const idx1TotalSize = 8 + idx1ContentSize;

  const riffContentSize = 4 + hdrlTotalSize + moviTotalSize + idx1TotalSize;
  if (riffContentSize > MAX_RIFF_SIZE) {
    throw new Error("Vault is too large for an AVI file (4 GiB limit)");
  }
  const totalFileSize = 8 + riffContentSize;

  // ── Build the binary ─────────────────────────────────────────────────
  const buffer = new ArrayBuffer(totalFileSize);
  const view = new DataView(buffer);
  const bytes = new Uint8Array(buffer);
  let pos = 0;
  const fourcc = (s: string) => {
    writeFourCC(view, pos, s);
    pos += 4;
  };
  const u32 = (v: number) => {
    view.setUint32(pos, v, true);
    pos += 4;
  };
  const i32 = (v: number) => {
    view.setInt32(pos, v, true);
    pos += 4;
  };
  const u16 = (v: number) => {
    view.setUint16(pos, v, true);
    pos += 2;
  };
  const i16 = (v: number) => {
    view.setInt16(pos, v, true);
    pos += 2;
  };

  // ── RIFF header ──
  fourcc("RIFF"); u32(riffContentSize); fourcc("AVI ");

  // ── hdrl LIST ──
  fourcc("LIST"); u32(hdrlContentSize); fourcc("hdrl");

  // ── avih chunk (AVIMAINHEADER, 56 bytes) ──
  fourcc("avih"); u32(avihDataSize);
  u32(Math.round(1_000_000 / fps)); // dwMicroSecPerFrame
  u32(0);                           // dwMaxBytesPerSec
  u32(0);                           // dwPaddingGranularity
  u32(AVIF_HASINDEX);               // dwFlags
  u32(frameCount);                  // dwTotalFrames
  u32(0);                           // dwInitialFrames
  u32(1);                           // dwStreams
  u32(maxFrameSize);                // dwSuggestedBufferSize
  u32(width);                       // dwWidth
  u32(height);                      // dwHeight
  u32(0); u32(0); u32(0); u32(0);   // dwReserved[4]

  // ── strl LIST ──
  fourcc("LIST"); u32(strlContentSize); fourcc("strl");

  // ── strh chunk (AVISTREAMHEADER, 56 bytes) ──
  fourcc("strh"); u32(strhDataSize);
  fourcc("vids");     // fccType
  fourcc("MPNG");     // fccHandler
  u32(0);             // dwFlags
  u16(0);             // wPriority
  u16(0);             // wLanguage
  u32(0);             // dwInitialFrames
  u32(1);             // dwScale
  u32(fps);           // dwRate
  u32(0);             // dwStart
  u32(frameCount);    // dwLength
  u32(maxFrameSize);  // dwSuggestedBufferSize
  u32(0xffffffff);    // dwQuality (-1)
  u32(0);             // dwSampleSize
  i16(0); i16(0); i16(width); i16(height); // rcFrame

  // ── strf chunk (BITMAPINFOHEADER, 40 bytes) ──
  fourcc("strf"); u32(strfDataSize);
  u32(40);        // biSize
  i32(width);     // biWidth
  i32(height);    // biHeight
  u16(1);         // biPlanes
  u16(32);        // biBitCount (RGBA)
  fourcc("MPNG"); // biCompression
  u32(0);         // biSizeImage
  i32(0);         // biXPelsPerMeter
  i32(0);         // biYPelsPerMeter
  u32(0);         // biClrUsed
  u32(0);         // biClrImportant

  // ── movi LIST ──
  fourcc("LIST"); u32(moviContentSize); fourcc("movi");
  for (const frame of frames) {
    fourcc("00dc");
    u32(frame.byteLength);
    bytes.set(frame, pos);
    pos += padded(frame.byteLength); // ArrayBuffer is zero-filled, so the pad byte is 0
  }

  // ── idx1 (offsets are relative to the 'movi' FourCC) ──
  fourcc("idx1"); u32(idx1ContentSize);
  let offset = 4;
  for (const frame of frames) {
    fourcc("00dc");
    u32(AVIIF_KEYFRAME);
    u32(offset);
    u32(frame.byteLength);
    offset += 8 + padded(frame.byteLength);
  }

  return new Blob([buffer], { type: "video/avi" });
}

// ─── AVI Parser ─────────────────────────────────────────────────────────────

/**
 * Extract PNG frame data from an AVI file created by DocVault.
 *
 * Parses the RIFF/AVI structure, locates the 'movi' list, and returns every
 * '00dc' video frame chunk as a view into `buffer` (no copies). All chunk
 * sizes are untrusted and bounds-checked.
 */
export function parseAviFrames(buffer: ArrayBuffer): Uint8Array[] {
  const view = new DataView(buffer);
  const len = buffer.byteLength;

  if (len < 12 || readFourCC(view, 0) !== "RIFF") {
    throw new Error("Not a valid RIFF file");
  }
  if (readFourCC(view, 8) !== "AVI ") {
    throw new Error("Not a valid AVI file");
  }

  const pngFrames: Uint8Array[] = [];
  let pos = 12;

  while (pos + 8 <= len) {
    const chunkId = readFourCC(view, pos);
    const chunkSize = view.getUint32(pos + 4, true);
    const body = pos + 8;

    if (chunkId === "LIST" && body + 4 <= len && readFourCC(view, body) === "movi") {
      const moviEnd = Math.min(body + chunkSize, len);
      pos = body + 4; // skip 'movi'

      while (pos + 8 <= moviEnd) {
        const frameId = readFourCC(view, pos);
        const frameSize = view.getUint32(pos + 4, true);
        const start = pos + 8;
        if (frameId === "00dc") {
          if (start + frameSize > len) {
            throw new Error(
              `AVI frame data truncated at offset ${start} (need ${frameSize} bytes, have ${len - start})`,
            );
          }
          pngFrames.push(new Uint8Array(buffer, start, frameSize));
        }
        pos = start + padded(frameSize);
      }
      break; // found movi, done
    }

    // Skip any other LIST or chunk (hdrl, INFO, idx1, JUNK, ...)
    pos = body + padded(chunkSize);
  }

  if (pngFrames.length === 0) {
    throw new Error("No video frames found in AVI file");
  }
  return pngFrames;
}

/** Convenience wrapper: read a Blob/File and extract its PNG frames. */
export async function extractPngFramesFromAvi(file: Blob): Promise<Uint8Array[]> {
  return parseAviFrames(await file.arrayBuffer());
}
