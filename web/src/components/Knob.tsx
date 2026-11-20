/** A slider plus a number box that sends its value on release, Enter or blur (not while dragging). */
import { useState } from "react";

export interface KnobProps {
  label: string;
  value: number;
  min?: number;
  max: number;
  step: number;
  unit: string;
  title?: string;
  onCommit: (x: number) => void;
}

export function Knob({ label, value, min = 0, max, step, unit, title, onCommit }: KnobProps) {
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? value;
  const commit = () => {
    if (draft !== null && Number.isFinite(draft) && draft !== value) onCommit(Math.min(max, Math.max(min, draft)));
    setDraft(null);
  };
  return (
    <div className="knob" title={title}>
      <span className="knob-label">{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={Math.min(Math.max(shown, min), max)}
        aria-label={label}
        onChange={(e) => setDraft(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <input
        type="number"
        min={min}
        max={max}
        step={step}
        value={Math.round(shown * 100) / 100}
        aria-label={`${label} value`}
        onChange={(e) => setDraft(Number(e.target.value))}
        onKeyDown={(e) => e.key === "Enter" && commit()}
        onBlur={commit}
        style={{ width: 70 }}
      />
      <span className="muted small">{unit}</span>
    </div>
  );
}
