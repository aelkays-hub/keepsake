// Accessibility helpers: roving focus for virtualized lists, focus traps
// for dialogs, and spoken descriptions of items.

import { useEffect, useRef, type RefObject } from "react";

import type { Row } from "./api";
import * as fmt from "./format";

/**
 * While keyboard focus is inside `container`, keep it on the element for
 * item `index` (`[data-index="…"]`), so screen readers announce the item
 * the arrows moved to. Keys still reach the container's handler.
 */
export function useRovingFocus(
  container: RefObject<HTMLElement>,
  index: number | null,
  deps: unknown[] = [],
): void {
  // Keyboard focus is in the list. A big jump (End) scrolls the focused
  // element away and the browser drops focus to the page; this remembers
  // that it was here, so it comes back once the new item is drawn.
  const within = useRef(false);
  const current = useRef(index);
  current.current = index;
  useEffect(() => {
    const c = container.current;
    if (!c) return;
    // Focus coming back into the list (after a dialog closes, say) goes to
    // the current item, not to the one focused before.
    const enter = (e: FocusEvent) => {
      within.current = true;
      const t = e.target as HTMLElement | null;
      const i = current.current;
      if (i !== null && t?.dataset.index !== undefined && t.dataset.index !== String(i)) {
        c.querySelector<HTMLElement>(`[data-index="${i}"]`)?.focus({ preventScroll: true });
      }
    };
    const leave = (e: FocusEvent) => {
      const to = e.relatedTarget as Node | null;
      if (to && !c.contains(to)) within.current = false;
    };
    c.addEventListener("focusin", enter);
    c.addEventListener("focusout", leave);
    return () => {
      c.removeEventListener("focusin", enter);
      c.removeEventListener("focusout", leave);
    };
  }, [container]);
  useEffect(() => {
    const c = container.current;
    if (!c || index === null) return;
    const lost = document.activeElement === document.body || document.activeElement === null;
    if (!c.contains(document.activeElement) && !(within.current && lost)) return;
    const el = c.querySelector<HTMLElement>(`[data-index="${index}"]`);
    if (el && el !== document.activeElement) el.focus({ preventScroll: true });
    else if (!el && lost) c.focus({ preventScroll: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [container, index, ...deps]);
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), video[controls]';

/**
 * A modal dialog: Tab and Shift+Tab cycle inside it, and focus goes back
 * to where it was when the dialog closes.
 */
export function useFocusTrap(ref: RefObject<HTMLElement>): void {
  const back = useRef<HTMLElement | null>(null);
  useEffect(() => {
    back.current = document.activeElement as HTMLElement | null;
    const el = ref.current;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab" || !el) return;
      const items = [...el.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
        (x) => x.offsetParent !== null || x === document.activeElement,
      );
      if (items.length === 0) {
        e.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      const inside = el.contains(document.activeElement);
      if (e.shiftKey && (document.activeElement === first || !inside)) {
        last.focus();
        e.preventDefault();
      } else if (!e.shiftKey && (document.activeElement === last || !inside)) {
        first.focus();
        e.preventDefault();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      const b = back.current;
      if (b && document.contains(b)) b.focus();
    };
  }, [ref]);
}

const KIND: Record<string, string> = { photo: "photo", raw: "RAW photo", video: "video" };

/** What a screen reader says for a grid tile. */
export function describeRow(r: Row | undefined, index: number): string {
  if (!r) return `Item ${index + 1}, loading`;
  const parts = [r.name ?? "Untitled", KIND[r.kind] ?? r.kind];
  if (r.captured) parts.push(fmt.dateTime(r.captured));
  if (r.kind === "video" && r.durationMs !== null) parts.push(fmt.duration(r.durationMs));
  if (r.rating > 0) parts.push(`${r.rating} star${r.rating > 1 ? "s" : ""}`);
  if (r.label) parts.push(`${r.label} label`);
  if (!r.online) parts.push("drive not connected");
  return parts.join(", ");
}
