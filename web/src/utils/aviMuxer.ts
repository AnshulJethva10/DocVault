/**
 * aviMuxer.ts — Minimal AVI muxer and parser for lossless PNG video frames.
 *
 * Creates valid AVI files using the MPNG (Motion PNG) codec, where each
 * video frame is a complete, lossless PNG image. Playable in VLC, FFmpeg,
 * MPV, and most major video players.
 *
 * RIFF/AVI structure:
 *   RIFF 'AVI '
 *   ├── LIST 'hdrl'
 *   │   ├── 'avih'  (AVIMAINHEADER)
 *   │   └── LIST 'strl'
 *   │       ├── 'strh'  (AVISTREAMHEADER)
 *   │       └── 'strf'  (BITMAPINFOHEADER, biCompression='MPNG')
 *   └── LIST 'movi'
 *       ├── '00dc'  (PNG frame 1)
 *       ├── '00dc'  (PNG frame 2)
 *       └── ...
 */

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

// ─── AVI Muxer ──────────────────────────────────────────────────────────────

/**
 * Create a valid AVI file from multiple PNG frame blobs.
 *
 * Each frame is stored as a lossless PNG inside a '00dc' chunk using the
 * MPNG (Motion PNG) codec. The resulting AVI is playable in VLC, FFmpeg,
 * and most video players that support the PNG video codec.
 *
 * @param pngBlobs  Array of PNG image Blobs (one per frame)
 * @param width     Frame width in pixels
 * @param height    Frame height in pixels
 * @param fps       Frames per second (default: 1)
 * @returns         A Blob containing the complete AVI file
 */
export async function createAviFromPngFrames(
  pngBlobs: Blob[],
  width: number,
  height: number,
  fps: number = 1,
): Promise<Blob> {
  if (pngBlobs.length === 0) {
    throw new Error("Cannot create AVI: no PNG frames were provided");
  }

  // Convert PNG blobs to ArrayBuffers
  const frames: ArrayBuffer[] = [];
  for (const blob of pngBlobs) {
    frames.push(await blob.arrayBuffer());
  }

  const frameCount = frames.length;
  const maxFrameSize = Math.max(...frames.map((f) => f.byteLength));

  // ── Calculate sizes ──────────────────────────────────────────────────
  // avih chunk: 8 (id+size) + 56 (data) = 64
  const avihDataSize = 56;
  const avihChunkSize = 8 + avihDataSize;

  // strh chunk: 8 + 56 = 64
  const strhDataSize = 56;
  const strhChunkSize = 8 + strhDataSize;

  // strf chunk: 8 + 40 = 48
  const strfDataSize = 40;
  const strfChunkSize = 8 + strfDataSize;

  // strl LIST: 12 (LIST+size+'strl') + strh + strf
  const strlContentSize = 4 + strhChunkSize + strfChunkSize; // 'strl' + chunks
  const strlTotalSize = 8 + strlContentSize; // LIST + size + content

  // hdrl LIST: 12 + avih + strl
  const hdrlContentSize = 4 + avihChunkSize + strlTotalSize; // 'hdrl' + chunks
  const hdrlTotalSize = 8 + hdrlContentSize;

  // movi LIST: 12 + frame chunks
  let moviFrameDataSize = 0;
  for (const frame of frames) {
    moviFrameDataSize += 8 + frame.byteLength; // chunk header + data
    if (frame.byteLength % 2 !== 0) moviFrameDataSize += 1; // padding
  }
  const moviContentSize = 4 + moviFrameDataSize; // 'movi' + frame chunks
  const moviTotalSize = 8 + moviContentSize;

  // Total RIFF size = 4 ('AVI ') + hdrl + movi
  const riffContentSize = 4 + hdrlTotalSize + moviTotalSize;
  const totalFileSize = 8 + riffContentSize; // RIFF + size + content

  // ── Build the binary ─────────────────────────────────────────────────
  const buffer = new ArrayBuffer(totalFileSize);
  const view = new DataView(buffer);
  let pos = 0;

  // ── RIFF header ──
  writeFourCC(view, pos, "RIFF");
  pos += 4;
  view.setUint32(pos, riffContentSize, true);
  pos += 4;
  writeFourCC(view, pos, "AVI ");
  pos += 4;

  // ── hdrl LIST ──
  writeFourCC(view, pos, "LIST");
  pos += 4;
  view.setUint32(pos, hdrlContentSize, true);
  pos += 4;
  writeFourCC(view, pos, "hdrl");
  pos += 4;

  // ── avih chunk (AVIMAINHEADER, 56 bytes) ──
  writeFourCC(view, pos, "avih");
  pos += 4;
  view.setUint32(pos, avihDataSize, true);
  pos += 4;

  const microSecPerFrame = Math.round(1_000_000 / fps);
  view.setUint32(pos, microSecPerFrame, true);
  pos += 4; // dwMicroSecPerFrame
  view.setUint32(pos, 0, true);
  pos += 4; // dwMaxBytesPerSec
  view.setUint32(pos, 0, true);
  pos += 4; // dwPaddingGranularity
  view.setUint32(pos, 0x10, true);
  pos += 4; // dwFlags (AVIF_HASINDEX)
  view.setUint32(pos, frameCount, true);
  pos += 4; // dwTotalFrames
  view.setUint32(pos, 0, true);
  pos += 4; // dwInitialFrames
  view.setUint32(pos, 1, true);
  pos += 4; // dwStreams
  view.setUint32(pos, maxFrameSize, true);
  pos += 4; // dwSuggestedBufferSize
  view.setUint32(pos, width, true);
  pos += 4; // dwWidth
  view.setUint32(pos, height, true);
  pos += 4; // dwHeight
  // dwReserved[4]
  view.setUint32(pos, 0, true);
  pos += 4;
  view.setUint32(pos, 0, true);
  pos += 4;
  view.setUint32(pos, 0, true);
  pos += 4;
  view.setUint32(pos, 0, true);
  pos += 4;

  // ── strl LIST ──
  writeFourCC(view, pos, "LIST");
  pos += 4;
  view.setUint32(pos, strlContentSize, true);
  pos += 4;
  writeFourCC(view, pos, "strl");
  pos += 4;

  // ── strh chunk (AVISTREAMHEADER, 56 bytes) ──
  writeFourCC(view, pos, "strh");
  pos += 4;
  view.setUint32(pos, strhDataSize, true);
  pos += 4;

  writeFourCC(view, pos, "vids");
  pos += 4; // fccType
  writeFourCC(view, pos, "MPNG");
  pos += 4; // fccHandler
  view.setUint32(pos, 0, true);
  pos += 4; // dwFlags
  view.setUint16(pos, 0, true);
  pos += 2; // wPriority
  view.setUint16(pos, 0, true);
  pos += 2; // wLanguage
  view.setUint32(pos, 0, true);
  pos += 4; // dwInitialFrames
  view.setUint32(pos, 1, true);
  pos += 4; // dwScale
  view.setUint32(pos, fps, true);
  pos += 4; // dwRate
  view.setUint32(pos, 0, true);
  pos += 4; // dwStart
  view.setUint32(pos, frameCount, true);
  pos += 4; // dwLength
  view.setUint32(pos, maxFrameSize, true);
  pos += 4; // dwSuggestedBufferSize
  view.setUint32(pos, 0xffffffff, true);
  pos += 4; // dwQuality (-1)
  view.setUint32(pos, 0, true);
  pos += 4; // dwSampleSize
  // rcFrame: left, top, right, bottom (each WORD = 2 bytes)
  view.setInt16(pos, 0, true);
  pos += 2; // left
  view.setInt16(pos, 0, true);
  pos += 2; // top
  view.setInt16(pos, width, true);
  pos += 2; // right
  view.setInt16(pos, height, true);
  pos += 2; // bottom

  // ── strf chunk (BITMAPINFOHEADER, 40 bytes) ──
  writeFourCC(view, pos, "strf");
  pos += 4;
  view.setUint32(pos, strfDataSize, true);
  pos += 4;

  view.setUint32(pos, 40, true);
  pos += 4; // biSize
  view.setInt32(pos, width, true);
  pos += 4; // biWidth
  view.setInt32(pos, height, true);
  pos += 4; // biHeight
  view.setUint16(pos, 1, true);
  pos += 2; // biPlanes
  view.setUint16(pos, 32, true);
  pos += 2; // biBitCount (RGBA)
  writeFourCC(view, pos, "MPNG");
  pos += 4; // biCompression
  view.setUint32(pos, 0, true);
  pos += 4; // biSizeImage
  view.setInt32(pos, 0, true);
  pos += 4; // biXPelsPerMeter
  view.setInt32(pos, 0, true);
  pos += 4; // biYPelsPerMeter
  view.setUint32(pos, 0, true);
  pos += 4; // biClrUsed
  view.setUint32(pos, 0, true);
  pos += 4; // biClrImportant

  // ── movi LIST ──
  writeFourCC(view, pos, "LIST");
  pos += 4;
  view.setUint32(pos, moviContentSize, true);
  pos += 4;
  writeFourCC(view, pos, "movi");
  pos += 4;

  // ── Video frame chunks ('00dc') ──
  for (const frame of frames) {
    writeFourCC(view, pos, "00dc");
    pos += 4;
    view.setUint32(pos, frame.byteLength, true);
    pos += 4;

    const frameBytes = new Uint8Array(frame);
    new Uint8Array(buffer, pos, frame.byteLength).set(frameBytes);
    pos += frame.byteLength;

    // RIFF requires word-aligned chunks
    if (frame.byteLength % 2 !== 0) {
      view.setUint8(pos, 0);
      pos += 1;
    }
  }

  return new Blob([buffer], { type: "video/avi" });
}

// ─── AVI Parser ─────────────────────────────────────────────────────────────

/**
 * Extract PNG frame data from an AVI file created by DocVault.
 *
 * Parses the RIFF/AVI structure, locates the 'movi' list,
 * and extracts all '00dc' video frame chunks as PNG Blobs.
 *
 * @param file  The .avi File object
 * @returns     Array of PNG Blobs (one per frame)
 */
export async function extractPngFramesFromAvi(file: File): Promise<Blob[]> {
  const arrayBuffer = await file.arrayBuffer();
  const view = new DataView(arrayBuffer);
  let pos = 0;

  // Validate RIFF header
  const riffId = readFourCC(view, pos);
  pos += 4;
  if (riffId !== "RIFF") {
    throw new Error("Not a valid RIFF file");
  }

  pos += 4; // skip RIFF size

  const aviId = readFourCC(view, pos);
  pos += 4;
  if (aviId !== "AVI ") {
    throw new Error("Not a valid AVI file");
  }

  // Walk through top-level chunks to find 'movi' LIST
  const pngFrames: Blob[] = [];

  while (pos < arrayBuffer.byteLength - 8) {
    const chunkId = readFourCC(view, pos);
    pos += 4;
    const chunkSize = view.getUint32(pos, true);
    pos += 4;

    if (chunkId === "LIST") {
      const listType = readFourCC(view, pos);

      if (listType === "movi") {
        // Parse the movi list to extract frame chunks
        const moviEnd = pos + chunkSize; // pos is at start of data, data is chunkSize bytes
        pos += 4; // skip list type ('movi')

        while (pos < moviEnd - 8) {
          const frameId = readFourCC(view, pos);
          pos += 4;
          const frameSize = view.getUint32(pos, true);
          pos += 4;

          if (frameId === "00dc") {
            // This is a video frame — extract its PNG data
            const pngData = arrayBuffer.slice(pos, pos + frameSize);
            pngFrames.push(new Blob([pngData], { type: "image/png" }));
          }

          pos += frameSize;
          // Word-align
          if (frameSize % 2 !== 0) pos += 1;
        }
        break; // found movi, done
      } else {
        // Skip this LIST entirely (hdrl, etc.)
        // chunkSize covers the full data block (including the listType we peeked at)
        pos += chunkSize;
        if (chunkSize % 2 !== 0) pos += 1;
      }
    } else {
      // Skip non-LIST chunk
      pos += chunkSize;
      if (chunkSize % 2 !== 0) pos += 1;
    }
  }

  if (pngFrames.length === 0) {
    throw new Error("No video frames found in AVI file");
  }

  return pngFrames;
}
