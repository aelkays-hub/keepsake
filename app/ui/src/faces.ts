// Recognise faces (ADR-040): what Settings says about it.

import type { FacesStatus } from "./api";
import * as fmt from "./format";

/** One line on how far finding faces has got. */
export function facesProgress(st: FacesStatus): string {
  if (!st.aiEnabled) return "Needs On-device AI (just above).";
  if (!st.enabled) return "Off.";
  if (st.problem) return `Not running: ${st.problem}`;
  if (!st.accel && st.pending > 0) return "Starting…";
  const found =
    st.faces === 0
      ? `${fmt.count(st.scanned)} of ${fmt.count(st.photos)} photos searched; no faces yet`
      : `${fmt.count(st.faces)} ${st.faces === 1 ? "face" : "faces"} in ` +
        `${fmt.count(st.withFaces)} of ${fmt.count(st.scanned)} photos searched ` +
        `(${fmt.count(st.photos)} photos in all)`;
  const where = st.accel ? ` On the ${st.accel}.` : "";
  return st.pending > 0
    ? `${found}; ${fmt.count(st.pending)} waiting.${where}`
    : `${found}.${where}`;
}

/** A face's box, widened by `margin` of its size on each side (kept in the
 * picture): faces are found tight. */
export function widen(
  f: { x: number; y: number; w: number; h: number },
  margin = 0.2,
): { x: number; y: number; w: number; h: number } {
  const x0 = Math.max(0, f.x - f.w * margin);
  const y0 = Math.max(0, f.y - f.h * margin);
  const x1 = Math.min(1, f.x + f.w * (1 + margin));
  const y1 = Math.min(1, f.y + f.h * (1 + margin));
  return { x: x0, y: y0, w: Math.max(x1 - x0, 1e-6), h: Math.max(y1 - y0, 1e-6) };
}

/** The CSS that shows only a face, `width` px wide, cut out of its
 * picture (`pw` × `ph` px) at its own shape. */
export function cropStyle(
  f: { x: number; y: number; w: number; h: number; pw: number; ph: number },
  width: number,
): { width: string; height: string; backgroundSize: string; backgroundPosition: string } {
  const b = widen(f);
  const aspect = (b.h * f.ph) / (b.w * f.pw);
  const height = Math.round(width * Math.min(Math.max(aspect, 0.5), 2));
  const pos = (start: number, size: number) => (size >= 1 ? 0 : (start / (1 - size)) * 100);
  return {
    width: `${width}px`,
    height: `${height}px`,
    backgroundSize: `${(100 / b.w).toFixed(3)}% ${(100 / b.h).toFixed(3)}%`,
    backgroundPosition: `${pos(b.x, b.w).toFixed(3)}% ${pos(b.y, b.h).toFixed(3)}%`,
  };
}

/** Faces grouped by the year their photo was taken, newest year first, in
 * their order inside a year ("Undated" last). */
export function byYear<T extends { date: string | null }>(faces: T[]): [string, T[]][] {
  const groups = new Map<string, T[]>();
  for (const f of faces) {
    const y = f.date?.slice(0, 4) ?? "Undated";
    groups.set(y, [...(groups.get(y) ?? []), f]);
  }
  return [...groups.entries()].sort(([a], [b]) =>
    a === "Undated" ? 1 : b === "Undated" ? -1 : b.localeCompare(a),
  );
}

/** What the Find step says while photos are searched. */
export function searchLine(st: FacesStatus | null): string {
  if (!st) return "Getting ready…";
  if (st.problem) return `Not running: ${st.problem}`;
  if (st.pending === 0) return "Every photo is searched.";
  return (
    `Searching photos for faces: ${fmt.count(st.scanned)} of ${fmt.count(st.photos)} done ` +
    `(${fmt.count(st.pending)} to go). You can close this window: it goes on in the background.`
  );
}
