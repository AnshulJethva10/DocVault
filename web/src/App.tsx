import { useState, useCallback } from 'react';
import type { AppMode, BatchFileItem } from './types';
import { useDocVault } from './hooks/useDocVault';
import { DropZone } from './components/DropZone';
import { BatchDropZone } from './components/BatchDropZone';
import { BatchFileList } from './components/BatchFileList';
import { PasswordInput } from './components/PasswordInput';
import { ProgressBar } from './components/ProgressBar';
import { ImagePreview } from './components/ImagePreview';
import { StatusMessage } from './components/StatusMessage';
import { Lock, Unlock } from 'lucide-react';

function triggerDownload(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

export default function App() {
    const [mode, setMode] = useState<AppMode>('encode');
    const [file, setFile] = useState<File | null>(null);
    const [batchFiles, setBatchFiles] = useState<BatchFileItem[]>([]);
    const [password, setPassword] = useState('');
    const [passwordError, setPasswordError] = useState(false);
    const { status, previewBlob, encodeFile, encodeBatch, decodeFile, reset } = useDocVault();

    const handleModeSwitch = useCallback(
        (newMode: AppMode) => {
            setMode(newMode);
            setFile(null);
            setBatchFiles([]);
            setPassword('');
            setPasswordError(false);
            reset();
        },
        [reset]
    );

    // --- Batch encode helpers ---
    const handleAddFiles = useCallback((files: File[]) => {
        const newItems: BatchFileItem[] = files.map((f) => ({
            id: crypto.randomUUID(),
            file: f,
            status: 'pending' as const,
            progress: 0,
            message: '',
        }));
        setBatchFiles((prev) => [...prev, ...newItems]);
    }, []);

    const handleRemoveFile = useCallback((id: string) => {
        setBatchFiles((prev) => prev.filter((f) => f.id !== id));
    }, []);

    const handleFileDownload = useCallback((item: BatchFileItem) => {
        if (item.outputBlob && item.outputFilename) {
            triggerDownload(item.outputBlob, item.outputFilename);
        }
    }, []);

    const handleDownloadAll = useCallback(() => {
        batchFiles
            .filter((f) => f.status === 'done' && f.outputBlob && f.outputFilename)
            .forEach((f) => {
                triggerDownload(f.outputBlob!, f.outputFilename!);
            });
    }, [batchFiles]);

    const handleBatchEncode = useCallback(async () => {
        if (batchFiles.length === 0) return;
        if (!password) {
            setPasswordError(true);
            return;
        }
        setPasswordError(false);

        // If only one file, use single-file flow for auto-download + preview
        if (batchFiles.length === 1) {
            try {
                const singleFile = batchFiles[0].file;
                const { blob, isVideo } = await encodeFile(singleFile, password);
                const ext = isVideo ? '.vault.avi' : '.vault.png';
                const encodedName = singleFile.name.replace(/\.[^.]+$/, '') + ext;
                triggerDownload(blob, encodedName);

                // Update batch item state to reflect done
                setBatchFiles((prev) =>
                    prev.map((f) => ({
                        ...f,
                        status: 'done' as const,
                        progress: 100,
                        message: `Done`,
                        outputBlob: blob,
                        outputFilename: encodedName,
                        isVideo,
                    }))
                );
            } catch {
                // encodeFile already sets error status
            }
            return;
        }

        await encodeBatch(batchFiles, password, (fileId, patch) => {
            setBatchFiles((prev) =>
                prev.map((f) => (f.id === fileId ? { ...f, ...patch } : f))
            );
        });
    }, [batchFiles, password, encodeFile, encodeBatch]);

    const handleDecode = useCallback(async () => {
        if (!file) return;
        if (!password) {
            setPasswordError(true);
            return;
        }
        setPasswordError(false);
        try {
            const { blob, filename } = await decodeFile(file, password);
            triggerDownload(blob, filename);
        } catch (err) {
            console.error('Decode error:', err);
        }
    }, [file, password, decodeFile]);

    const isProcessing = status.type === 'processing';

    return (
        <div className="app">
            <header className="header">
                <div className="header__brand">
                    {/* Minimal geometric diamond/shield drawn in indigo */}
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#6366F1" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M12 2L2 12l10 10 10-10L12 2z" />
                    </svg>
                    <div>
                        <h1 className="header__title">DocVault</h1>
                    </div>
                </div>
                <div className="header__badge">
                    <span className="header__badge-text">v1.0 · AES-256-GCM · WASM</span>
                    <div className="header__badge-dot"></div>
                </div>
            </header>

            <main className="main">
                <div className="tabs" id="mode-tabs">
                    <button
                        className={`tab ${mode === 'encode' ? 'tab--active' : ''}`}
                        onClick={() => handleModeSwitch('encode')}
                        disabled={isProcessing}
                    >
                        <Lock size={12} />
                        <span>Encode</span>
                    </button>
                    <button
                        className={`tab ${mode === 'decode' ? 'tab--active' : ''}`}
                        onClick={() => handleModeSwitch('decode')}
                        disabled={isProcessing}
                    >
                        <Unlock size={12} />
                        <span>Decode</span>
                    </button>
                </div>

                <div className="card">
                    {mode === 'encode' ? (
                        <>
                            <div className="card__section">
                                <div className="card__section-header">
                                    <h2 className="card__heading">Encrypt & Disguise</h2>
                                    <p className="card__description">
                                        Select one or more files. Each will be encrypted and encoded into a PNG (≤3 MB) or an AVI video (larger files).
                                    </p>
                                </div>
                                <BatchDropZone
                                    onFiles={handleAddFiles}
                                    label="Select files to encrypt"
                                    accept="*/*"
                                    fileCount={batchFiles.length}
                                    disabled={isProcessing}
                                />
                            </div>

                            <BatchFileList
                                items={batchFiles}
                                onRemove={handleRemoveFile}
                                onDownload={handleFileDownload}
                                onDownloadAll={handleDownloadAll}
                                disabled={isProcessing}
                            />

                            <div className="password-section">
                                <span className="section-label">Encryption Key</span>
                                <PasswordInput
                                    value={password}
                                    onChange={(v) => {
                                        setPassword(v);
                                        setPasswordError(false);
                                    }}
                                    label="Encryption Password"
                                    error={passwordError}
                                />
                            </div>

                            <button
                                className="action-btn"
                                onClick={handleBatchEncode}
                                disabled={batchFiles.length === 0 || isProcessing}
                            >
                                {isProcessing ? (
                                    <div className="action-btn__spinner" />
                                ) : (
                                    <Lock size={16} />
                                )}
                                <span>
                                    {isProcessing
                                        ? 'ENCRYPTING...'
                                        : batchFiles.length > 1
                                            ? `ENCODE ${batchFiles.length} FILES`
                                            : 'ENCODE & DOWNLOAD'}
                                </span>
                            </button>
                        </>
                    ) : (
                        <>
                            <div className="card__section">
                                <div className="card__section-header">
                                    <h2 className="card__heading">Decrypt & Restore</h2>
                                    <p className="card__description">
                                        Select a DocVault file (.vault.png or .vault.avi) to decrypt and restore the original.
                                    </p>
                                </div>
                                <DropZone onFile={setFile} label="Select .vault.png or .vault.avi" accept="image/png,video/avi,video/x-msvideo,.avi" file={file} onClear={() => setFile(null)} />
                            </div>

                            <div className="password-section">
                                <span className="section-label">Decryption Key</span>
                                <PasswordInput
                                    value={password}
                                    onChange={(v) => {
                                        setPassword(v);
                                        setPasswordError(false);
                                    }}
                                    label="Decryption Password"
                                    error={passwordError}
                                />
                            </div>

                            <button
                                className="action-btn"
                                onClick={handleDecode}
                                disabled={!file || isProcessing}
                            >
                                {isProcessing ? (
                                    <div className="action-btn__spinner" />
                                ) : (
                                    <Unlock size={16} />
                                )}
                                <span>{isProcessing ? 'DECRYPTING...' : 'DECODE & RESTORE'}</span>
                            </button>
                        </>
                    )}

                    {status.type === 'processing' && (
                        <ProgressBar progress={status.progress} message={status.message} />
                    )}

                    {mode === 'encode' && <ImagePreview blob={previewBlob} />}

                    <StatusMessage status={status} />
                </div>
            </main>

            <footer className="footer">
                <span className="footer-item">AES-256-GCM</span>
                <span className="footer-dot">·</span>
                <span className="footer-item">PBKDF2 × 310K</span>
                <span className="footer-dot">·</span>
                <span className="footer-item">RUST + WASM</span>
            </footer>
        </div>
    );
}
