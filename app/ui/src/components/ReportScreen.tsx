// Report: how many photos and videos the library holds and how much disk
// they take, by year, month, day, camera, lens, file type and drive, from
// the catalog alone (no media file is read). Export writes every breakdown
// to one CSV file. A row's name opens its items in the library.

import { useCallback, useEffect, useState } from "react";

import { api, pickSaveCsv, type LibraryReport } from "../api";
import * as fmt from "../format";
import {
  BREAKDOWNS,
  csvName,
  lineQuery,
  lines,
  lineTitle,
  sum,
  totals,
  type Breakdown,
  type ReportLine,
} from "../report";
import { useUi } from "../state";

/** The breakdown shown last, so that coming back from a row's items shows
 * the same table. */
let lastBy: Breakdown = "year";

export function ReportScreen() {
  const [r, setR] = useState<LibraryReport | null>(null);
  const { dispatch } = useUi();
  const [by, setByState] = useState<Breakdown>(lastBy);
  const setBy = (b: Breakdown) => {
    lastBy = b;
    setByState(b);
  };
  const [loading, setLoading] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const load = useCallback(() => {
    setLoading(true);
    setProblem(null);
    api
      .libraryReport()
      .then(setR)
      .catch((e) => setProblem(String(e)))
      .finally(() => setLoading(false));
  }, []);
  useEffect(load, [load]);
  const exportCsv = () => {
    setProblem(null);
    setSaved(null);
    pickSaveCsv("Save the report as CSV", csvName(new Date()))
      .then((path) => (path ? api.libraryReportCsv(path).then(setSaved) : undefined))
      .catch((e) => setProblem(String(e)));
  };
  const all = r ? totals(r) : null;
  const rows = r ? lines(r, by) : [];
  const meta = BREAKDOWNS.find((b) => b.key === by) ?? BREAKDOWNS[0];
  return (
    <section className="screen" aria-label="Report" data-testid="report-screen">
      <h1 className="heading">Report</h1>
      <p className="muted">
        What the library holds and the disk space it takes. A RAW+JPEG pair counts as one photo;
        files and size include every file of an item (both halves of a pair, sidecars), counted once
        even when it is also on a backup drive. Items on unplugged drives are included.
      </p>

      <div className="card" data-testid="report-totals">
        <div className="report-totals">
          <Stat label="Photos" value={all ? fmt.count(all.photos ?? 0) : "…"} id="photos" />
          <Stat label="Videos" value={all ? fmt.count(all.videos ?? 0) : "…"} id="videos" />
          <Stat label="Files" value={all ? fmt.count(all.files) : "…"} id="files" />
          <Stat
            label="Size"
            value={all ? fmt.bytes(all.bytes) : "…"}
            title={all ? `${fmt.count(all.bytes)} bytes` : undefined}
            id="bytes"
          />
        </div>
        <div className="actions">
          <button onClick={load} disabled={loading} data-testid="report-refresh">
            {loading ? "Counting…" : "Refresh"}
          </button>
          <button className="primary" onClick={exportCsv} disabled={!r} data-testid="report-export">
            Export to CSV…
          </button>
        </div>
        {saved && (
          <p className="ok" role="status" data-testid="report-saved">
            Saved to {saved}
          </p>
        )}
        {problem && (
          <p className="error" role="alert">
            {problem}
          </p>
        )}
      </div>

      <div className="card">
        <div className="segmented" role="group" aria-label="Break down by">
          {BREAKDOWNS.map((b) => (
            <button
              key={b.key}
              aria-pressed={by === b.key}
              onClick={() => setBy(b.key)}
              data-testid={`report-by-${b.key}`}
            >
              {b.label}
            </button>
          ))}
        </div>
        {by === "drive" && (
          <p className="muted">
            Every copy on each drive, so an item backed up to two drives counts on both. Unplugged
            drives are included.
          </p>
        )}
        {by === "type" && (
          <p className="muted">A RAW+JPEG pair counts as an item under both of its file types.</p>
        )}
        {r && rows.length === 0 ? (
          <p className="muted">The library is empty.</p>
        ) : (
          <table className="history report" data-testid="report-table">
            <thead>
              <tr>
                <th scope="col">{meta.column}</th>
                <th scope="col" className="num">
                  Photos
                </th>
                <th scope="col" className="num">
                  Videos
                </th>
                <th scope="col" className="num">
                  Items
                </th>
                <th scope="col" className="num">
                  Files
                </th>
                <th scope="col" className="num">
                  Size
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((l) => (
                <Line
                  key={l.key}
                  l={l}
                  onOpen={() =>
                    dispatch({
                      t: "open",
                      section: { kind: "report", title: lineTitle(by, l), query: lineQuery(by, l) },
                    })
                  }
                />
              ))}
            </tbody>
            {all && rows.length > 0 && (
              <tfoot>
                {by === "drive" ? (
                  <Line l={{ ...all, ...sum(rows), label: "All drives" }} countsHidden />
                ) : (
                  <Line l={{ ...all, label: "Total" }} />
                )}
              </tfoot>
            )}
          </table>
        )}
      </div>
    </section>
  );
}

function Stat(p: { label: string; value: string; title?: string; id: string }) {
  return (
    <div className="report-stat" title={p.title}>
      <div className="muted">{p.label}</div>
      <strong data-testid={`report-total-${p.id}`}>{p.value}</strong>
    </div>
  );
}

function Line(p: { l: ReportLine; countsHidden?: boolean; onOpen?: () => void }) {
  const { l } = p;
  const n = (v: number | null) => (p.countsHidden || v === null ? "—" : fmt.count(v));
  return (
    <tr>
      <th scope="row" className={l.label === "Unknown" ? "muted" : undefined}>
        {p.onOpen ? (
          <button
            className="link"
            onClick={p.onOpen}
            title={`Show these ${fmt.count(l.items)} items`}
          >
            {l.label}
          </button>
        ) : (
          l.label
        )}
        {l.offline ? <span className="muted"> (unplugged)</span> : null}
      </th>
      <td className="num">{n(l.photos)}</td>
      <td className="num">{n(l.videos)}</td>
      <td className="num">{n(l.items)}</td>
      <td className="num">{fmt.count(l.files)}</td>
      <td className="num" title={`${fmt.count(l.bytes)} bytes`}>
        {fmt.bytes(l.bytes)}
      </td>
    </tr>
  );
}
