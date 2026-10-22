export type AppMode = 'encode' | 'decode';

export type Status =
    | { type: 'idle' }
    | { type: 'processing'; message: string; progress: number }
    | { type: 'success'; message: string }
    | { type: 'error'; message: string };

/** Represents one file in the batch encode queue */
export interface BatchFileItem {
    id: string;
    file: File;
    status: 'pending' | 'processing' | 'done' | 'error';
    progress: number;
    message: string;
    outputBlob?: Blob;
    outputFilename?: string;
    isVideo?: boolean;
    error?: string;
}

export type BatchStatus =
    | { type: 'idle' }
    | { type: 'processing'; currentIndex: number; total: number }
    | { type: 'done'; successCount: number; errorCount: number };
