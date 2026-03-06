import React, { useState, useCallback, useEffect } from 'react';
import { Eye, EyeOff } from 'lucide-react';

interface PasswordInputProps {
    value: string;
    onChange: (v: string) => void;
    label: string;
    error?: boolean;
}

function getStrengthLevel(password: string): number {
    if (password.length === 0) return 0;
    let score = 0;
    if (password.length >= 8) score++;
    if (password.length >= 12) score++;
    if (password.length >= 16) score++;
    if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score++;
    if (/\d/.test(password)) score++;
    if (/[^a-zA-Z0-9]/.test(password)) score++;

    if (score <= 2) return 1; // Weak
    if (score <= 4) return 2; // Fair
    if (score >= 5) return 4; // Strong (shows 4 bars)
    return Math.min(score, 4);
}

const STRENGTH_COLORS = {
    0: 'var(--border-default)',
    1: 'var(--error)',    // Weak
    2: 'var(--warning)',  // Fair
    3: 'var(--warning)',  // Fair (3 bars)
    4: 'var(--success)'   // Strong
};

const STRENGTH_LABELS = {
    0: '',
    1: 'Weak',
    2: 'Fair',
    3: 'Fair',
    4: 'Strong'
};

export function PasswordInput({ value, onChange, error }: PasswordInputProps) {
    const [visible, setVisible] = useState(false);
    const [level, setLevel] = useState(0);

    // Update level with a slight delay if we wanted to stagger, 
    // but the actual requirements asked for staggered fill animations on the segments.
    // The staggered fill is handled in CSS/React by applying a delay to each segment.
    useEffect(() => {
        setLevel(getStrengthLevel(value));
    }, [value]);

    const handleToggle = useCallback((e: React.MouseEvent) => {
        e.preventDefault();
        e.stopPropagation();
        setVisible((v) => !v);
    }, []);

    const color = STRENGTH_COLORS[level as keyof typeof STRENGTH_COLORS];
    const label = STRENGTH_LABELS[level as keyof typeof STRENGTH_LABELS];

    return (
        <div className="password-input" id="password-input">
            <div className={`password-input__wrapper ${error ? 'password-input__wrapper--error' : ''}`}>
                <input
                    type={visible ? 'text' : 'password'}
                    value={value}
                    onChange={(e) => onChange(e.target.value)}
                    className="password-input__field"
                    placeholder="enter passphrase..."
                    autoComplete="off"
                />
                <button
                    type="button"
                    className="password-input__toggle"
                    onClick={handleToggle}
                    aria-label={visible ? 'Hide password' : 'Show password'}
                >
                    {visible ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
            </div>

            <div className="strength">
                <div className="strength__bars">
                    {[0, 1, 2, 3].map((index) => {
                        const isFilled = index < level;
                        return (
                            <div
                                key={index}
                                className={`strength__segment ${isFilled ? 'strength__segment--filled' : ''}`}
                                style={isFilled ? {
                                    backgroundColor: color,
                                    animationDelay: `${index * 100}ms`
                                } : {}}
                            />
                        );
                    })}
                </div>
                {value.length > 0 && (
                    <span
                        className="strength__label"
                        style={{ color }}
                    >
                        {label}
                    </span>
                )}
            </div>
        </div>
    );
}
