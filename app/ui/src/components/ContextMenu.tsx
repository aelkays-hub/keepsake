// A right-click menu at the pointer (or under the focused entry, for the
// Menu key and Shift+F10), kept inside the window. The first item is
// focused; arrows, Home and End move; Escape or Tab, a click elsewhere, or
// leaving or resizing the window close it.
//
// Its items are either a list of actions (`items`: an action may return a
// promise, and the menu stays open until it settles and shows why if it
// fails) or any menu items the owner draws (`children`, with `ready` when
// they load later). Used by the sidebar's date and folder menus and the
// library items' menu.

import { useEffect, useRef, useState, type ReactNode } from "react";

export interface MenuItem {
  label: string;
  title?: string;
  disabled?: boolean;
  testId?: string;
  run: () => void | Promise<unknown>;
}

export interface ContextMenuProps {
  /** Where to open it (viewport pixels). */
  x: number;
  y: number;
  /** What the menu is for, read out by screen readers. */
  label: string;
  onClose: () => void;
  testId?: string;
  /** The actions, drawn as menu items. */
  items?: MenuItem[];
  /** Or menu items drawn by the owner. */
  children?: ReactNode;
  /** Changes when `children` are ready (loaded later): focus moves to the
   * first one then, unless an item already has focus. */
  ready?: unknown;
}

/** Focus without scrolling: the menu floats over a list (the sidebar's
 * tree, the grid), and a plain focus() scrolls that list to "show" it. */
const focus = (el: HTMLElement | undefined) => el?.focus({ preventScroll: true });

const enabled = (root: HTMLElement | null) => [
  ...(root?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []),
];

export function ContextMenu({
  x,
  y,
  label,
  onClose,
  testId,
  items,
  children,
  ready,
}: ContextMenuProps) {
  const root = useRef<HTMLDivElement>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
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
    focus(enabled(el)[0] ?? el);
  }, [ready]);

  const onKey = (e: React.KeyboardEvent) => {
    // The list's keys (and the viewer's) do not apply while it is open.
    e.stopPropagation();
    const all = enabled(root.current);
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape" || e.key === "Tab") onClose();
    else if (e.key === "ArrowDown") focus(all[(at + 1) % all.length]);
    else if (e.key === "ArrowUp") focus(all[(at - 1 + all.length) % all.length]);
    else if (e.key === "Home") focus(all[0]);
    else if (e.key === "End") focus(all[all.length - 1]);
    else return;
    e.preventDefault();
  };

  const run = (item: MenuItem) => {
    setProblem(null);
    const r = item.run();
    if (!(r instanceof Promise)) {
      onClose();
      return;
    }
    setBusy(true);
    r.then(onClose)
      .catch((e) => setProblem(String(e)))
      .finally(() => setBusy(false));
  };

  const left = Math.min(x, window.innerWidth - 260);
  const rows = items?.length ?? 4;
  const top = Math.min(y, window.innerHeight - 40 - rows * 32);
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
      {items?.map((item) => (
        <button
          key={item.label}
          role="menuitem"
          disabled={item.disabled || busy}
          title={item.title}
          onClick={() => run(item)}
          data-testid={item.testId}
        >
          {item.label}
        </button>
      ))}
      {children}
      {busy && (
        <p className="busy" role="status" data-testid="menu-busy">
          Working…
        </p>
      )}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
