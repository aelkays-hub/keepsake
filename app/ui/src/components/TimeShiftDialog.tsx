// Batch time shift (Ctrl+T): the camera clock was wrong. Pick a reference
// photo from the selection, type the time it should show, check the offset
// and what it does to the selection, then apply. Only the catalog and the
// XMP sidecars change; the originals are never written.

import { useEffect, useMemo, useRef, useState } from "react";

import { api, events, fileUrl, type EditProgress, type Row, type TimeShiftPreview } from "../api";
import { selectionPayload } from "../edits";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";

/** Reference candidates listed (the first of the selection). */
const CANDIDATES = 200;

export function TimeShiftDialog() {
  const { s, view, dispatch, refresh } = useUi();
  const payload = useMemo(() => selectionPayload(view?.view ?? null, s.sel), [view?.view, s.sel]);
  const count = s.sel.ranges.length ? sel.size(s.sel) : s.sel.focus !== null ? 1 : 0;
  const [rows, setRows] = useState<Row[]>([]);
  const [reference, setReference] = useState<string | null>(null);
  const [correct, setCorrect] = useState("");
  const [preview, setPreview] = useState<TimeShiftPreview | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [progress, setProgress] = useState<EditProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const first = useRef<HTMLSelectElement>(null);
  const close = () => dispatch({ t: "dialog", dialog: null });

  // Candidates: dated items of the selection, the focused one first.
  useEffect(() => {
    if (!view) return;
    let live = true;
    (async () => {
      const out: Row[] = [];
      if (s.sel.focus !== null) out.push(...(await api.viewRows(view.view, s.sel.focus, 1)));
      for (const [a, b] of payload.ranges) {
        if (out.length >= CANDIDATES) break;
        out.push(...(await api.viewRows(view.view, a, Math.min(b - a, CANDIDATES))));
      }
      const seen = new Set<string>();
      const dated = out.filter((r) => r.captured && !seen.has(r.id) && seen.add(r.id));
      if (!live) return;
      setRows(dated.slice(0, CANDIDATES));
      if (dated[0]) {
        setReference(dated[0].id);
        setCorrect(dated[0].captured ?? "");
      }
    })().catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [view, payload, s.sel.focus]);

  useEffect(() => first.current?.focus(), [rows.length]);

  useEffect(() => {
    if (!reference || !correct) return;
    let live = true;
    const t = setTimeout(() => {
      api
        .timeShiftPreview(payload, reference, correct)
        .then((p) => {
          if (!live) return;
          setPreview(p);
          setProblem(null);
        })
        .catch((e) => {
          if (!live) return;
          setPreview(null);
          setProblem(String(e));
        });
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [payload, reference, correct]);

  const finish = () => {
    dispatch({ t: "edited" });
    refresh();
    close();
  };
  const apply = async () => {
    if (!preview || preview.offsetS === 0) return;
    setBusy(true);
    try {
      const r = await api.timeShiftApply(payload, preview.offsetS);
      if (!r.background) return finish();
      const un = await events.edit((p) => {
        setProgress(p);
        if (p.finished) {
          void un();
          if (p.errors.length) {
            setProblem(p.errors.join("\n"));
            setBusy(false);
          } else finish();
        }
      });
    } catch (e) {
      setProblem(String(e));
      setBusy(false);
    }
  };
  const ref = rows.find((r) => r.id === reference) ?? null;
  return (
    <div
      className="modal-back"
      onMouseDown={(e) => e.target === e.currentTarget && !busy && close()}
    >
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="ts-title"
        data-testid="time-shift"
        onKeyDown={(e) => {
          if (e.key === "Escape" && !busy) close();
          e.stopPropagation();
        }}
      >
        <h2 id="ts-title">Shift capture time</h2>
        <p className="muted">
          {fmt.plural(count, "item")} selected. Pick a photo whose correct time you know; the same
          offset is applied to all of them. Only the catalog and XMP sidecars change.
        </p>
        {rows.length === 0 ? (
          <p className="warn">{problem ?? "None of the selected items has a capture time."}</p>
        ) : (
          <>
            <div className="ts-ref">
              {ref?.thumb ? (
                <img src={fileUrl(ref.thumb) ?? undefined} alt="" />
              ) : (
                <div className="ph" />
              )}
              <div className="ts-fields">
                <label>
                  Reference photo
                  <select
                    ref={first}
                    value={reference ?? ""}
                    onChange={(e) => {
                      setReference(e.target.value);
                      const r = rows.find((x) => x.id === e.target.value);
                      if (r?.captured) setCorrect(r.captured);
                    }}
                    data-testid="ts-reference"
                  >
                    {rows.map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.name ?? r.id} — {fmt.dateTime(r.captured)}
                      </option>
                    ))}
                  </select>
                </label>
                <p className="muted">
                  The camera says{" "}
                  <strong data-testid="ts-before">
                    {preview?.referenceBefore ?? ref?.captured?.replace("T", " ")}
                  </strong>
                </p>
                <label>
                  It was really taken at
                  <input
                    type="datetime-local"
                    step={1}
                    value={correct}
                    onChange={(e) => setCorrect(e.target.value)}
                    data-testid="ts-correct"
                  />
                </label>
                <p>
                  Offset:{" "}
                  <strong data-testid="ts-offset">
                    {preview ? fmt.offset(preview.offsetS) : "—"}
                  </strong>
                </p>
              </div>
            </div>
            {preview && preview.total > 0 && (
              <table className="ts-table" data-testid="ts-preview">
                <thead>
                  <tr>
                    <th>Item</th>
                    <th>Now</th>
                    <th>After</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.samples.map((x) => (
                    <tr key={x.id}>
                      <td>{x.name ?? x.id}</td>
                      <td>{x.before}</td>
                      <td>{x.after}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {preview && preview.total > preview.samples.length && (
              <p className="muted">
                and {fmt.plural(preview.total - preview.samples.length, "more item")}.
              </p>
            )}
            {preview && preview.undated > 0 && (
              <p className="muted">
                {fmt.plural(preview.undated, "item")} without a capture time stay as they are.
              </p>
            )}
            {problem && <p className="error">{problem}</p>}
            {progress && !progress.finished && (
              <p role="status">
                Shifting {fmt.count(progress.done)} of {fmt.count(progress.total)}…
              </p>
            )}
          </>
        )}
        <footer className="modal-foot">
          <span className="spacer" />
          <button onClick={close} disabled={busy}>
            Cancel
          </button>
          <button
            className="primary"
            onClick={() => void apply()}
            disabled={busy || !preview || preview.offsetS === 0 || preview.total === 0}
            data-testid="ts-apply"
          >
            {preview ? `Shift ${fmt.plural(preview.total, "item")}` : "Shift"}
          </button>
        </footer>
      </div>
    </div>
  );
}
