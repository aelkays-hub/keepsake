// The right-click menu of a library item (grid tile, details row, viewer):
// show its original in Explorer or Finder, selected, or open it in another
// application. Also opened with the Menu key or Shift+F10. With several
// items selected it acts on the one it was opened on. The original is only
// pointed at, never changed.

import { useEffect, useState } from "react";

import { api, type OpenChoice, type OriginalPlace } from "../api";
import { ContextMenu, type MenuItem } from "./ContextMenu";

export interface ItemMenuProps {
  x: number;
  y: number;
  /** The item's id and name. */
  id: string;
  name: string;
  onClose: () => void;
  /** A line for a status area ("Opened with …"), where there is one. */
  onNote?: (note: string) => void;
  /** More actions of the list it is opened in: `before` the item's own
   * (Open), `after` them below a line (Tag with Claude). */
  actions?: { before?: MenuItem[]; after?: MenuItem[] };
}

export function ItemMenu({ x, y, id, name, onClose, onNote, actions }: ItemMenuProps) {
  const [place, setPlace] = useState<OriginalPlace | null>(null);
  const [choices, setChoices] = useState<OpenChoice[]>([]);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .originalPlace(id)
      .then((p) => {
        if (!live) return;
        setPlace(p);
        // The Open in choices need the file too.
        if (p.online)
          api
            .openChoices(id, "primary")
            .then((c) => live && setChoices(c))
            .catch(() => undefined);
      })
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [id]);

  const reveal = () => {
    setProblem(null);
    api
      .showInFolder(id)
      .then(() => {
        onNote?.(`Shown in ${place?.manager ?? "the file manager"}`);
        onClose();
      })
      .catch((e) => setProblem(String(e)));
  };
  const openIn = (c: OpenChoice) => {
    setProblem(null);
    api
      .openExternal(id, "primary", c.app)
      .then((line) => {
        onNote?.(line);
        onClose();
      })
      .catch((e) => setProblem(String(e)));
  };

  const manager = place?.manager ?? "folder";
  const action = (a: MenuItem) => (
    <button
      key={a.label}
      role="menuitem"
      disabled={a.disabled}
      title={a.title}
      onClick={() => {
        onClose();
        void a.run();
      }}
      data-testid={a.testId}
    >
      {a.label}
    </button>
  );
  return (
    <ContextMenu
      x={x}
      y={y}
      label={name}
      testId="item-menu"
      onClose={onClose}
      ready={place !== null && choices.length}
    >
      {actions?.before?.map(action)}
      <button
        role="menuitem"
        disabled={!place?.online}
        onClick={reveal}
        title={place?.path ?? place?.problem ?? undefined}
        data-testid="item-menu-reveal"
      >
        Show in {manager}
      </button>
      {place && !place.online && (
        <p className="muted" data-testid="item-menu-offline">
          {place.problem}
        </p>
      )}
      {choices.length > 0 && <hr />}
      {choices.map((c) => (
        <button
          key={c.app || "system"}
          role="menuitem"
          onClick={() => openIn(c)}
          title={c.app || undefined}
          data-testid="item-menu-open"
        >
          {c.app ? `Open in ${c.name}` : "Open with the system default"}
        </button>
      ))}
      {actions?.after?.length ? (
        <>
          <hr />
          {actions.after.map(action)}
        </>
      ) : null}
      {problem && (
        <p className="error" role="alert" data-testid="item-menu-problem">
          {problem}
        </p>
      )}
    </ContextMenu>
  );
}
