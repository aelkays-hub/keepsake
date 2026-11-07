// The right-click menu of a folder in the sidebar's folder tree: show it, or
// scan it again (that folder and what is below it). Also opened with the
// Menu key or Shift+F10 on the focused folder.

import { useEffect, useRef, useState } from "react";

export interface FolderMenuProps {
  /** Where to open it (viewport pixels). */
  x: number;
  y: number;
  /** The folder's name, for the menu's label. */
  label: string;
  /** Its drive is connected: it can be scanned. */
  online: boolean;
  onShow: () => void;
  /** Start the scan; rejects with why it could not start. */
  onScan: () => Promise<void>;
  /** Find duplicates or tag the photos of the folder and those in it. */
  onAi: (what: "duplicates" | "tags") => void;
  onClose: () => void;
}

export function FolderMenu({
  x,
  y,
  label,
  online,
  onShow,
  onScan,
  onAi,
  onClose,
}: FolderMenuProps) {
  const root = useRef<HTMLDivElement>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  // The latest, so the set-up below runs once (the sidebar re-renders often).
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  // Focus the first item; a click elsewhere, or leaving or resizing the
  // window, closes it.
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
    // The sidebar's tree keys do not apply while the menu is open.
    e.stopPropagation();
    const items = [
      ...(root.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []),
    ];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape" || e.key === "Tab") onClose();
    else if (e.key === "ArrowDown") items[(at + 1) % items.length]?.focus();
    else if (e.key === "ArrowUp") items[(at - 1 + items.length) % items.length]?.focus();
    else if (e.key === "Home") items[0]?.focus();
    else if (e.key === "End") items[items.length - 1]?.focus();
    else return;
    e.preventDefault();
  };

  const scan = () => {
    setProblem(null);
    setStarting(true);
    onScan()
      .then(onClose)
      .catch((e) => setProblem(String(e)))
      .finally(() => setStarting(false));
  };

  // Kept inside the window.
  const left = Math.min(x, window.innerWidth - 240);
  const top = Math.min(y, window.innerHeight - 190);
  return (
    <div
      ref={root}
      className="context-menu"
      style={{ left: Math.max(4, left), top: Math.max(4, top) }}
      role="menu"
      aria-label={`${label} folder`}
      onKeyDown={onKey}
      onContextMenu={(e) => e.preventDefault()}
      data-testid="folder-menu"
    >
      <button
        role="menuitem"
        onClick={() => {
          onShow();
          onClose();
        }}
      >
        Show
      </button>
      <button
        role="menuitem"
        disabled={!online || starting}
        onClick={scan}
        title={
          online
            ? "Find new, changed and removed files in this folder and the folders in it"
            : "Connect the drive first"
        }
        data-testid="folder-menu-rescan"
      >
        Scan again
      </button>
      <button
        role="menuitem"
        onClick={() => {
          onAi("duplicates");
          onClose();
        }}
        title="Compare the RAW photos of this folder and the folders in it, on this computer"
        data-testid="folder-menu-dups"
      >
        Find duplicates
      </button>
      <button
        role="menuitem"
        onClick={() => {
          onAi("tags");
          onClose();
        }}
        title="Tag the photos of this folder and the folders in it by what they show, on this computer"
        data-testid="folder-menu-tags"
      >
        Tag photos
      </button>
      {!online && <p className="muted">The drive is not connected.</p>}
      {problem && (
        <p className="error" role="alert" data-testid="folder-menu-problem">
          {problem}
        </p>
      )}
    </div>
  );
}
