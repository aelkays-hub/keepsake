// Status bar: item count, selection, renders, offline drives, edits, and
// how far "Tag photos" has got.

import { useEffect, useState } from "react";

import { rangeProgress } from "../ai";
import { api, events, type AiRangeStatus, type EditProgress, type RenderSummary } from "../api";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";

/** Tag photos, from the date menu: shows "July 2026: Getting ready to
 * tag…" at once while the work is queued (slow on a busy drive), then
 * "Tagging: 34 of 120 done…", polled while it runs. When it is done the
 * library refreshes (tags appear in the inspector) and the line goes after
 * a few seconds; a problem stays until it is dismissed. */
function Tagging() {
  const { s, dispatch } = useUi();
  const range = s.tagging;
  const [st, setSt] = useState<AiRangeStatus | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    setSt(null);
    setProblem(null);
    if (!range) return;
    let live = true;
    let timer = 0;
    const poll = () =>
      api
        .aiRangeStatus(range.from, range.to, "tags")
        .then((x) => {
          if (!live) return;
          setSt(x);
          if (x.pending > 0) timer = window.setTimeout(poll, 3000);
          else {
            dispatch({ t: "edited" });
            timer = window.setTimeout(() => live && dispatch({ t: "tagging", range: null }), 8000);
          }
        })
        .catch((e) => live && setProblem(String(e)));
    api
      .aiAnalyzeRange(range.from, range.to, "tags")
      .then((x) => {
        if (!live) return;
        setSt(x);
        poll();
      })
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [range, dispatch]);
  if (!range) return null;
  if (problem)
    return (
      <span data-testid="tagging" role="alert" className="error">
        {range.title}: Could not tag: {problem}{" "}
        <button
          className="link"
          onClick={() => dispatch({ t: "tagging", range: null })}
          aria-label="Dismiss"
          data-testid="tagging-dismiss"
        >
          ✕
        </button>
      </span>
    );
  return (
    <span data-testid="tagging" role="status">
      <span className="spinner" aria-hidden={true} hidden={!!st && st.pending === 0} />
      {range.title}: {st ? rangeProgress(st, "tags") : "Getting ready to tag…"}
    </span>
  );
}

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
      <Tagging />
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
