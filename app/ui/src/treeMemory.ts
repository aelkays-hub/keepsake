// Where the sidebar's tree was left: which levels are open and how far it
// is scrolled, for each tree (By date, By folder). Kept in this computer's
// web storage, so the tree comes back as it was after a restart; if the
// storage cannot be used, the tree simply starts at the top.

export type TreeName = "date" | "folder";

const OPEN_KEY = (t: TreeName) => `keepsake.tree.${t}.open`;
const SCROLL_KEY = (t: TreeName) => `keepsake.tree.${t}.scroll`;
/** Most open levels kept (the oldest are dropped). */
export const MAX_OPEN = 400;

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not kept: fine.
  }
}

/** The levels left open, or `fallback` the first time. */
export function openLevels(tree: TreeName, fallback: string[]): Set<string> {
  const raw = read(OPEN_KEY(tree));
  if (raw === null) return new Set(fallback);
  try {
    const v: unknown = JSON.parse(raw);
    if (Array.isArray(v) && v.every((k) => typeof k === "string")) return new Set(v);
  } catch {
    // Unreadable: start again.
  }
  return new Set(fallback);
}

export function keepOpenLevels(tree: TreeName, open: Set<string>) {
  write(OPEN_KEY(tree), JSON.stringify([...open].slice(-MAX_OPEN)));
}

/** How far the tree was scrolled (pixels; 0 the first time). */
export function scrollOf(tree: TreeName): number {
  const n = Number(read(SCROLL_KEY(tree)));
  return Number.isFinite(n) && n > 0 ? Math.round(n) : 0;
}

export function keepScroll(tree: TreeName, top: number) {
  write(SCROLL_KEY(tree), String(Math.max(0, Math.round(top))));
}
