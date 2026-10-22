/// <reference lib="webworker" />
/**
 * vault.worker.ts — runs all WASM encode/decode work off the main thread.
 *
 * Pixels never touch a canvas: PNG encoding/decoding happens inside the
 * WASM core, so browser anti-fingerprinting (Firefox resistFingerprinting,
 * LibreWolf, Tor/Mullvad Browser, Brave farbling) cannot corrupt vaults.
 * Running in a worker also keeps the UI responsive, lets a job be cancelled
 * by terminating the worker, and contains WASM out-of-memory crashes.
 */
import init, { encode_file_png, PngFrameDecoder } from "docvault-core";
// @ts-ignore: Vite ?url import
import wasmUrl from "docvault-core/docvault_core_bg.wasm?url";
import { createAviFromPngFrames, parseAviFrames } from "../utils/aviMuxer";
import type { WorkerRequest, WorkerResponse } from "../utils/vaultWorker";

/** Fixed frame dimensions matching the Rust encoder */
const FRAME_WIDTH = 1024;
const FRAME_HEIGHT = 1024;

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

let ready: Promise<unknown> | null = null;

const post = (msg: WorkerResponse, transfer: Transferable[] = []) =>
  (self as unknown as DedicatedWorkerGlobalScope).postMessage(msg, transfer);

function isPng(bytes: Uint8Array): boolean {
  return PNG_SIGNATURE.every((b, i) => bytes[i] === b);
}

function isAvi(bytes: Uint8Array): boolean {
  const ascii = (start: number) => String.fromCharCode(...bytes.subarray(start, start + 4));
  return bytes.length >= 12 && ascii(0) === "RIFF" && ascii(8) === "AVI ";
}

async function encode(id: number, file: File, password: string): Promise<void> {
  post({ id, kind: "progress", progress: 20, message: "Reading file..." });
  const fileBytes = new Uint8Array(await file.arrayBuffer());

  post({ id, kind: "progress", progress: 40, message: "Encrypting & encoding..." });
  const result = encode_file_png(
    fileBytes,
    file.name,
    file.type || "application/octet-stream",
    password,
  );
  // get_frame_png returns a fresh copy (not a view into WASM memory)
  const frames: Uint8Array<ArrayBuffer>[] = [];
  try {
    for (let i = 0; i < result.frame_count; i++) {
      frames.push(result.get_frame_png(i) as Uint8Array<ArrayBuffer>);
    }
  } finally {
    result.free();
  }
  if (frames.length === 0) throw new Error("Encoder returned no frames");

  if (frames.length === 1) {
    post({ id, kind: "encoded", blob: new Blob([frames[0]], { type: "image/png" }), isVideo: false });
    return;
  }
  post({ id, kind: "progress", progress: 85, message: `Building AVI video (${frames.length} frames)...` });
  const blob = createAviFromPngFrames(frames, FRAME_WIDTH, FRAME_HEIGHT, 1);
  post({ id, kind: "encoded", blob, isVideo: true });
}

async function decode(id: number, file: File, password: string): Promise<void> {
  post({ id, kind: "progress", progress: 15, message: "Reading file..." });
  const bytes = new Uint8Array(await file.arrayBuffer());

  // Trust the file's contents, not its name or reported type
  let frames: Uint8Array[];
  if (isPng(bytes)) {
    frames = [bytes];
  } else if (isAvi(bytes)) {
    frames = parseAviFrames(bytes.buffer);
  } else {
    throw new Error("Not a DocVault file (expected a .vault.png image or .vault.avi video)");
  }

  const decoder = new PngFrameDecoder();
  try {
    for (let i = 0; i < frames.length; i++) {
      post({
        id,
        kind: "progress",
        progress: 20 + Math.floor((i / frames.length) * 40),
        message: frames.length > 1 ? `Reading frame ${i + 1} of ${frames.length}...` : "Reading image...",
      });
      decoder.push_frame(frames[i]);
    }

    post({ id, kind: "progress", progress: 65, message: "Decrypting..." });
    const result = decoder.finish(password);
    try {
      // file_bytes is a fresh copy, so it stays valid after result.free()
      const fileBytes = result.file_bytes as Uint8Array<ArrayBuffer>;
      const blob = new Blob([fileBytes], { type: result.mime_type || "application/octet-stream" });
      post({ id, kind: "decoded", blob, filename: result.filename });
    } finally {
      result.free();
    }
  } finally {
    decoder.free();
  }
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const { id, op, file, password } = event.data;
  try {
    ready ??= init({ module_or_path: wasmUrl });
    await ready;
    if (op === "encode") await encode(id, file, password);
    else await decode(id, file, password);
  } catch (err: unknown) {
    const message =
      err instanceof Error ? err.message : typeof err === "string" ? err : "Unknown error";
    post({ id, kind: "error", message });
  }
};
