import { useState, useCallback } from 'react';
import type { AppMode } from './types';
import { useDocVault } from './hooks/useDocVault';
import { DropZone } from './components/DropZone';
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
    const [password, setPassword] = useState('');
    const [passwordError, setPasswordError] = useState(false);
    const { status, previewBlob, encodeFile, decodeFile, reset } = useDocVault();

    const handleModeSwitch = useCallback(
        (newMode: AppMode) => {
            setMode(newMode);
            setFile(null);
            setPassword('');
            setPasswordError(false);
            reset();
        },
        [reset]
    );

    const handleEncode = useCallback(async () => {
        if (!file) return;
        if (!password) {
            setPasswordError(true);
            return;
        }
        setPasswordError(false);
        try {
            const blob = await encodeFile(file, password);
            const encodedName = file.name.replace(/\.[^.]+$/, '') + '.vault.png';
            triggerDownload(blob, encodedName);
        } catch (err) {
            console.error('Encode error:', err);
        }
    }, [file, password, encodeFile]);

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
                                        Select any file. It will be encrypted securely and encoded visually into a PNG image.
                                    </p>
                                </div>
                                <DropZone onFile={setFile} label="Select file to encrypt" accept="*/*" file={file} onClear={() => setFile(null)} />
                            </div>

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
                                onClick={handleEncode}
                                disabled={!file || isProcessing}
                            >
                                {isProcessing ? (
                                    <div className="action-btn__spinner" />
                                ) : (
                                    <Lock size={16} />
                                )}
                                <span>{isProcessing ? 'ENCRYPTING...' : 'ENCODE & DOWNLOAD'}</span>
                            </button>
                        </>
                    ) : (
                        <>
                            <div className="card__section">
                                <div className="card__section-header">
                                    <h2 className="card__heading">Decrypt & Restore</h2>
                                    <p className="card__description">
                                        Select an image encoded by DocVault to decrypt and restore the original file.
                                    </p>
                                </div>
                                <DropZone onFile={setFile} label="Select .vault.png image" accept="image/png" file={file} onClear={() => setFile(null)} />
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
