// A small right-click menu: a list of actions at the pointer (or under the
// focused entry, for the Menu key and Shift+F10). Arrows, Home and End move;
// Escape or Tab, a click elsewhere, or leaving the window close it. An
// action may return a promise: the menu stays open until it settles and
// shows why if it fails.

import { useEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  title?: string;
  disabled?: boolean;
  testId?: string;
  run: () => void | Promise<unknown>;
}

export interface ContextMenuProps {
  x: number;
  y: number;
  /** What the menu is for, read out by screen readers. */
  label: string;
  items: MenuItem[];
  onClose: () => void;
  testId?: string;
}

export function ContextMenu({ x, y, label, items, onClose, testId }: ContextMenuProps) {
  const root = useRef<HTMLDivElement>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    root.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
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

  const onKey = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    const all = [
      ...(root.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []),
    ];
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape" || e.key === "Tab") onClose();
    else if (e.key === "ArrowDown") all[(at + 1) % all.length]?.focus();
    else if (e.key === "ArrowUp") all[(at - 1 + all.length) % all.length]?.focus();
    else if (e.key === "Home") all[0]?.focus();
    else if (e.key === "End") all[all.length - 1]?.focus();
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
  const top = Math.min(y, window.innerHeight - 40 - items.length * 32);
  return (
    <div
      ref={root}
      className="context-menu"
      style={{ left: Math.max(4, left), top: Math.max(4, top) }}
      role="menu"
      aria-label={label}
      onKeyDown={onKey}
      onContextMenu={(e) => e.preventDefault()}
      data-testid={testId}
    >
      {items.map((item) => (
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
