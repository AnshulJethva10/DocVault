import { useState, useCallback } from 'react';
import type { Status } from '../types';
import { createAviFromPngFrames, extractPngFramesFromAvi } from '../utils/aviMuxer';

import init, { encode_file_multi, decode_file, decode_file_multi, get_version } from 'docvault-core';
// @ts-ignore: Vite ?url import
import wasmUrl from 'docvault-core/docvault_core_bg.wasm?url';

// Track initialization at the module level
let wasmInitialized = false;

/** Fixed frame dimensions matching the Rust encoder */
const FRAME_WIDTH = 1024;
const FRAME_HEIGHT = 1024;

export function useDocVault() {
    const [status, setStatus] = useState<Status>({ type: 'idle' });
    const [previewBlob, setPreviewBlob] = useState<Blob | null>(null);

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
    const frameToPngBlob = async (rgba: Uint8Array, width: number, height: number): Promise<Blob> => {
        const canvas = document.createElement('canvas');
        canvas.width = width;
        canvas.height = height;
        const ctx = canvas.getContext('2d');
        if (!ctx) throw new Error('Failed to create canvas context');
        const imageData = new ImageData(new Uint8ClampedArray(rgba), width, height);
        ctx.putImageData(imageData, 0, 0);
        return new Promise<Blob>((resolve, reject) => {
            canvas.toBlob((b) => {
                if (b) resolve(b);
                else reject(new Error('Failed to export PNG'));
            }, 'image/png');
        });
    };

    /**
     * Encode a file into either a single PNG (≤ 1 frame) or an AVI video
     * with lossless MPNG frames (> 1 frame).
     *
     * @returns The output blob and whether it's an AVI video.
     */
    const encodeFile = useCallback(async (file: File, password: string): Promise<{ blob: Blob; isVideo: boolean }> => {
        try {
            setStatus({ type: 'processing', message: 'Loading encryption engine...', progress: 10 });
            const wasm = await loadWasm();

            setStatus({ type: 'processing', message: 'Reading file...', progress: 20 });
            const arrayBuffer = await file.arrayBuffer();
            const fileBytes = new Uint8Array(arrayBuffer);

            setStatus({ type: 'processing', message: 'Encrypting & encoding...', progress: 40 });
            const result = wasm.encode_file_multi(
                fileBytes,
                file.name,
                file.type || 'application/octet-stream',
                password
            );

            const frameCount = result.frame_count;
            const width = result.width;
            const height = result.height;

            if (frameCount === 1) {
                // Single frame → PNG
                setStatus({ type: 'processing', message: 'Generating image...', progress: 70 });
                const rgba = result.get_frame(0);
                result.free();

                setStatus({ type: 'processing', message: 'Exporting PNG...', progress: 90 });
                const blob = await frameToPngBlob(rgba, width, height);

                setPreviewBlob(blob);
                setStatus({ type: 'success', message: `Encoded successfully! (${formatSize(blob.size)} PNG, 1 frame)` });
                return { blob, isVideo: false };
            } else {
                // Multi-frame → AVI with MPNG lossless codec
                setStatus({ type: 'processing', message: `Rendering ${frameCount} frames...`, progress: 55 });

                // Render each RGBA frame to a PNG blob
                const pngBlobs: Blob[] = [];
                for (let i = 0; i < frameCount; i++) {
                    setStatus({
                        type: 'processing',
                        message: `Rendering frame ${i + 1} of ${frameCount}...`,
                        progress: 55 + Math.floor((i / frameCount) * 30),
                    });
                    const rgba = result.get_frame(i);
                    const pngBlob = await frameToPngBlob(rgba, width, height);
                    pngBlobs.push(pngBlob);
                }

                result.free();

                setStatus({ type: 'processing', message: 'Building AVI video...', progress: 90 });
                const aviBlob = await createAviFromPngFrames(pngBlobs, width, height, 1);

                setPreviewBlob(null);
                setStatus({
                    type: 'success',
                    message: `Encoded successfully! (${formatSize(aviBlob.size)} AVI, ${frameCount} frames)`,
                });
                return { blob: aviBlob, isVideo: true };
            }
        } catch (err: any) {
            console.error('WASM Encode Error:', err);
            const errMsg = err?.message || typeof err === 'string' ? err : 'Unknown error during encoding';
            setStatus({ type: 'error', message: `Encryption failed: ${errMsg}` });
            throw err;
        }
    }, [loadWasm]);

    /**
     * Extract RGBA pixel data from a single PNG image via canvas.
     */
    const extractPixelsFromImage = async (file: File): Promise<{ rgba: Uint8Array; width: number; height: number }> => {
        const imageBitmap = await createImageBitmap(file);
        const canvas = document.createElement('canvas');
        canvas.width = imageBitmap.width;
        canvas.height = imageBitmap.height;
        const ctx = canvas.getContext('2d');
        if (!ctx) throw new Error('Failed to create canvas context');
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
    const extractPixelsFromPngBlob = async (pngBlob: Blob): Promise<Uint8Array> => {
        const bitmap = await createImageBitmap(pngBlob);
        const canvas = document.createElement('canvas');
        canvas.width = bitmap.width;
        canvas.height = bitmap.height;
        const ctx = canvas.getContext('2d');
        if (!ctx) throw new Error('Failed to create canvas context');
        ctx.drawImage(bitmap, 0, 0);
        const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
        return new Uint8Array(imageData.data.buffer);
    };

    /**
     * Decode a DocVault encoded file back to the original.
     * Supports both single-frame .vault.png and multi-frame .vault.avi files.
     */
    const decodeFile = useCallback(async (file: File, password: string): Promise<{ blob: Blob; filename: string }> => {
        try {
            setStatus({ type: 'processing', message: 'Loading decryption engine...', progress: 10 });
            const wasm = await loadWasm();

            const isAvi = file.name.endsWith('.vault.avi') || file.name.endsWith('.avi');

            if (isAvi) {
                // Multi-frame AVI decode
                setStatus({ type: 'processing', message: 'Parsing AVI video...', progress: 15 });
                const pngFrames = await extractPngFramesFromAvi(file);
                const frameCount = pngFrames.length;

                // Extract RGBA pixels from each PNG frame
                const frameRgbaSize = FRAME_WIDTH * FRAME_HEIGHT * 4;
                const allFramesRgba = new Uint8Array(frameCount * frameRgbaSize);

                for (let i = 0; i < frameCount; i++) {
                    setStatus({
                        type: 'processing',
                        message: `Extracting frame ${i + 1} of ${frameCount}...`,
                        progress: 20 + Math.floor((i / frameCount) * 30),
                    });
                    const rgba = await extractPixelsFromPngBlob(pngFrames[i]);
                    allFramesRgba.set(rgba, i * frameRgbaSize);
                }

                setStatus({ type: 'processing', message: `Decrypting (${frameCount} frames)...`, progress: 60 });
                const result = wasm.decode_file_multi(
                    allFramesRgba,
                    frameCount,
                    FRAME_WIDTH,
                    FRAME_HEIGHT,
                    password
                );

                setStatus({ type: 'processing', message: 'Restoring file...', progress: 80 });
                const fileBytesCopy = result.file_bytes.slice();
                const filename = result.filename;
                const mimeType = result.mime_type;
                result.free();

                const fileBlob = new Blob([fileBytesCopy], {
                    type: mimeType || 'application/octet-stream',
                });

                setStatus({ type: 'success', message: `✅ Restored: ${filename}` });
                return { blob: fileBlob, filename };
            } else {
                // Single-frame PNG decode
                setStatus({ type: 'processing', message: 'Reading image...', progress: 20 });
                const { rgba, width, height } = await extractPixelsFromImage(file);

                setStatus({ type: 'processing', message: 'Decrypting & decoding...', progress: 50 });
                const result = wasm.decode_file(
                    rgba,
                    width,
                    height,
                    password
                );

                setStatus({ type: 'processing', message: 'Restoring file...', progress: 80 });
                const fileBytesCopy = result.file_bytes.slice();
                const filename = result.filename;
                const mimeType = result.mime_type;
                result.free();

                const fileBlob = new Blob([fileBytesCopy], {
                    type: mimeType || 'application/octet-stream',
                });

                setStatus({ type: 'success', message: `✅ Restored: ${filename}` });
                return { blob: fileBlob, filename };
            }
        } catch (err: any) {
            console.error('WASM Decode Error:', err);
            const errMsg = err?.message || typeof err === 'string' ? err : 'Unknown error during decoding';
            setStatus({ type: 'error', message: `Decryption failed: ${errMsg}` });
            throw err;
        }
    }, [loadWasm]);

    const reset = useCallback(() => {
        setStatus({ type: 'idle' });
        setPreviewBlob(null);
    }, []);

    return { status, previewBlob, encodeFile, decodeFile, reset };
}

/** Format byte sizes to human-readable strings. */
function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
