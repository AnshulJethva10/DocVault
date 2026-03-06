interface ProgressBarProps {
    progress: number;
    message: string;
}

export function ProgressBar({ progress, message }: ProgressBarProps) {
    // Convert 0-1 to 0-100%
    const percent = Math.round(progress * 100);

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
