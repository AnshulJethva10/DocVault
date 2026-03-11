import type { BatchFileItem } from '../types';
import {
    File,
    FileText,
    Image as ImageIcon,
    Video,
    Archive,
    Code,
    Check,
    X,
    Download,
    Loader2,
    Clock,
} from 'lucide-react';

interface BatchFileListProps {
    items: BatchFileItem[];
    onRemove: (id: string) => void;
    onDownload: (item: BatchFileItem) => void;
    onDownloadAll: () => void;
    disabled?: boolean;
}

function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function getFileIcon(type: string, name: string) {
    if (type.startsWith('image/')) return <ImageIcon size={16} />;
    if (type.startsWith('video/')) return <Video size={16} />;
    if (type.startsWith('text/') || name.endsWith('.md')) return <FileText size={16} />;
    if (type.includes('zip') || type.includes('tar') || type.includes('compressed')) return <Archive size={16} />;
    if (type.includes('json') || name.match(/\.(ts|js|jsx|tsx|css|html|py|rs)$/)) return <Code size={16} />;
    return <File size={16} />;
}

function StatusIcon({ status }: { status: BatchFileItem['status'] }) {
    switch (status) {
        case 'pending':
            return <Clock size={14} className="batch-item__status-icon batch-item__status-icon--pending" />;
        case 'processing':
            return <Loader2 size={14} className="batch-item__status-icon batch-item__status-icon--processing" />;
        case 'done':
            return <Check size={14} className="batch-item__status-icon batch-item__status-icon--done" />;
        case 'error':
            return <X size={14} className="batch-item__status-icon batch-item__status-icon--error" />;
    }
}

export function BatchFileList({ items, onRemove, onDownload, onDownloadAll, disabled }: BatchFileListProps) {
    if (items.length === 0) return null;

    const doneCount = items.filter((i) => i.status === 'done').length;
    const allDone = items.every((i) => i.status === 'done' || i.status === 'error');
    const hasAnyDone = doneCount > 0;

    return (
        <div className="batch-list" id="batch-file-list">
            <div className="batch-list__header">
                <span className="section-label">File Queue</span>
                <span className="batch-list__count">{items.length} file{items.length !== 1 ? 's' : ''}</span>
            </div>

            <div className="batch-list__items">
                {items.map((item) => (
                    <div
                        key={item.id}
                        className={`batch-item batch-item--${item.status}`}
                    >
                        <div className="batch-item__left">
                            <StatusIcon status={item.status} />
                            <div className="batch-item__file-icon">
                                {getFileIcon(item.file.type, item.file.name)}
                            </div>
                            <div className="batch-item__info">
                                <span className="batch-item__name" title={item.file.name}>
                                    {item.file.name}
                                </span>
                                <span className="batch-item__meta">
                                    {item.status === 'pending' && formatFileSize(item.file.size)}
                                    {item.status === 'processing' && item.message}
                                    {item.status === 'done' && item.message}
                                    {item.status === 'error' && (
                                        <span className="batch-item__error-text">{item.error}</span>
                                    )}
                                </span>
                            </div>
                        </div>

                        <div className="batch-item__right">
                            {item.status === 'processing' && (
                                <div className="batch-item__progress-track">
                                    <div
                                        className="batch-item__progress-fill"
                                        style={{ width: `${item.progress}%` }}
                                    />
                                </div>
                            )}
                            {item.status === 'done' && item.outputBlob && (
                                <button
                                    className="batch-item__download"
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onDownload(item);
                                    }}
                                    title="Download"
                                    type="button"
                                >
                                    <Download size={14} />
                                </button>
                            )}
                            {item.status === 'pending' && !disabled && (
                                <button
                                    className="batch-item__remove"
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onRemove(item.id);
                                    }}
                                    title="Remove"
                                    type="button"
                                >
                                    <X size={14} />
                                </button>
                            )}
                        </div>
                    </div>
                ))}
            </div>

            {allDone && hasAnyDone && doneCount > 1 && (
                <button
                    className="batch-list__download-all"
                    onClick={onDownloadAll}
                    type="button"
                >
                    <Download size={14} />
                    <span>Download All ({doneCount})</span>
                </button>
            )}
        </div>
    );
}
