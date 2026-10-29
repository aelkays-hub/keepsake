// "All metadata" in the inspector: every tag ExifTool reads from the item's
// file (EXIF, GPS, the camera's maker notes, XMP, file facts), grouped,
// with a filter and Copy. Read only when the section is open; the file is
// never changed.

import { useEffect, useMemo, useState } from "react";

import { api, type FileMetadata, type MemberDto } from "../api";
import { groupName, tagName } from "../metaNames";

const OPEN_KEY = "keepsake.allMetadata.open";

function remembered(): boolean {
  try {
    return localStorage.getItem(OPEN_KEY) === "1";
  } catch {
    return false;
  }
}

function remember(open: boolean) {
  try {
    localStorage.setItem(OPEN_KEY, open ? "1" : "0");
  } catch {
    // Not kept: fine.
  }
}

export function AllMetadata({
  id,
  online,
  hasRaw,
}: {
  id: string;
  online: boolean;
  hasRaw: boolean;
}) {
  const [open, setOpen] = useState(remembered);
  const [member, setMember] = useState<MemberDto>("primary");
  const [data, setData] = useState<FileMetadata | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [copied, setCopied] = useState(false);
  useEffect(() => setMember("primary"), [id]);
  useEffect(() => {
    setData(null);
    setProblem(null);
    setCopied(false);
    if (!open || !online) return;
    let live = true;
    api
      .metadataAll(id, member)
      .then((m) => live && setData(m))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [id, member, open, online]);
  const shown = useMemo(() => {
    if (!data) return [];
    const f = filter.trim().toLowerCase();
    if (!f) return data.groups;
    return data.groups
      .map((g) => ({
        ...g,
        tags: g.tags.filter(
          (t) =>
            t.name.toLowerCase().includes(f) ||
            tagName(t.name).toLowerCase().includes(f) ||
            t.value.toLowerCase().includes(f),
        ),
      }))
      .filter((g) => g.tags.length > 0);
  }, [data, filter]);
  const total = data?.groups.reduce((n, g) => n + g.tags.length, 0) ?? 0;
  const copy = () => {
    if (!data) return;
    const text = data.groups
      .map((g) => [`[${g.group}]`, ...g.tags.map((t) => `${t.name}: ${t.value}`)].join("\n"))
      .join("\n\n");
    navigator.clipboard
      .writeText(`${data.file}\n\n${text}\n`)
      .then(() => setCopied(true))
      .catch(() => setProblem("could not copy to the clipboard"));
  };
  return (
    <details
      className="files all-meta"
      open={open}
      onToggle={(e) => {
        const o = (e.currentTarget as HTMLDetailsElement).open;
        setOpen(o);
        remember(o);
      }}
      data-testid="all-metadata"
    >
      <summary>All metadata{data ? ` (${total})` : ""}</summary>
      {!online ? (
        <p className="muted">Connect the drive with this item to read its metadata.</p>
      ) : (
        <>
          <div className="all-meta-tools">
            <input
              type="search"
              placeholder="Filter, e.g. ISO or lens"
              aria-label="Filter metadata"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
              data-testid="meta-filter"
            />
            {hasRaw && (
              <div className="segmented" role="group" aria-label="File">
                {(["primary", "raw"] as const).map((m) => (
                  <button key={m} aria-pressed={member === m} onClick={() => setMember(m)}>
                    {m === "primary" ? "JPEG" : "RAW"}
                  </button>
                ))}
              </div>
            )}
            <button onClick={copy} disabled={!data} data-testid="meta-copy">
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          {problem && <p className="error">{problem}</p>}
          {!data && !problem && <p className="muted">Reading…</p>}
          {data && (
            <>
              <p className="muted all-meta-file" title={data.file}>
                {data.file}
              </p>
              {shown.length === 0 && <p className="muted">No tags match.</p>}
              {shown.map((g) => (
                <section key={g.group} aria-label={groupName(g.group)}>
                  <h3>{groupName(g.group)}</h3>
                  <dl className="meta">
                    {g.tags.map((t, i) => (
                      <div key={`${t.name}${i}`} className="meta-row">
                        <dt title={t.name}>{tagName(t.name)}</dt>
                        <dd>{t.value || "—"}</dd>
                      </div>
                    ))}
                  </dl>
                </section>
              ))}
            </>
          )}
        </>
      )}
    </details>
  );
}
