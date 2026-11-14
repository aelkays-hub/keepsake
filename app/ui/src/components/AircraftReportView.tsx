// Report → Aircraft: the photos of aircraft and the airlines, types and
// registrations named on them (keywords under Aircraft, typed or accepted
// from Read aircraft). A row's name opens its photos in the library.

import { useCallback, useEffect, useState } from "react";

import { api, pickSaveCsv, type AircraftReport } from "../api";
import {
  AIRCRAFT_TABLES,
  aircraftCsvName,
  aircraftRows,
  aircraftRowTitle,
  seen,
  share,
  type AircraftTable,
} from "../aircraftReport";
import * as fmt from "../format";
import { useUi } from "../state";

/** The table shown last, so that coming back from a row's photos shows it. */
let lastTable: AircraftTable = "airlines";
let lastFind = "";

export function AircraftReportView() {
  const [r, setR] = useState<AircraftReport | null>(null);
  const { dispatch } = useUi();
  const [table, setTableState] = useState<AircraftTable>(lastTable);
  const [find, setFindState] = useState(lastFind);
  const setTable = (t: AircraftTable) => {
    lastTable = t;
    setTableState(t);
  };
  const setFind = (f: string) => {
    lastFind = f;
    setFindState(f);
  };
  const [loading, setLoading] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const load = useCallback(() => {
    setLoading(true);
    setProblem(null);
    api
      .aircraftReport()
      .then(setR)
      .catch((e) => setProblem(String(e)))
      .finally(() => setLoading(false));
  }, []);
  useEffect(load, [load]);
  const exportCsv = () => {
    setProblem(null);
    setSaved(null);
    pickSaveCsv("Save the aircraft report as CSV", aircraftCsvName(new Date()))
      .then((path) => (path ? api.aircraftReportCsv(path).then(setSaved) : undefined))
      .catch((e) => setProblem(String(e)));
  };
  const t = r?.totals;
  const n = (v: number | undefined) => (v === undefined ? "…" : fmt.count(v));
  const meta = AIRCRAFT_TABLES.find((x) => x.key === table) ?? AIRCRAFT_TABLES[0];
  const rows = r ? aircraftRows(r, table, find) : [];
  const most = r ? Math.max(1, ...r[table].map((x) => x.photos)) : 1;
  const yearMax = r ? Math.max(1, ...r.years.map((y) => y.photos)) : 1;
  const open = (path: string, title: string) =>
    dispatch({ t: "open", section: { kind: "report", title, query: { tag: path } } });

  return (
    <>
      <p className="muted">
        Photos tagged Airplane or with a keyword under Aircraft. The airlines, types and
        registrations are those keywords: typed by you, or found by Read aircraft and accepted.
      </p>

      <div className="card" data-testid="aircraft-totals">
        <div className="report-totals">
          <div className="report-stat">
            <div className="muted">Aircraft photos</div>
            <strong data-testid="aircraft-total-photos">{n(t?.photos)}</strong>
            <div className="muted small" data-testid="aircraft-total-shots">
              {t ? `${fmt.count(t.shots)} distinct shots` : ""}
            </div>
          </div>
          <Stat
            label="Airlines"
            value={n(t?.airlines)}
            sub={t && `on ${fmt.count(t.withAirline)} photos`}
            id="airlines"
          />
          <Stat
            label="Types"
            value={n(t?.types)}
            sub={t && `on ${fmt.count(t.withType)} photos`}
            id="types"
          />
          <Stat
            label="Registrations"
            value={n(t?.registrations)}
            sub={t && `on ${fmt.count(t.withRegistration)} photos`}
            id="registrations"
          />
          <Stat
            label="Waiting for review"
            value={n(t?.waitingReview)}
            sub="Read aircraft results"
            id="waiting"
          />
        </div>
        {t && t.photos > 0 && (
          <p className="muted" data-testid="aircraft-named">
            {fmt.count(t.withMarks)} of {fmt.count(t.photos)} aircraft photos (
            {Math.round(share(t.withMarks, t.photos))}%) name an airline, type or registration.
            {t.waitingReview > 0 &&
              " Reads waiting for review count once you accept them (right-click → Read aircraft → Review)."}
          </p>
        )}
        <div className="actions">
          <button onClick={load} disabled={loading} data-testid="aircraft-refresh">
            {loading ? "Counting…" : "Refresh"}
          </button>
          <button
            className="primary"
            onClick={exportCsv}
            disabled={!r}
            data-testid="aircraft-export"
          >
            Export to CSV…
          </button>
        </div>
        {saved && (
          <p className="ok" role="status" data-testid="aircraft-saved">
            Saved to {saved}
          </p>
        )}
        {problem && (
          <p className="error" role="alert">
            {problem}
          </p>
        )}
      </div>

      {r && r.totals.photos === 0 ? (
        <div className="card">
          <p className="muted" data-testid="aircraft-empty">
            No aircraft photos yet. Tag photos (Tools → On-device AI) to find the airplanes, then
            right-click a folder or dates → Read aircraft.
          </p>
        </div>
      ) : (
        <>
          <div className="card">
            <h2>By year</h2>
            <div className="aircraft-legend muted">
              <span>
                <i className="aircraft-swatch all" /> All aircraft photos
              </span>
              <span>
                <i className="aircraft-swatch named" /> Naming an airline, type or registration
              </span>
            </div>
            <table className="history report aircraft-years" data-testid="aircraft-years">
              <thead>
                <tr>
                  <th scope="col">Year</th>
                  <th scope="col">
                    <span className="aircraft-sr">Bar</span>
                  </th>
                  <th scope="col" className="num">
                    Photos
                  </th>
                  <th scope="col" className="num">
                    Named
                  </th>
                </tr>
              </thead>
              <tbody>
                {(r?.years ?? []).map((y) => (
                  <tr key={y.year ?? "undated"}>
                    <th scope="row" className={y.year ? undefined : "muted"}>
                      {y.year ?? "Undated"}
                    </th>
                    <td className="aircraft-bar-cell">
                      <span
                        className="aircraft-bar all"
                        style={{ width: `${share(y.photos, yearMax)}%` }}
                      >
                        <span
                          className="aircraft-bar named"
                          style={{ width: `${share(y.withMarks, y.photos)}%` }}
                        />
                      </span>
                    </td>
                    <td className="num">{fmt.count(y.photos)}</td>
                    <td className="num muted" title="Naming an airline, type or registration">
                      {fmt.count(y.withMarks)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="card">
            <div className="aircraft-tools">
              <div className="segmented" role="group" aria-label="Show">
                {AIRCRAFT_TABLES.map((x) => (
                  <button
                    key={x.key}
                    aria-pressed={table === x.key}
                    onClick={() => setTable(x.key)}
                    data-testid={`aircraft-by-${x.key}`}
                  >
                    {x.label}
                    {r ? ` (${fmt.count(r[x.key].length)})` : ""}
                  </button>
                ))}
              </div>
              <input
                type="search"
                placeholder="Find…"
                aria-label="Find in the table"
                value={find}
                onChange={(e) => setFind(e.target.value)}
                data-testid="aircraft-find"
              />
            </div>
            <div className="aircraft-table-wrap">
              <table className="history report" data-testid="aircraft-table">
                <thead>
                  <tr>
                    <th scope="col">{meta.column}</th>
                    <th scope="col" className="num">
                      Photos
                    </th>
                    {table !== "registrations" && (
                      <th scope="col" className="num">
                        Registrations
                      </th>
                    )}
                    {table !== "airlines" && <th scope="col">Airline</th>}
                    {table !== "types" && <th scope="col">Type</th>}
                    <th scope="col">Seen</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.path}>
                      <th scope="row">
                        <button
                          className="link"
                          onClick={() => open(row.path, aircraftRowTitle(table, row))}
                          title={`Show these ${fmt.count(row.photos)} photos`}
                        >
                          {row.name}
                        </button>
                      </th>
                      <td className="num">
                        {fmt.count(row.photos)}
                        <span
                          className="aircraft-bar all thin"
                          style={{ width: `${share(row.photos, most)}%` }}
                        />
                      </td>
                      {table !== "registrations" && (
                        <td className="num">{fmt.count(row.registrations)}</td>
                      )}
                      {table !== "airlines" && <td>{row.airlines.join(", ") || "—"}</td>}
                      {table !== "types" && (
                        <td className="muted">{row.types.join(", ") || "—"}</td>
                      )}
                      <td className="muted aircraft-seen">{seen(row)}</td>
                    </tr>
                  ))}
                  {r && rows.length === 0 && (
                    <tr>
                      <td colSpan={5} className="muted" data-testid="aircraft-none">
                        {find.trim() ? `Nothing matches “${find.trim()}”.` : "None yet."}
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            <p className="muted small">
              Airline and type list up to three, most photos first. A photo can name several, so the
              rows add up to more than the totals.
            </p>
          </div>
        </>
      )}
    </>
  );
}

function Stat(p: { label: string; value: string; sub?: string; id: string }) {
  return (
    <div className="report-stat">
      <div className="muted">{p.label}</div>
      <strong data-testid={`aircraft-total-${p.id}`}>{p.value}</strong>
      {p.sub && <div className="muted small">{p.sub}</div>}
    </div>
  );
}
