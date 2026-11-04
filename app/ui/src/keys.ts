// The keyboard map. Pure: turns a key press into an action (unit-tested).

export type Action =
  | {
      kind: "move";
      by: "left" | "right" | "up" | "down" | "pageUp" | "pageDown" | "home" | "end";
      extend: boolean;
    }
  | { kind: "selectAll" }
  | { kind: "clear" }
  | { kind: "open" }
  | { kind: "up" }
  | { kind: "rate"; stars: number }
  | { kind: "label"; label: string | null }
  | { kind: "view"; mode: "calendar" | "grid" | "details" }
  | { kind: "search" }
  | { kind: "toggleSelect" };

export interface KeyLike {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

/** Lightroom's colour-label keys. */
export const LABEL_KEYS: Record<string, string> = {
  "6": "Red",
  "7": "Yellow",
  "8": "Green",
  "9": "Blue",
};

const MOVES: Record<string, Extract<Action, { kind: "move" }>["by"]> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
  PageUp: "pageUp",
  PageDown: "pageDown",
  Home: "home",
  End: "end",
};

/** The action for a key press in the main area, or null. */
export function actionFor(e: KeyLike): Action | null {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && !e.altKey) {
    switch (e.key.toLowerCase()) {
      case "a":
        return { kind: "selectAll" };
      case "f":
        return { kind: "search" };
      case "1":
        return { kind: "view", mode: "calendar" };
      case "2":
        return { kind: "view", mode: "grid" };
      case "3":
        return { kind: "view", mode: "details" };
    }
    if (e.key === " ") return { kind: "toggleSelect" };
    return null;
  }
  if (e.altKey) return null;
  const by = MOVES[e.key];
  if (by) return { kind: "move", by, extend: e.shiftKey };
  switch (e.key) {
    case "Escape":
      return { kind: "clear" };
    case "Enter":
      return { kind: "open" };
    case "Backspace":
      return { kind: "up" };
  }
  if (/^[0-5]$/.test(e.key)) return { kind: "rate", stars: Number(e.key) };
  if (e.key in LABEL_KEYS) return { kind: "label", label: LABEL_KEYS[e.key] };
  return null;
}

/** New focus index for a move in a grid of `cols` columns. */
export function moveIndex(
  from: number | null,
  by: Extract<Action, { kind: "move" }>["by"],
  count: number,
  cols: number,
  pageRows: number,
): number | null {
  if (count === 0) return null;
  const i = from ?? -1;
  const clamp = (v: number) => Math.max(0, Math.min(count - 1, v));
  switch (by) {
    case "left":
      return clamp(i < 0 ? 0 : i - 1);
    case "right":
      return clamp(i + 1);
    case "up":
      return clamp(i < 0 ? 0 : i - cols);
    case "down":
      return clamp(i < 0 ? 0 : i + cols);
    case "pageUp":
      return clamp(i - cols * pageRows);
    case "pageDown":
      return clamp((i < 0 ? 0 : i) + cols * pageRows);
    case "home":
      return 0;
    case "end":
      return count - 1;
  }
}
