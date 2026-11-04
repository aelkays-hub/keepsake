// "Open in…": a menu of the applications set for the item's file type in
// Settings (the first is the default, also Ctrl+O), then the system default.

import { useEffect, useRef, useState } from "react";

import { api, type MemberDto, type OpenChoice } from "../api";

export function OpenInMenu({
  id,
  member,
  disabled,
  onOpened,
}: {
  id: string | null;
  member: MemberDto;
  disabled?: boolean;
  /** A line for the status area: "Opened with …", or the problem. */
  onOpened: (note: string) => void;
}) {
  const [choices, setChoices] = useState<OpenChoice[] | null>(null);
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLUListElement>(null);

  // The choices depend on the item's type: forget them when it changes.
  useEffect(() => {
    setChoices(null);
    setOpen(false);
  }, [id, member]);

  useEffect(() => {
    if (open && choices) menu.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [open, choices]);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!menu.current?.contains(t) && !button.current?.contains(t)) setOpen(false);
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);

  const toggle = () => {
    if (!id) return;
    if (open) {
      setOpen(false);
      return;
    }
    setOpen(true);
    if (!choices)
      api
        .openChoices(id, member)
        .then(setChoices)
        .catch((e) => {
          setOpen(false);
          onOpened(String(e));
        });
  };
  const pick = (c: OpenChoice) => {
    setOpen(false);
    button.current?.focus();
    if (!id) return;
    api
      .openExternal(id, member, c.app)
      .then((name) => onOpened(`Opened with ${name}`))
      .catch((e) => onOpened(String(e)));
  };
  const onMenuKey = (e: React.KeyboardEvent) => {
    const items = [...(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    const go = (n: number) => items[(n + items.length) % items.length]?.focus();
    if (e.key === "ArrowDown") go(i + 1);
    else if (e.key === "ArrowUp") go(i - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(items.length - 1);
    else if (e.key === "Escape") {
      setOpen(false);
      button.current?.focus();
    } else if (e.key === "Tab") {
      setOpen(false);
      return;
    } else return;
    // Keep these keys from the viewer (Escape would close it).
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <span className="open-in">
      <button
        ref={button}
        onClick={toggle}
        disabled={disabled || !id}
        aria-haspopup="menu"
        aria-expanded={open}
        title="Open in another application (Ctrl+O opens the default)"
        data-testid="open-in"
      >
        Open in… ▾
      </button>
      {open && (
        <ul
          ref={menu}
          className="menu"
          role="menu"
          aria-label="Open in"
          onKeyDown={onMenuKey}
          data-testid="open-in-menu"
        >
          {!choices && (
            <li role="none" className="muted">
              …
            </li>
          )}
          {choices?.map((c) => (
            <li role="none" key={c.app || "system"}>
              <button
                role="menuitem"
                tabIndex={-1}
                onClick={() => pick(c)}
                title={c.app || undefined}
              >
                <span>{c.name}</span>
                {c.default && <kbd>Ctrl+O</kbd>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}
