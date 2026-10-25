// The inspector: preview, metadata, and edits (rating, label, comment,
// keywords). Rating, label and keywords apply to the whole selection; the
// comment to the focused item. Every edit goes through core and writes XMP.

import { useEffect, useState } from "react";

import { api, fileUrl, type Details, type Row } from "../api";
import { useEdits } from "../edits";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";
import { LabelPicker, Stars } from "./Stars";

function Field({ name, value }: { name: string; value: React.ReactNode }) {
  return (
    <>
      <dt>{name}</dt>
      <dd>{value || "—"}</dd>
    </>
  );
}

export function Inspector() {
  const { s, view } = useUi();
  const edits = useEdits();
  const focus = s.sel.focus;
  const [row, setRow] = useState<Row | null>(null);
  useEffect(() => {
    let live = true;
    if (focus === null || !view) {
      setRow(null);
      return;
    }
    api
      .viewRows(view.view, focus, 1)
      .then((r) => live && setRow(r[0] ?? null))
      .catch(console.error);
    return () => {
      live = false;
    };
  }, [focus, view, s.revision]);
  const [d, setD] = useState<Details | null>(null);
  const [comment, setComment] = useState("");
  const [kw, setKw] = useState("");
  const [suggest, setSuggest] = useState<string[]>([]);
  const n = sel.size(s.sel);
  useEffect(() => {
    let live = true;
    if (!row) {
      setD(null);
      return;
    }
    api
      .details(row.id)
      .then((x) => {
        if (!live) return;
        setD(x);
        setComment(x.comment ?? "");
      })
      .catch(console.error);
    return () => {
      live = false;
    };
  }, [row, s.revision]);
  useEffect(() => {
    api
      .keywords()
      .then(setSuggest)
      .catch(() => undefined);
  }, [s.revision]);

  if (!d) {
    return (
      <aside className="inspector" aria-label="Inspector">
        <p className="muted">Select an item to see its details.</p>
      </aside>
    );
  }
  const preview = fileUrl(d.preview) ?? fileUrl(row?.thumb);
  const saveComment = () => {
    if ((d.comment ?? "") !== comment) {
      api
        .editComment(d.id, comment || null)
        .then(() => setD({ ...d, comment }))
        .catch(console.error);
    }
  };
  const addKeyword = () => {
    const k = kw.trim();
    if (!k) return;
    edits.keywords([k], []);
    setKw("");
  };
  return (
    <aside className="inspector" aria-label="Inspector" data-testid="inspector">
      {preview ? <img className="preview" src={preview} alt="" /> : <div className="preview ph" />}
      {n > 1 && (
        <p className="multi" data-testid="multi">
          {fmt.plural(n, "item")} selected — rating, label and keywords apply to all.
        </p>
      )}
      <h2 className="name" title={d.name ?? ""}>
        {d.name ?? "—"}
      </h2>
      {!d.online && <p className="badge off">The drive with this item is not connected.</p>}
      <section aria-label="Rating and label" className="edit">
        <Stars value={d.rating} onChange={(v) => edits.rate(v)} />
        <LabelPicker value={d.label} onChange={(v) => edits.label(v)} />
      </section>
      <dl className="meta">
        <Field name="Captured" value={fmt.dateTime(d.captured)} />
        <Field
          name="Time source"
          value={
            d.captureSource ? `${d.captureSource}${d.confidence ? ` (${d.confidence})` : ""}` : null
          }
        />
        <Field name="Camera" value={[d.cameraMake, d.cameraModel].filter(Boolean).join(" ")} />
        <Field name="Lens" value={d.lens} />
        <Field name="Type" value={{ photo: "Photo", video: "Video", raw: "RAW" }[d.kind]} />
        {d.kind === "video" && <Field name="Duration" value={fmt.duration(d.durationMs)} />}
        <Field name="Event" value={d.event} />
        {d.gps && (
          <Field name="Location" value={`${d.gps.lat.toFixed(5)}, ${d.gps.lon.toFixed(5)}`} />
        )}
      </dl>
      <label className="block">
        Comment
        <textarea
          value={comment}
          disabled={n > 1}
          onChange={(e) => setComment(e.target.value)}
          onBlur={saveComment}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) saveComment();
            e.stopPropagation();
          }}
          rows={3}
          data-testid="comment"
        />
      </label>
      <div className="block">
        Keywords
        <ul className="chips" aria-label="Keywords">
          {d.keywords.map((k) => (
            <li key={k}>
              {k}
              <button aria-label={`Remove ${k}`} onClick={() => edits.keywords([], [k])}>
                ✕
              </button>
            </li>
          ))}
        </ul>
        <input
          list="kw-suggest"
          value={kw}
          placeholder="Add keyword (Parent|Child)"
          onChange={(e) => setKw(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") addKeyword();
            e.stopPropagation();
          }}
          data-testid="keyword-input"
        />
        <datalist id="kw-suggest">
          {suggest.map((k) => (
            <option key={k} value={k} />
          ))}
        </datalist>
      </div>
      <details className="files">
        <summary>Files ({d.files.length})</summary>
        <ul>
          {d.files.map((f) => (
            <li key={f.path} className={f.online ? "" : "offline"}>
              <span className="role">{f.role}</span> {f.path}{" "}
              <span className="muted">on {f.volume}</span> {fmt.bytes(f.size)}
            </li>
          ))}
        </ul>
      </details>
    </aside>
  );
}
