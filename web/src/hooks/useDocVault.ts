import { useState, useCallback } from 'react';
import type { Status } from '../types';

import init, { encode_file, decode_file, get_version } from 'docvault-core';
// @ts-ignore: Vite ?url import
import wasmUrl from 'docvault-core/docvault_core_bg.wasm?url';

// Track initialization at the module level
let wasmInitialized = false;

export function useDocVault() {
    const [status, setStatus] = useState<Status>({ type: 'idle' });
    const [previewBlob, setPreviewBlob] = useState<Blob | null>(null);

    const loadWasm = useCallback(async () => {
        if (!wasmInitialized) {
            await init(wasmUrl);
            wasmInitialized = true;
        }
        return { encode_file, decode_file, get_version };
    }, []);

    const encodeFile = useCallback(async (file: File, password: string): Promise<Blob> => {
        try {
            setStatus({ type: 'processing', message: 'Loading encryption engine...', progress: 10 });
            const wasm = await loadWasm();

            setStatus({ type: 'processing', message: 'Reading file...', progress: 20 });
            const arrayBuffer = await file.arrayBuffer();
            const fileBytes = new Uint8Array(arrayBuffer);

            setStatus({ type: 'processing', message: 'Encrypting & encoding...', progress: 40 });
            const result = wasm.encode_file(
                fileBytes,
                file.name,
                file.type || 'application/octet-stream',
                password
            );

            setStatus({ type: 'processing', message: 'Generating image...', progress: 70 });

            // Create an OffscreenCanvas or regular canvas to render pixels
            const canvas = document.createElement('canvas');
            canvas.width = result.width;
            canvas.height = result.height;
            const ctx = canvas.getContext('2d');
            if (!ctx) {
                result.free();
                throw new Error('Failed to create canvas context');
            }

            // Create ImageData from RGBA pixels
            // The constructor creates a copy of the WASM memory
            const rgbaArray = new Uint8ClampedArray(result.rgba_pixels);
            const imageData = new ImageData(rgbaArray, result.width, result.height);
            ctx.putImageData(imageData, 0, 0);

            // Free the WASM memory now that we have a copy in the canvas
            result.free();

            setStatus({ type: 'processing', message: 'Exporting PNG...', progress: 90 });

            // Convert canvas to PNG Blob
            const blob = await new Promise<Blob>((resolve, reject) => {
                canvas.toBlob((b) => {
                    if (b) resolve(b);
                    else reject(new Error('Failed to export PNG'));
                }, 'image/png');
            });

            setPreviewBlob(blob);
            setStatus({ type: 'success', message: `Encoded successfully! (${formatSize(blob.size)} PNG)` });
            return blob;
        } catch (err: any) {
            console.error('WASM Encode Error:', err);
            const errMsg = err?.message || typeof err === 'string' ? err : 'Unknown error during encoding';
            setStatus({ type: 'error', message: `Encryption failed: ${errMsg}` });
            throw err;
        }
    }, [loadWasm]);

    const decodeFile = useCallback(async (file: File, password: string): Promise<{ blob: Blob; filename: string }> => {
        try {
            setStatus({ type: 'processing', message: 'Loading decryption engine...', progress: 10 });
            const wasm = await loadWasm();

            setStatus({ type: 'processing', message: 'Reading image...', progress: 20 });

            // Load image and extract pixel data via canvas
            const imageBitmap = await createImageBitmap(file);
            const canvas = document.createElement('canvas');
            canvas.width = imageBitmap.width;
            canvas.height = imageBitmap.height;
            const ctx = canvas.getContext('2d');
            if (!ctx) throw new Error('Failed to create canvas context');

            ctx.drawImage(imageBitmap, 0, 0);
            const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);

            setStatus({ type: 'processing', message: 'Decrypting & decoding...', progress: 50 });
            const result = wasm.decode_file(
                new Uint8Array(imageData.data.buffer),
                canvas.width,
                canvas.height,
                password
            );

            setStatus({ type: 'processing', message: 'Restoring file...', progress: 80 });

            // Copy data out of the WASM heap
            const fileBytesCopy = result.file_bytes.slice();
            const filename = result.filename;
            const mimeType = result.mime_type;

            // Free WASM memory
            result.free();

            const fileBlob = new Blob([fileBytesCopy], {
                type: mimeType || 'application/octet-stream',
            });

            setStatus({ type: 'success', message: `✅ Restored: ${filename}` });
            return { blob: fileBlob, filename: filename };
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

function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
