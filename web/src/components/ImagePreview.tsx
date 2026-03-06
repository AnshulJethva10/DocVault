import { useState, useEffect } from 'react';

interface ImagePreviewProps {
    blob: Blob | null;
}

function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ImagePreview({ blob }: ImagePreviewProps) {
    const [url, setUrl] = useState<string | null>(null);
    const [dimensions, setDimensions] = useState<{ w: number; h: number } | null>(null);

    useEffect(() => {
        if (!blob) {
            setUrl(null);
            setDimensions(null);
            return;
        }

        const objectUrl = URL.createObjectURL(blob);
        setUrl(objectUrl);

        // Load image to get dimensions
        const img = new Image();
        img.onload = () => {
            setDimensions({ w: img.naturalWidth, h: img.naturalHeight });
        };
        img.src = objectUrl;

        return () => {
            URL.revokeObjectURL(objectUrl);
        };
    }, [blob]);

    if (!blob || !url) return null;

    return (
        <div className="image-preview" id="image-preview">
            <span className="section-label">Encoded View</span>
            <div className="image-preview__container">
                <img src={url} alt="Encoded document" className="image-preview__img" />
            </div>
            <div className="image-preview__info">
                {dimensions && (
                    <span>
                        {dimensions.w} × {dimensions.h} PX
                    </span>
                )}
                <span>{formatSize(blob.size)}</span>
            </div>
        </div>
    );
}
