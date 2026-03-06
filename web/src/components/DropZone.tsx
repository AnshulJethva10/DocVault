import React, { useRef, useState, useCallback } from 'react';
import { UploadCloud, File, FileText, Image as ImageIcon, Video, Archive, Code, X } from 'lucide-react';

interface DropZoneProps {
    onFile: (file: File | null) => void;
    label: string;
    accept: string;
    file: File | null;
    onClear: () => void;
}

function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function getFileIcon(type: string, name: string) {
    if (type.startsWith('image/')) return <ImageIcon size={20} className="dropzone__file-icon" />;
    if (type.startsWith('video/')) return <Video size={20} className="dropzone__file-icon" />;
    if (type.startsWith('text/') || name.endsWith('.md')) return <FileText size={20} className="dropzone__file-icon" />;
    if (type.includes('zip') || type.includes('tar') || type.includes('compressed')) return <Archive size={20} className="dropzone__file-icon" />;
    if (type.includes('json') || name.match(/\.(ts|js|jsx|tsx|css|html|py|rs)$/)) return <Code size={20} className="dropzone__file-icon" />;
    return <File size={20} className="dropzone__file-icon" />;
}

export function DropZone({ onFile, label, accept, file, onClear }: DropZoneProps) {
    const [dragOver, setDragOver] = useState(false);
    const inputRef = useRef<HTMLInputElement>(null);

    const handleDrop = useCallback(
        (e: React.DragEvent<HTMLDivElement>) => {
            e.preventDefault();
            e.stopPropagation();
            setDragOver(false);
            const droppedFile = e.dataTransfer.files[0];
            if (droppedFile) {
                onFile(droppedFile);
            }
        },
        [onFile]
    );

    const handleDragOver = useCallback((e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setDragOver(true);
    }, []);

    const handleDragLeave = useCallback((e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setDragOver(false);
    }, []);

    const handleClick = useCallback(() => {
        inputRef.current?.click();
    }, []);

    const handleChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) => {
            const selectedFile = e.target.files?.[0];
            if (selectedFile) {
                onFile(selectedFile);
            }
        },
        [onFile]
    );

    if (file) {
        return (
            <div className="dropzone dropzone--has-file">
                <div className="dropzone__file-info">
                    {getFileIcon(file.type, file.name)}
                    <div style={{ display: 'flex', flexDirection: 'column' }}>
                        <span className="dropzone__filename" title={file.name}>{file.name}</span>
                        <div style={{ display: 'flex', alignItems: 'center' }}>
                            <span className="dropzone__filesize">{formatFileSize(file.size)}</span>
                            {file.type && (
                                <>
                                    <span style={{ color: 'var(--text-tertiary)', margin: '0 4px', fontSize: '10px' }}>·</span>
                                    <span className="dropzone__filesize">{file.type}</span>
                                </>
                            )}
                        </div>
                    </div>
                </div>
                <button
                    className="dropzone__clear"
                    onClick={(e) => {
                        e.stopPropagation();
                        if (inputRef.current) inputRef.current.value = '';
                        onClear();
                    }}
                    type="button"
                    aria-label="Remove file"
                    style={{ background: 'none', border: 'none', cursor: 'pointer', padding: '4px' }}
                >
                    <X size={16} />
                </button>
            </div>
        );
    }

    return (
        <div
            id="dropzone"
            className={`dropzone ${dragOver ? 'dropzone--active' : ''}`}
            onDrop={handleDrop}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onClick={handleClick}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') handleClick();
            }}
        >
            <input
                ref={inputRef}
                type="file"
                accept={accept}
                onChange={handleChange}
                className="dropzone__input"
            />
            <UploadCloud className="dropzone__icon" size={24} />
            <span className="dropzone__label">{dragOver ? 'Release to select' : label}</span>
            <span className="dropzone__hint">Drag & drop or click to browse</span>
        </div>
    );
}
