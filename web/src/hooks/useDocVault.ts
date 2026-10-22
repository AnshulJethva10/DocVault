import { useState, useCallback, useRef, useEffect } from "react";
import type { Status, BatchFileItem } from "../types";
import { CancelledError, VaultWorkerClient } from "../utils/vaultWorker";

/** Pull a readable message out of whatever the worker or WASM threw. */
function errorMessage(err: unknown, fallback: string): string {
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string") return err;
  return fallback;
}

export function useDocVault() {
  const [status, setStatus] = useState<Status>({ type: "idle" });
  const [previewBlob, setPreviewBlob] = useState<Blob | null>(null);
  const batchCancelledRef = useRef(false);
  const clientRef = useRef<VaultWorkerClient | null>(null);

  const client = useCallback((): VaultWorkerClient => {
    clientRef.current ??= new VaultWorkerClient();
    return clientRef.current;
  }, []);

  // Stop any running job when the component unmounts
  useEffect(() => () => clientRef.current?.cancel(), []);

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

        const { blob, isVideo } = await client().encode(
          file,
          password,
          (progress, message) => {
            setStatus({ type: "processing", message, progress });
          },
        );

        setPreviewBlob(isVideo ? null : blob);

        const frameLabel = isVideo ? "AVI" : "PNG";
        setStatus({
          type: "success",
          message: `Encoded successfully! (${formatSize(blob.size)} ${frameLabel})`,
        });
        return { blob, isVideo };
      } catch (err: unknown) {
        if (err instanceof CancelledError) {
          setStatus({ type: "idle" });
        } else {
          console.error("Encode error:", err);
          const msg = errorMessage(err, "Unknown error during encoding");
          setStatus({ type: "error", message: `Encryption failed: ${msg}` });
        }
        throw err;
      }
    },
    [client],
  );

  /**
   * Encode multiple files sequentially with the same password.
   * Files that are already done are skipped, so pressing the button again
   * after adding files or after a failure only processes what is left.
   * Each file's progress is reported via `onFileUpdate`.
   */
  const encodeBatch = useCallback(
    async (
      items: BatchFileItem[],
      password: string,
      onFileUpdate: (fileId: string, patch: Partial<BatchFileItem>) => void,
    ): Promise<void> => {
      batchCancelledRef.current = false;

      const todo = items.filter((item) => item.status !== "done");
      let successCount = items.length - todo.length;
      let errorCount = 0;
      // Lower-cased output names already handed out in this batch, so
      // "report.pdf" and "report.docx" don't both download as report.vault.png
      const usedNames = new Set<string>(
        items.flatMap((item) => (item.outputFilename ? [item.outputFilename.toLowerCase()] : [])),
      );

      for (let i = 0; i < todo.length; i++) {
        if (batchCancelledRef.current) break;

        const item = todo[i];
        setStatus({
          type: "processing",
          message: `Encoding file ${i + 1} of ${todo.length}: ${item.file.name}`,
          progress: Math.round((i / todo.length) * 100),
        });

        onFileUpdate(item.id, {
          status: "processing",
          progress: 10,
          message: "Starting...",
          error: undefined,
        });

        try {
          const { blob, isVideo } = await client().encode(
            item.file,
            password,
            (progress, message) => {
              onFileUpdate(item.id, { progress, message });
            },
          );

          const ext = isVideo ? ".vault.avi" : ".vault.png";
          const outputFilename = uniqueOutputName(
            item.file.name.replace(/\.[^.]+$/, ""),
            ext,
            usedNames,
          );

          onFileUpdate(item.id, {
            status: "done",
            progress: 100,
            message: `Done — ${formatSize(blob.size)}`,
            outputBlob: blob,
            outputFilename,
            isVideo,
          });
          successCount++;
        } catch (err: unknown) {
          if (err instanceof CancelledError) {
            onFileUpdate(item.id, { status: "pending", progress: 0, message: "" });
            break;
          }
          const msg = errorMessage(err, "Unknown error");
          onFileUpdate(item.id, {
            status: "error",
            progress: 0,
            message: msg,
            error: msg,
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
    [client],
  );

  /**
   * Decode a DocVault encoded file back to the original.
   * The format (.vault.png or .vault.avi) is detected from the file's bytes.
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
        const { blob, filename } = await client().decode(
          file,
          password,
          (progress, message) => {
            setStatus({ type: "processing", message, progress });
          },
        );
        setStatus({ type: "success", message: `✅ Restored: ${filename}` });
        return { blob, filename };
      } catch (err: unknown) {
        if (err instanceof CancelledError) {
          setStatus({ type: "idle" });
        } else {
          console.error("Decode error:", err);
          const msg = errorMessage(err, "Unknown error during decoding");
          setStatus({ type: "error", message: `Decryption failed: ${msg}` });
        }
        throw err;
      }
    },
    [client],
  );

  /** Stop any running job and clear status. */
  const reset = useCallback(() => {
    batchCancelledRef.current = true;
    clientRef.current?.cancel();
    setStatus({ type: "idle" });
    setPreviewBlob(null);
  }, []);

  return { status, previewBlob, encodeFile, encodeBatch, decodeFile, reset };
}

/** Return `${stem}${ext}`, or `${stem} (n)${ext}` if that name is already used. */
function uniqueOutputName(
  stem: string,
  ext: string,
  used: Set<string>,
): string {
  let name = stem + ext;
  for (let n = 2; used.has(name.toLowerCase()); n++) {
    name = `${stem} (${n})${ext}`;
  }
  used.add(name.toLowerCase());
  return name;
}

/** Format byte sizes to human-readable strings. */
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
