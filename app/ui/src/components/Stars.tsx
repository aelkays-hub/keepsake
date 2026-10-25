// Rating stars and colour-label swatches, used by tiles, rows and the
// inspector.

import { LABELS } from "../labels";

export function Stars({
  value,
  onChange,
  size = "md",
}: {
  value: number;
  onChange?: (v: number) => void;
  size?: "sm" | "md";
}) {
  if (!onChange) {
    return value > 0 ? (
      <span className={`stars ${size}`} aria-label={`${value} stars`}>
        {"★".repeat(value)}
      </span>
    ) : null;
  }
  return (
    <div className="stars-input" role="radiogroup" aria-label="Rating">
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          type="button"
          role="radio"
          aria-checked={value === n}
          aria-label={`${n} star${n > 1 ? "s" : ""}`}
          className={n <= value ? "on" : ""}
          onClick={() => onChange(value === n ? 0 : n)}
          title={`${n} (key ${n}; click again to clear)`}
        >
          ★
        </button>
      ))}
    </div>
  );
}

export function LabelPicker({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (v: string | null) => void;
}) {
  return (
    <div className="labels-input" role="radiogroup" aria-label="Colour label">
      {LABELS.map((l) => (
        <button
          key={l.name}
          type="button"
          role="radio"
          aria-checked={value?.toLowerCase() === l.name.toLowerCase()}
          aria-label={l.name}
          title={l.key ? `${l.name} (key ${l.key})` : l.name}
          style={{ background: l.color }}
          onClick={() => onChange(value?.toLowerCase() === l.name.toLowerCase() ? null : l.name)}
        />
      ))}
      <button type="button" className="none" aria-label="No label" onClick={() => onChange(null)}>
        ✕
      </button>
    </div>
  );
}
