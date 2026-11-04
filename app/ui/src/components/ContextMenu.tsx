// A right-click menu: placed at the pointer (kept inside the window), the
// first item focused, arrow keys, Home and End to move, Escape or Tab to
// close. A click elsewhere, or leaving or resizing the window, closes it.
// Used by the sidebar's folder menu and the library items' menu.

import { useEffect, useRef, type ReactNode } from "react";

export interface ContextMenuProps {
  /** Where to open it (viewport pixels). */
  x: number;
  y: number;
  /** The menu's accessible name. */
  label: string;
  testId: string;
  onClose: () => void;
  /** Changes when the items are ready (loaded later): focus moves to the
   * first one then, unless an item already has focus. */
  ready?: unknown;
  children: ReactNode;
}

const enabled = (root: HTMLElement | null) => [
  ...(root?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []),
];

export function ContextMenu({ x, y, label, testId, onClose, ready, children }: ContextMenuProps) {
  const root = useRef<HTMLDivElement>(null);
  // The latest, so the set-up below runs once (the owner re-renders often).
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    const close = () => closeRef.current();
    const away = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) close();
    };
    document.addEventListener("mousedown", away);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("mousedown", away);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  }, []);

  // The first item, or the menu itself while there is none (keys still work).
  useEffect(() => {
    const el = root.current;
    const now = document.activeElement;
    if (!el || (now instanceof HTMLButtonElement && el.contains(now))) return;
    (enabled(el)[0] ?? el).focus();
  }, [ready]);

  const onKey = (e: React.KeyboardEvent) => {
    // The list's keys (and the viewer's) do not apply while it is open.
    e.stopPropagation();
    const items = enabled(root.current);
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape" || e.key === "Tab") onClose();
    else if (e.key === "ArrowDown") items[(at + 1) % items.length]?.focus();
    else if (e.key === "ArrowUp") items[(at - 1 + items.length) % items.length]?.focus();
    else if (e.key === "Home") items[0]?.focus();
    else if (e.key === "End") items[items.length - 1]?.focus();
    else return;
    e.preventDefault();
  };

  const left = Math.min(x, window.innerWidth - 260);
  const top = Math.min(y, window.innerHeight - 160);
  return (
    <div
      ref={root}
      className="context-menu"
      style={{ left: Math.max(4, left), top: Math.max(4, top) }}
      role="menu"
      aria-label={label}
      tabIndex={-1}
      onKeyDown={onKey}
      // Inside the grid or viewer: a press here is not a press on them.
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
      data-testid={testId}
    >
      {children}
    </div>
  );
}
