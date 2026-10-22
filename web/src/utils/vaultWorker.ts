/**
 * vaultWorker.ts — promise-based client for `workers/vault.worker.ts`.
 *
 * One job runs at a time. `cancel()` terminates the worker (the only way to
 * stop a WASM call mid-flight); a fresh worker is started for the next job.
 */

export interface WorkerRequest {
  id: number;
  op: "encode" | "decode";
  file: File;
  password: string;
}

export type WorkerResponse =
  | { id: number; kind: "progress"; progress: number; message: string }
  | { id: number; kind: "encoded"; blob: Blob; isVideo: boolean }
  | { id: number; kind: "decoded"; blob: Blob; filename: string }
  | { id: number; kind: "error"; message: string };

export type ProgressFn = (progress: number, message: string) => void;

/** Thrown when a job is stopped by `cancel()`. */
export class CancelledError extends Error {
  constructor() {
    super("Cancelled");
    this.name = "CancelledError";
  }
}

interface Pending {
  id: number;
  onProgress?: ProgressFn;
  resolve: (msg: WorkerResponse) => void;
  reject: (err: Error) => void;
}

export class VaultWorkerClient {
  private worker: Worker | null = null;
  private pending: Pending | null = null;
  private nextId = 1;

  private getWorker(): Worker {
    if (this.worker) return this.worker;
    const worker = new Worker(new URL("../workers/vault.worker.ts", import.meta.url), {
      type: "module",
    });
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const msg = event.data;
      const job = this.pending;
      if (!job || msg.id !== job.id) return;
      if (msg.kind === "progress") {
        job.onProgress?.(msg.progress, msg.message);
        return;
      }
      this.pending = null;
      if (msg.kind === "error") job.reject(new Error(msg.message));
      else job.resolve(msg);
    };
    // A crashed worker (e.g. WASM out of memory) must not leave the UI stuck
    worker.onerror = (event) => {
      event.preventDefault();
      this.fail(new Error(event.message || "The encryption engine crashed (the file may be too large for this browser)"));
    };
    this.worker = worker;
    return worker;
  }

  private fail(err: Error): void {
    this.worker?.terminate();
    this.worker = null;
    const job = this.pending;
    this.pending = null;
    job?.reject(err);
  }

  private run(op: WorkerRequest["op"], file: File, password: string, onProgress?: ProgressFn): Promise<WorkerResponse> {
    if (this.pending) return Promise.reject(new Error("Another operation is already running"));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending = { id, onProgress, resolve, reject };
      this.getWorker().postMessage({ id, op, file, password } satisfies WorkerRequest);
    });
  }

  async encode(file: File, password: string, onProgress?: ProgressFn): Promise<{ blob: Blob; isVideo: boolean }> {
    const msg = await this.run("encode", file, password, onProgress);
    if (msg.kind !== "encoded") throw new Error("Unexpected response from encryption engine");
    return { blob: msg.blob, isVideo: msg.isVideo };
  }

  async decode(file: File, password: string, onProgress?: ProgressFn): Promise<{ blob: Blob; filename: string }> {
    const msg = await this.run("decode", file, password, onProgress);
    if (msg.kind !== "decoded") throw new Error("Unexpected response from encryption engine");
    return { blob: msg.blob, filename: msg.filename };
  }

  /** Stop the running job, if any. */
  cancel(): void {
    if (this.pending) this.fail(new CancelledError());
  }
}
