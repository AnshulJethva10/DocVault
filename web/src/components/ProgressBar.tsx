interface ProgressBarProps {
    progress: number;
    message: string;
}

export function ProgressBar({ progress, message }: ProgressBarProps) {
    // progress is already 0–100 from the hook
    const percent = Math.min(100, Math.max(0, Math.round(progress)));

    return (
        <div className="progress-container" id="progress-bar" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
            <div className="progress-header">
                <span>{message}</span>
                <span>{percent}%</span>
            </div>
            <div className="progress-track">
                <div
                    className="progress-fill"
                    style={{ width: `${percent}%` }}
                />
            </div>
        </div>
    );
}
