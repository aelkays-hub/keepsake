// Report → Flights: every flight photographed, newest first: the date and
// time (the camera's), flight number, from and to (with countries),
// airline, registration, type and how many photos. From the photos' route
// keywords (Add route, Fill in route keywords, or typed) and aircraft
// keywords. A row opens its photos; Save as CSV writes the table.

import { useEffect, useMemo, useState } from "react";

import { api, pickSaveCsv } from "../api";
import type { FlightReportDto } from "../bindings/FlightReportDto";
import type { FlightReportRowDto } from "../bindings/FlightReportRowDto";
import { flightCsvName, flightPlace, flightMatches } from "../flightReport";
import * as fmt from "../format";
import { useUi } from "../state";
import { sortRows, type SortValue, type TableSort } from "../tableSort";
import { SortHeader } from "./SortHeader";

/** The column sorted last. */
let lastSort: TableSort = null;

const COLUMNS: { key: keyof FlightReportRowDto; label: string }[] = [
  { key: "date", label: "Date" },
  { key: "flight", label: "Flight" },
  { key: "from", label: "From" },
  { key: "to", label: "To" },
  { key: "airline", label: "Airline" },
  { key: "registration", label: "Registration" },
  { key: "aircraftType", label: "Type" },
  { key: "photos", label: "Photos" },
];

function value(r: FlightReportRowDto, col: string): SortValue {
  if (col === "date") return `${r.date ?? ""} ${r.time ?? ""}`;
  const v = r[col as keyof FlightReportRowDto];
  return typeof v === "number" ? v : typeof v === "string" ? v.toLowerCase() : null;
}

export function FlightReportView() {
  const { dispatch } = useUi();
  const [r, setR] = useState<FlightReportDto | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [find, setFind] = useState("");
  const [sort, setSortState] = useState<TableSort>(lastSort);
  const setSort = (t: TableSort) => {
    lastSort = t;
    setSortState(t);
  };

  useEffect(() => {
    let live = true;
    api
      .flightReport()
      .then((x) => live && setR(x))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, []);

  const rows = useMemo(() => {
    const shown = (r?.flights ?? []).filter((f) => flightMatches(f, find));
    return sort ? sortRows(shown, sort, value) : shown;
  }, [r, find, sort]);

  const save = () => {
    setSaved(null);
    setProblem(null);
    pickSaveCsv("Save the flight report as CSV", flightCsvName(new Date()))
      .then((path) => (path ? api.flightReportCsv(path).then(setSaved) : undefined))
      .catch((e) => setProblem(String(e)));
  };
  const open = (f: FlightReportRowDto) =>
    dispatch({
      t: "open",
      section: {
        kind: "report",
        title: [f.flight, f.registration, f.date].filter(Boolean).join(" · "),
        query: { tags: f.tags },
      },
    });

  return (
    <>
      <p className="muted">
        Every flight photographed: the photos of one day with the same registration, flight number,
        from and to. From the route keywords (Add route, Fill in route keywords, or typed) and the
        aircraft keywords. A date opens the flight&apos;s photos.
      </p>
      <div className="card">
        {r && r.withoutRoute > 0 && (
          <p className="muted" data-testid="flight-without-route">
            {fmt.plural(r.withoutRoute, "photo")} with a registration have no route yet: right-click
            them and choose Add route or Fill in route keywords.
          </p>
        )}
        <div className="actions">
          <button className="primary" onClick={save} disabled={!r} data-testid="flight-csv">
            Export to CSV…
          </button>
        </div>
        {saved && (
          <p className="ok" role="status" data-testid="flight-saved">
            Saved to {saved}
          </p>
        )}
        {problem && (
          <p className="error" role="alert" data-testid="flight-problem">
            {problem}
          </p>
        )}
      </div>
      <div className="card">
        <div className="aircraft-tools">
          <strong className="grow" data-testid="flight-count">
            {r ? fmt.plural(rows.length, "flight") : "Counting…"}
          </strong>
          <input
            type="search"
            placeholder="Find…"
            aria-label="Find a flight, airport, airline or registration"
            value={find}
            onChange={(e) => setFind(e.target.value)}
            data-testid="flight-find"
          />
        </div>
        <div className="aircraft-table-wrap">
          <table className="history report" data-testid="flight-table">
            <thead>
              <tr>
                {COLUMNS.map((c) => (
                  <SortHeader
                    key={c.key}
                    col={c.key}
                    label={c.label}
                    sort={sort}
                    onSort={setSort}
                    numeric={c.key === "photos"}
                    testId={`flight-sort-${c.key}`}
                  />
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((f, i) => (
                <tr key={i} data-testid="flight-row">
                  <td>
                    <button className="link" onClick={() => open(f)} title="Open its photos">
                      {f.date ?? "Undated"}
                      {f.time ? ` ${f.time}` : ""}
                    </button>
                  </td>
                  <td>{f.flight ?? ""}</td>
                  <td>{flightPlace(f.from, f.fromCountry)}</td>
                  <td>{flightPlace(f.to, f.toCountry)}</td>
                  <td>{f.airline ?? ""}</td>
                  <td>{f.registration ?? ""}</td>
                  <td>{f.aircraftType ?? ""}</td>
                  <td className="num">{fmt.count(f.photos)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {r && r.flights.length === 0 && (
          <p className="muted" data-testid="flight-none">
            No flights yet: add routes with Add route or Fill in route keywords.
          </p>
        )}
      </div>
    </>
  );
}
