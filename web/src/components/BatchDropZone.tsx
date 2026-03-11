import React, { useRef, useState, useCallback } from 'react';
import { UploadCloud, Files } from 'lucide-react';

interface BatchDropZoneProps {
    onFiles: (files: File[]) => void;
    label: string;
    accept: string;
    fileCount: number;
    disabled?: boolean;
}

export function BatchDropZone({ onFiles, label, accept, fileCount, disabled }: BatchDropZoneProps) {
    const [dragOver, setDragOver] = useState(false);
    const inputRef = useRef<HTMLInputElement>(null);

    const handleDrop = useCallback(
        (e: React.DragEvent<HTMLDivElement>) => {
            e.preventDefault();
            e.stopPropagation();
            setDragOver(false);
            if (disabled) return;
            const files = Array.from(e.dataTransfer.files);
            if (files.length > 0) {
                onFiles(files);
            }
        },
        [onFiles, disabled]
    );

    const handleDragOver = useCallback((e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        if (!disabled) setDragOver(true);
    }, [disabled]);

    const handleDragLeave = useCallback((e: React.DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        e.stopPropagation();
        setDragOver(false);
    }, []);

    const handleClick = useCallback(() => {
        if (!disabled) inputRef.current?.click();
    }, [disabled]);

    const handleChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) => {
            const selected = Array.from(e.target.files || []);
            if (selected.length > 0) {
                onFiles(selected);
            }
            // Reset input so re-selecting same files works
            if (inputRef.current) inputRef.current.value = '';
        },
        [onFiles]
    );

    return (
        <div
            id="batch-dropzone"
            className={`dropzone batch-dropzone ${dragOver ? 'dropzone--active' : ''} ${disabled ? 'batch-dropzone--disabled' : ''}`}
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
                multiple
            />
            <UploadCloud className="dropzone__icon" size={24} />
            <span className="dropzone__label">
                {dragOver ? 'Release to add files' : label}
            </span>
            <span className="dropzone__hint">
                Drag & drop or click · multiple files supported
            </span>
            {fileCount > 0 && (
                <div className="batch-dropzone__badge">
                    <Files size={12} />
                    <span>{fileCount} file{fileCount !== 1 ? 's' : ''} queued</span>
                </div>
            )}
        </div>
    );
}
