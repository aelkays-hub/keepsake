// The right-click menu of a folder in the sidebar's folder tree: show it, or
// scan it again (that folder and what is below it), find duplicates, tag
// its photos or read its aircraft photos on this computer. Also opened with the
// Menu key or Shift+F10 on the focused folder.

import { useState } from "react";

import { ContextMenu } from "./ContextMenu";

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
  /** Find duplicates, tag the photos or read the aircraft photos of the
   * folder and those in it. */
  onAi: (what: "duplicates" | "tags" | "aircraft") => void;
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
  const [problem, setProblem] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  const scan = () => {
    setProblem(null);
    setStarting(true);
    onScan()
      .then(onClose)
      .catch((e) => setProblem(String(e)))
      .finally(() => setStarting(false));
  };

  return (
    <ContextMenu x={x} y={y} label={`${label} folder`} testId="folder-menu" onClose={onClose}>
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
      <button
        role="menuitem"
        onClick={() => {
          onAi("aircraft");
          onClose();
        }}
        title="Read the registration, airline and type on the photos tagged Airplane, Helicopter or Airport in this folder and the folders in it, on this computer"
        data-testid="folder-menu-aircraft"
      >
        Read aircraft
      </button>
      {!online && <p className="muted">The drive is not connected.</p>}
      {problem && (
        <p className="error" role="alert" data-testid="folder-menu-problem">
          {problem}
        </p>
      )}
    </ContextMenu>
  );
}
