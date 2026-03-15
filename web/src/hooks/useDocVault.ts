import { useState, useCallback, useRef } from "react";
import type { Status, BatchFileItem } from "../types";
import {
  createAviFromPngFrames,
  extractPngFramesFromAvi,
} from "../utils/aviMuxer";

import init, {
  encode_file_multi,
  decode_file,
  decode_file_multi,
  get_version,
} from "docvault-core";
// @ts-ignore: Vite ?url import
import wasmUrl from "docvault-core/docvault_core_bg.wasm?url";

// Track initialization at the module level
let wasmInitialized = false;

/** Fixed frame dimensions matching the Rust encoder */
const FRAME_WIDTH = 1024;
const FRAME_HEIGHT = 1024;

export function useDocVault() {
  const [status, setStatus] = useState<Status>({ type: "idle" });
  const [previewBlob, setPreviewBlob] = useState<Blob | null>(null);
  const batchCancelledRef = useRef(false);

  const loadWasm = useCallback(async () => {
    if (!wasmInitialized) {
      await init(wasmUrl);
      wasmInitialized = true;
    }
    return { encode_file_multi, decode_file, decode_file_multi, get_version };
  }, []);

  /**
   * Render a single RGBA frame onto a canvas and return it as a lossless PNG Blob.
   */
  const frameToPngBlob = async (
    rgba: Uint8Array,
    width: number,
    height: number,
  ): Promise<Blob> => {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Failed to create canvas context");
    const imageData = new ImageData(new Uint8ClampedArray(rgba), width, height);
    ctx.putImageData(imageData, 0, 0);
    return new Promise<Blob>((resolve, reject) => {
      canvas.toBlob((b) => {
        if (b) resolve(b);
        else reject(new Error("Failed to export PNG"));
      }, "image/png");
    });
  };

  /**
   * Encode a single file into PNG or AVI — internal helper shared by
   * encodeFile (single) and encodeBatch (multi).
   * Does NOT set global status — caller is responsible for status updates.
   */
  const encodeOneFile = async (
    file: File,
    password: string,
    onProgress?: (progress: number, message: string) => void,
  ): Promise<{ blob: Blob; isVideo: boolean }> => {
    const wasm = await loadWasm();

    onProgress?.(20, "Reading file...");
    const arrayBuffer = await file.arrayBuffer();
    const fileBytes = new Uint8Array(arrayBuffer);

    onProgress?.(40, "Encrypting & encoding...");
    const result = wasm.encode_file_multi(
      fileBytes,
      file.name,
      file.type || "application/octet-stream",
      password,
    );

    const frameCount = result.frame_count;
    const width = result.width;
    const height = result.height;

    if (frameCount === 1) {
      onProgress?.(70, "Generating image...");
      const rgba = result.get_frame(0);
      result.free();

      onProgress?.(90, "Exporting PNG...");
      const blob = await frameToPngBlob(rgba, width, height);
      return { blob, isVideo: false };
    } else {
      onProgress?.(55, `Rendering ${frameCount} frames...`);

      const pngBlobs: Blob[] = [];
      for (let i = 0; i < frameCount; i++) {
        onProgress?.(
          55 + Math.floor((i / frameCount) * 30),
          `Rendering frame ${i + 1} of ${frameCount}...`,
        );
        const rgba = result.get_frame(i);
        const pngBlob = await frameToPngBlob(rgba, width, height);
        pngBlobs.push(pngBlob);
      }

      result.free();

      onProgress?.(90, "Building AVI video...");
      const aviBlob = await createAviFromPngFrames(pngBlobs, width, height, 1);
      return { blob: aviBlob, isVideo: true };
    }
  };

  /**
   * Encode a file into either a single PNG (≤ 1 frame) or an AVI video
   * with lossless MPNG frames (> 1 frame).
   *
   * @returns The output blob and whether it's an AVI video.
   */
  const encodeFile = useCallback(
    async (
      file: File,
      password: string,
    ): Promise<{ blob: Blob; isVideo: boolean }> => {
      try {
        setStatus({
          type: "processing",
          message: "Loading encryption engine...",
          progress: 10,
        });

        const { blob, isVideo } = await encodeOneFile(
          file,
          password,
          (progress, message) => {
            setStatus({ type: "processing", message, progress });
          },
        );

        if (!isVideo) {
          setPreviewBlob(blob);
        } else {
          setPreviewBlob(null);
        }

        const frameLabel = isVideo ? "AVI" : "PNG";
        setStatus({
          type: "success",
          message: `Encoded successfully! (${formatSize(blob.size)} ${frameLabel})`,
        });
        return { blob, isVideo };
      } catch (err: any) {
        console.error("WASM Encode Error:", err);
        const errMsg =
          err?.message ||
          (typeof err === "string" ? err : "Unknown error during encoding");
        setStatus({ type: "error", message: `Encryption failed: ${errMsg}` });
        throw err;
      }
    },
    [loadWasm],
  );

  /**
   * Encode multiple files sequentially with the same password.
   * Each file's progress is reported via `onFileUpdate` so the UI can
   * render per-file status indicators.
   */
  const encodeBatch = useCallback(
    async (
      items: BatchFileItem[],
      password: string,
      onFileUpdate: (fileId: string, patch: Partial<BatchFileItem>) => void,
    ): Promise<void> => {
      batchCancelledRef.current = false;

      try {
        setStatus({
          type: "processing",
          message: "Loading encryption engine...",
          progress: 5,
        });
        await loadWasm();
      } catch (err: any) {
        setStatus({
          type: "error",
          message: "Failed to load encryption engine",
        });
        return;
      }

      let successCount = 0;
      let errorCount = 0;

      for (let i = 0; i < items.length; i++) {
        if (batchCancelledRef.current) break;

        const item = items[i];
        const overallProgress = Math.round((i / items.length) * 100);
        setStatus({
          type: "processing",
          message: `Encoding file ${i + 1} of ${items.length}: ${item.file.name}`,
          progress: overallProgress,
        });

        onFileUpdate(item.id, {
          status: "processing",
          progress: 10,
          message: "Starting...",
        });

        try {
          const { blob, isVideo } = await encodeOneFile(
            item.file,
            password,
            (progress, message) => {
              onFileUpdate(item.id, { progress, message });
            },
          );

          const ext = isVideo ? ".vault.avi" : ".vault.png";
          const outputFilename = item.file.name.replace(/\.[^.]+$/, "") + ext;

          onFileUpdate(item.id, {
            status: "done",
            progress: 100,
            message: `Done — ${formatSize(blob.size)}`,
            outputBlob: blob,
            outputFilename,
            isVideo,
          });
          successCount++;
        } catch (err: any) {
          const errMsg =
            err?.message ||
            (typeof err === "string" ? String(err) : "Unknown error");
          onFileUpdate(item.id, {
            status: "error",
            progress: 0,
            message: errMsg,
            error: errMsg,
          });
          errorCount++;
        }
      }

      if (batchCancelledRef.current) {
        setStatus({ type: "idle" });
      } else {
        const summary =
          errorCount > 0
            ? `Batch complete: ${successCount} encoded, ${errorCount} failed`
            : `All ${successCount} files encoded successfully!`;
        setStatus({
          type: successCount > 0 ? "success" : "error",
          message: summary,
        });
      }
    },
    [loadWasm],
  );

  /**
   * Extract RGBA pixel data from a single PNG image via canvas.
   */
  const extractPixelsFromImage = async (
    file: File,
  ): Promise<{ rgba: Uint8Array; width: number; height: number }> => {
    const imageBitmap = await createImageBitmap(file);
    const canvas = document.createElement("canvas");
    canvas.width = imageBitmap.width;
    canvas.height = imageBitmap.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Failed to create canvas context");
    ctx.drawImage(imageBitmap, 0, 0);
    const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
    return {
      rgba: new Uint8Array(imageData.data.buffer),
      width: canvas.width,
      height: canvas.height,
    };
  };

  /**
   * Extract RGBA pixel data from a PNG Blob via canvas.
   */
  const extractPixelsFromPngBlob = async (
    pngBlob: Blob,
  ): Promise<Uint8Array> => {
    const bitmap = await createImageBitmap(pngBlob);
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Failed to create canvas context");
    ctx.drawImage(bitmap, 0, 0);
    const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
    return new Uint8Array(imageData.data.buffer);
  };

  /**
   * Decode a DocVault encoded file back to the original.
   * Supports both single-frame .vault.png and multi-frame .vault.avi files.
   */
  const decodeFile = useCallback(
    async (
      file: File,
      password: string,
    ): Promise<{ blob: Blob; filename: string }> => {
      try {
        setStatus({
          type: "processing",
          message: "Loading decryption engine...",
          progress: 10,
        });
        const wasm = await loadWasm();

        const lowerName = file.name.toLowerCase();
        const isAvi =
          lowerName.endsWith(".vault.avi") ||
          file.type === "video/avi" ||
          file.type === "video/x-msvideo";

        if (isAvi) {
          // Multi-frame AVI decode
          setStatus({
            type: "processing",
            message: "Parsing AVI video...",
            progress: 15,
          });
          const pngFrames = await extractPngFramesFromAvi(file);
          const frameCount = pngFrames.length;

          // Extract RGBA pixels from each PNG frame
          const frameRgbaSize = FRAME_WIDTH * FRAME_HEIGHT * 4;
          const allFramesRgba = new Uint8Array(frameCount * frameRgbaSize);

          for (let i = 0; i < frameCount; i++) {
            setStatus({
              type: "processing",
              message: `Extracting frame ${i + 1} of ${frameCount}...`,
              progress: 20 + Math.floor((i / frameCount) * 30),
            });
            const rgba = await extractPixelsFromPngBlob(pngFrames[i]);
            allFramesRgba.set(rgba, i * frameRgbaSize);
          }

          setStatus({
            type: "processing",
            message: `Decrypting (${frameCount} frames)...`,
            progress: 60,
          });
          const result = wasm.decode_file_multi(
            allFramesRgba,
            frameCount,
            FRAME_WIDTH,
            FRAME_HEIGHT,
            password,
          );

          setStatus({
            type: "processing",
            message: "Restoring file...",
            progress: 80,
          });
          const fileBytesCopy = result.file_bytes.slice();
          const filename = result.filename;
          const mimeType = result.mime_type;
          result.free();

          const fileBlob = new Blob([fileBytesCopy], {
            type: mimeType || "application/octet-stream",
          });

          setStatus({ type: "success", message: `✅ Restored: ${filename}` });
          return { blob: fileBlob, filename };
        } else {
          // Single-frame PNG decode
          setStatus({
            type: "processing",
            message: "Reading image...",
            progress: 20,
          });
          const { rgba, width, height } = await extractPixelsFromImage(file);

          setStatus({
            type: "processing",
            message: "Decrypting & decoding...",
            progress: 50,
          });
          const result = wasm.decode_file(rgba, width, height, password);

          setStatus({
            type: "processing",
            message: "Restoring file...",
            progress: 80,
          });
          const fileBytesCopy = result.file_bytes.slice();
          const filename = result.filename;
          const mimeType = result.mime_type;
          result.free();

          const fileBlob = new Blob([fileBytesCopy], {
            type: mimeType || "application/octet-stream",
          });

          setStatus({ type: "success", message: `✅ Restored: ${filename}` });
          return { blob: fileBlob, filename };
        }
      } catch (err: any) {
        console.error("WASM Decode Error:", err);
        const errMsg =
          err?.message ||
          (typeof err === "string" ? err : "Unknown error during decoding");
        setStatus({ type: "error", message: `Decryption failed: ${errMsg}` });
        throw err;
      }
    },
    [loadWasm],
  );

  const reset = useCallback(() => {
    setStatus({ type: "idle" });
    setPreviewBlob(null);
    batchCancelledRef.current = true;
  }, []);

  return { status, previewBlob, encodeFile, encodeBatch, decodeFile, reset };
}

/** Format byte sizes to human-readable strings. */
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
