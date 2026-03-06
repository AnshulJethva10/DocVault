import type { Status } from '../types';
import { X, Check } from 'lucide-react';

interface StatusMessageProps {
    status: Status;
}

export function StatusMessage({ status }: StatusMessageProps) {
    // Only show error or success in this new design standard when idle.
    // Progress logic is moved out to ProgressBar in the new layout.
    if (status.type === 'idle' || status.type === 'processing') return null;

    const isError = status.type === 'error';
    const Icon = isError ? X : Check;

    return (
        <div
            className={`status-message ${isError ? 'status-message--error' : 'status-message--success'}`}
            id="status-message"
            role="alert"
        >
            <Icon size={12} strokeWidth={3} />
            <span>{status.message}</span>
        </div>
    );
}
