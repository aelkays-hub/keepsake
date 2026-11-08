// The inspector: preview, metadata, and edits (rating, label, comment,
// keywords). Every edit applies to the whole selection and goes through core,
// which writes XMP. With several selected, the edit fields show what they
// have in common (a selection summary), not just the focused item.

import { useEffect, useState } from "react";

import { api, fileUrl, type Details, type Row, type SelectionSummary } from "../api";
import { keywordChips, selectionPayload, useEdits } from "../edits";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";
import { AiTags } from "./AiTags";
import { PeopleBlock } from "./PeopleBlock";
import { AllMetadata } from "./AllMetadata";
import { OpenInMenu } from "./OpenInMenu";
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
  const { s, view, dispatch } = useUi();
  const [opened, setOpened] = useState<string | null>(null);
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
  const multi = n > 1;
  const summary = useSummary(multi);
  // The comment box: what the user typed since it was last loaded.
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    if (multi) setComment(summary?.comment ?? "");
    setDirty(false);
  }, [multi, summary]);
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
        if (!multi) {
          setComment(x.comment ?? "");
          setDirty(false);
        }
      })
      .catch(console.error);
    return () => {
      live = false;
    };
    // `multi` only decides whether the comment box follows this item.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    if (!dirty) return;
    setDirty(false);
    edits.comment(comment.trim() ? comment : null);
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
      {multi && (
        <p className="multi" data-testid="multi">
          {fmt.plural(n, "item")} selected — every edit below applies to all of them.
        </p>
      )}
      <h2 className="name" title={d.name ?? ""}>
        {d.name ?? "—"}
      </h2>
      {!d.online && <p className="badge off">The drive with this item is not connected.</p>}
      <section aria-label="Rating and label" className="edit">
        <Stars value={multi ? (summary?.rating ?? 0) : d.rating} onChange={(v) => edits.rate(v)} />
        {multi && summary && summary.rating === null && (
          <span className="mixed" data-testid="rating-mixed">
            Mixed ratings
          </span>
        )}
        <LabelPicker
          value={multi ? (summary?.label ?? null) : d.label}
          onChange={(v) => edits.label(v)}
        />
        {multi && summary?.labelMixed && (
          <span className="mixed" data-testid="label-mixed">
            Mixed labels
          </span>
        )}
      </section>
      <div className="actions" aria-label="Actions">
        <button onClick={() => dispatch({ t: "viewer", open: true })} title="View (Enter)">
          View
        </button>
        <button
          onClick={() => dispatch({ t: "dialog", dialog: "timeShift" })}
          title="Correct the capture time (Ctrl+T)"
        >
          Shift time…
        </button>
        <OpenInMenu id={d.id} member="primary" disabled={!d.online} onOpened={setOpened} />
      </div>
      {opened && (
        <p className="muted" role="status" data-testid="opened">
          {opened}
        </p>
      )}
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
          placeholder={
            multi && summary?.commentMixed
              ? "Different comments. What you type replaces them all."
              : undefined
          }
          onChange={(e) => {
            setComment(e.target.value);
            setDirty(true);
          }}
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
          {keywordChips(multi, d.keywords, summary).map(({ k, of }) => (
            <li
              key={k}
              className={of ? "partial" : undefined}
              title={of ? `On ${of} of the ${fmt.count(n)} selected` : undefined}
            >
              {k}
              {of && <span className="muted"> ({of})</span>}
              {of && (
                <button aria-label={`Add ${k} to all`} onClick={() => edits.keywords([k], [])}>
                  +
                </button>
              )}
              <button
                aria-label={multi ? `Remove ${k} from all` : `Remove ${k}`}
                onClick={() => edits.keywords([], [k])}
              >
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
      {!multi && <AiTags id={d.id} />}
      {!multi && d.kind !== "video" && <PeopleBlock id={d.id} title={d.name ?? "this photo"} />}
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
      <AllMetadata
        id={d.id}
        online={d.online}
        hasRaw={d.files.some((f) => f.role === "raw_pair")}
      />
    </aside>
  );
}

/**
 * What the selection shares (rating, label, comment, keywords), fetched again
 * when the selection or the library changes. A short wait first, so a drag
 * across the grid asks once, not on every tile it passes.
 */
function useSummary(multi: boolean): SelectionSummary | null {
  const { s, view } = useUi();
  const [summary, setSummary] = useState<SelectionSummary | null>(null);
  const v = view?.view ?? null;
  useEffect(() => {
    if (!multi) {
      setSummary(null);
      return;
    }
    let live = true;
    const t = setTimeout(() => {
      api
        .selectionSummary(selectionPayload(v, s.sel))
        .then((x) => live && setSummary(x))
        .catch(console.error);
    }, 120);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [multi, v, s.sel, s.revision]);
  return summary;
}
