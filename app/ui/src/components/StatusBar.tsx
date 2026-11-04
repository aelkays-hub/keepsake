// Status bar: item count, selection, renders, offline drives, edits.

import { useEffect, useState } from "react";

import { api, events, type EditProgress, type RenderSummary } from "../api";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";

export function StatusBar() {
  const { s, view, dispatch } = useUi();
  const [r, setR] = useState<RenderSummary | null>(null);
  const [edit, setEdit] = useState<EditProgress | null>(null);
  const [card, setCard] = useState<string | null>(null);
  useEffect(() => {
    const poll = () =>
      api
        .renderStatus()
        .then(setR)
        .catch(() => undefined);
    poll();
    const t = setInterval(poll, 10000);
    return () => clearInterval(t);
  }, []);
  useEffect(() => {
    const a = events.edit((e) => {
      setEdit(e.finished ? null : e);
      if (e.finished) dispatch({ t: "edited" });
    });
    const b = events.card((c) =>
      setCard(c.removed ? null : `${c.label}: ${fmt.plural(Number(c.newItems), "new item")}`),
    );
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, [dispatch]);
  const n = sel.size(s.sel);
  return (
    <footer className="statusbar" aria-live="polite">
      <span data-testid="count">{view ? fmt.plural(Number(view.count), "item") : "…"}</span>
      {n > 0 && <span data-testid="selected">{fmt.count(n)} selected</span>}
      {edit && (
        <span>
          Saving {fmt.count(Number(edit.done))} / {fmt.count(Number(edit.total))}…
        </span>
      )}
      <span className="spacer" />
      {card && <span className="card">💾 {card}</span>}
      {r && (
        <span
          title={`Cache ${fmt.bytes(Number(r.cacheBytes))} of ${fmt.bytes(Number(r.cacheMax))}`}
        >
          Renders: {Number(r.queued) > 0 ? `${fmt.count(Number(r.queued))} queued` : "up to date"}
          {r.paused && Number(r.queued) > 0 ? ` · paused (${r.paused})` : ""}
          {Number(r.failed) > 0 ? ` · ${fmt.count(Number(r.failed))} failed` : ""}
        </span>
      )}
    </footer>
  );
}
