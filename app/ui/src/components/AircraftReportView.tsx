// Report → Aircraft: the photos of aircraft and the airlines, types and
// registrations named on them (keywords under Aircraft, typed or accepted
// from Read aircraft). It opens on the drill-down (airline › type ›
// registration, each with its photos); the three tables are beside it. A
// row's name opens its photos in the library.

import { useCallback, useEffect, useState } from "react";

import { api, pickSaveCsv, type AircraftReport } from "../api";
import type { AircraftReportGroup } from "../bindings/AircraftReportGroup";
import type { AircraftReportYear } from "../bindings/AircraftReportYear";
import {
  AIRCRAFT_TABLES,
  aircraftCsvName,
  aircraftRows,
  aircraftRowTitle,
  aircraftValue,
  drillKeys,
  drillRows,
  seen,
  share,
  type AircraftTable,
  type AircraftView,
} from "../aircraftReport";
import { ageLabel, ageOn, builtLabel, today } from "../aircraftAge";
import * as fmt from "../format";
import { useUi } from "../state";
import { sortRows, type SortValue, type TableSort } from "../tableSort";
import { SortHeader } from "./SortHeader";

/** The table shown last, so that coming back from a row's photos shows it. */
let lastTable: AircraftView = "drill";
let lastOpen: ReadonlySet<string> = new Set();
let lastFind = "";
let lastSort: TableSort = null;
let lastYearSort: TableSort = null;

export function AircraftReportView() {
  const [r, setR] = useState<AircraftReport | null>(null);
  const { dispatch } = useUi();
  const [view, setViewState] = useState<AircraftView>(lastTable);
  // The table whose rows are shown (behind the drill-down: the airlines).
  const table: AircraftTable = view === "drill" ? "airlines" : view;
  const [find, setFindState] = useState(lastFind);
  const setTable = (t: AircraftView) => {
    lastTable = t;
    setViewState(t);
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
  const [sort, setSortState] = useState<TableSort>(lastSort);
  const setSort = (t: TableSort) => {
    lastSort = t;
    setSortState(t);
  };
  const [yearSort, setYearSortState] = useState<TableSort>(lastYearSort);
  const setYearSort = (t: TableSort) => {
    lastYearSort = t;
    setYearSortState(t);
  };
  // The age is worked out from today, when the report opens (ADR-044).
  const [now] = useState(today);
  const rows = sortRows(r ? aircraftRows(r, table, find) : [], sort, (row, col) =>
    aircraftValue(row, col, now),
  );
  // Undated stays last.
  const years = sortRows(r?.years ?? [], yearSort, yearValue, (y) => y.year === null);
  const most = r ? Math.max(1, ...r[table].map((x) => x.photos)) : 1;
  const yearMax = r ? Math.max(1, ...r.years.map((y) => y.photos)) : 1;
  const open = (path: string, title: string) =>
    dispatch({ t: "open", section: { kind: "report", title, query: { tag: path } } });
  const openAll = (tags: string[], title: string) =>
    dispatch({ t: "open", section: { kind: "report", title, query: { tags } } });

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
                  <SortHeader
                    label="Year"
                    col="year"
                    sort={yearSort}
                    onSort={setYearSort}
                    testId="aircraft-years-sort-year"
                  />
                  <th scope="col">
                    <span className="aircraft-sr">Bar</span>
                  </th>
                  <SortHeader
                    label="Photos"
                    col="photos"
                    numeric
                    sort={yearSort}
                    onSort={setYearSort}
                    testId="aircraft-years-sort-photos"
                  />
                  <SortHeader
                    label="Named"
                    col="named"
                    numeric
                    sort={yearSort}
                    onSort={setYearSort}
                    testId="aircraft-years-sort-named"
                  />
                </tr>
              </thead>
              <tbody>
                {years.map((y) => (
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
                <button
                  aria-pressed={view === "drill"}
                  onClick={() => setTable("drill")}
                  title="Each airline, its types, and their registrations, with the photos of each"
                  data-testid="aircraft-by-drill"
                >
                  Airline › Type › Registration
                </button>
                {AIRCRAFT_TABLES.map((x) => (
                  <button
                    key={x.key}
                    aria-pressed={view === x.key}
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
            {view === "drill" && r && (
              <DrillDown groups={r.byAirline} find={find} onOpen={openAll} />
            )}
            <div className="aircraft-table-wrap" hidden={view === "drill"}>
              <table className="history report" data-testid="aircraft-table">
                <thead>
                  <tr>
                    {(
                      [
                        [meta.column, "name", false, true],
                        ["Photos", "photos", true, true],
                        ["Registrations", "registrations", true, table !== "registrations"],
                        ["Airline", "airline", false, table !== "airlines"],
                        ["Type", "type", false, table !== "types"],
                        ["Seen", "seen", false, true],
                        ["Built", "built", true, table === "registrations"],
                        ["Age", "age", true, table === "registrations"],
                      ] as const
                    )
                      .filter(([, , , shown]) => shown)
                      .map(([label, col, numeric]) => (
                        <SortHeader
                          key={col}
                          label={label}
                          col={col}
                          numeric={numeric}
                          sort={sort}
                          onSort={setSort}
                          testId={`aircraft-sort-${col}`}
                        />
                      ))}
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
                      {table === "registrations" && (
                        <>
                          <td className="num" data-testid="aircraft-built">
                            {row.built ? builtLabel(row.built) : "—"}
                          </td>
                          <td className="num" data-testid="aircraft-age">
                            {ageCell(row.built, now)}
                          </td>
                        </>
                      )}
                    </tr>
                  ))}
                  {r && rows.length === 0 && (
                    <tr>
                      <td colSpan={6} className="muted" data-testid="aircraft-none">
                        {find.trim() ? `Nothing matches “${find.trim()}”.` : "None yet."}
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
            {view !== "drill" && (
              <p className="muted small">
                Airline and type list up to three, most photos first. A photo can name several, so
                the rows add up to more than the totals.
                {table === "registrations" &&
                  " Age is counted to today from when the aircraft was built (“about” when only the year is known)."}
              </p>
            )}
          </div>
        </>
      )}
    </>
  );
}

/** The drill-down: the airlines; open one for its types, and a type for
 * its registrations. Each line has its photos; a name opens them. */
function DrillDown({
  groups,
  find,
  onOpen,
}: {
  groups: AircraftReportGroup[];
  find: string;
  onOpen: (tags: string[], title: string) => void;
}) {
  const [openKeys, setOpenState] = useState<ReadonlySet<string>>(lastOpen);
  const setOpen = (s: ReadonlySet<string>) => {
    lastOpen = s;
    setOpenState(s);
  };
  const rows = drillRows(groups, openKeys, find);
  const most = Math.max(1, ...groups.map((g) => g.photos));
  const toggle = (key: string) => {
    const n = new Set(openKeys);
    if (n.has(key)) n.delete(key);
    else n.add(key);
    setOpen(n);
  };
  return (
    <>
      <div className="actions aircraft-drill-actions">
        <button onClick={() => setOpen(new Set(drillKeys(groups)))} data-testid="aircraft-expand">
          Expand all
        </button>
        <button onClick={() => setOpen(new Set())} data-testid="aircraft-collapse">
          Collapse all
        </button>
      </div>
      <div className="aircraft-table-wrap">
        <table className="history report aircraft-drill" data-testid="aircraft-drill">
          <thead>
            <tr>
              <th scope="col">Airline › Type › Registration</th>
              <th scope="col" className="num">
                Photos
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.key} data-depth={row.depth} data-testid="aircraft-drill-row">
                <th scope="row" style={{ paddingLeft: `${8 + row.depth * 22}px` }}>
                  {row.children > 0 ? (
                    <button
                      className="link aircraft-twist"
                      aria-expanded={row.open}
                      aria-label={`${row.open ? "Hide" : "Show"} the ${row.under} of ${row.label}`}
                      onClick={() => toggle(row.key)}
                      data-testid="aircraft-drill-toggle"
                    >
                      {row.open ? "▾" : "▸"}
                    </button>
                  ) : (
                    <span className="aircraft-twist" aria-hidden={true} />
                  )}
                  {row.tags ? (
                    <button
                      className="link"
                      onClick={() => onOpen(row.tags ?? [], row.title)}
                      title={`Show these ${fmt.count(row.photos)} photos`}
                    >
                      {row.label}
                    </button>
                  ) : (
                    <span className={row.named ? undefined : "muted"}>{row.label}</span>
                  )}
                  {row.under && <span className="muted small"> · {row.under}</span>}
                </th>
                <td className="num">
                  {fmt.count(row.photos)}
                  {row.depth === 0 && (
                    <span
                      className="aircraft-bar all thin"
                      style={{ width: `${share(row.photos, most)}%` }}
                    />
                  )}
                </td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={2} className="muted" data-testid="aircraft-none">
                  {find.trim() ? `Nothing matches “${find.trim()}”.` : "None yet."}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <p className="muted small">
        Photos that name an airline, type or registration. A photo naming two airlines (or two
        registrations) is under each, so the lines can add up to more than the line above.
      </p>
    </>
  );
}

/** An aircraft row's value in a column, for sorting; Seen sorts by the
 * last day seen. */
function yearValue(y: AircraftReportYear, col: string): SortValue {
  if (col === "year") return y.year;
  if (col === "photos") return y.photos;
  if (col === "named") return y.withMarks;
  return null;
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

/** The age cell: worked out from today. */
function ageCell(built: string | null, now: string): string {
  const a = ageOn(built, now);
  return a ? ageLabel(a) : "—";
}
