// The grid's thumbnail size (Small, Medium, Large), remembered on this
// computer. Shared by the toolbar's switch and the grid.

import { useSyncExternalStore } from "react";

export type ThumbSize = "small" | "medium" | "large";

export const THUMB_SIZES: { size: ThumbSize; label: string; px: number }[] = [
  { size: "small", label: "Small", px: 120 },
  { size: "medium", label: "Medium", px: 176 },
  { size: "large", label: "Large", px: 280 },
];

const KEY = "keepsake.thumbSize";
const listeners = new Set<() => void>();

function read(): ThumbSize {
  try {
    const v = localStorage.getItem(KEY);
    return v === "small" || v === "large" ? v : "medium";
  } catch {
    return "medium";
  }
}

let current: ThumbSize = read();

export function setThumbSize(size: ThumbSize) {
  current = size;
  try {
    localStorage.setItem(KEY, size);
  } catch {
    // Not remembered: still applies now.
  }
  listeners.forEach((l) => l());
}

/** One step bigger (+1) or smaller (−1); stays at the ends. */
export function stepThumbSize(by: 1 | -1) {
  const i = THUMB_SIZES.findIndex((t) => t.size === current);
  const next = THUMB_SIZES[Math.max(0, Math.min(THUMB_SIZES.length - 1, i + by))];
  setThumbSize(next.size);
}

export function useThumbSize(): ThumbSize {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}

/** Tile width and height in CSS pixels. */
export const tilePx = (size: ThumbSize) => THUMB_SIZES.find((t) => t.size === size)?.px ?? 176;

/** Large tiles, and tiles a dense screen draws with over 300 pixels, use the 1024 px thumbnail. */
export const wantsLarge = (size: ThumbSize, dpr: number) =>
  size === "large" || tilePx(size) * dpr > 300;
