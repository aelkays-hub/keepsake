// Fin numbers (ADR-052): which registration each airline's fin (fleet
// number) stood for. Learned from photos accepted with a fin read, filled
// from photos already tagged, or imported from a CSV file
// (airline,fin,registration[,from][,to]). Nothing is shipped with Keepsake.

import { useCallback, useEffect, useState } from "react";

import { byAirline, finDates, finSource, importLine, learnLine } from "../aircraftFins";
import { api, pickFile, pickSaveCsv } from "../api";
import type { FinDto } from "../bindings/FinDto";
import * as fmt from "../format";

export function AircraftFins() {
  const [fins, setFins] = useState<FinDto[] | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const load = useCallback(() => {
    api
      .aircraftFins()
      .then(setFins)
      .catch((e) => setProblem(String(e)));
  }, []);
  useEffect(load, [load]);
  if (!fins) return null;
  const act = (ask: () => Promise<string | null | undefined>) => {
    setProblem(null);
    setNote(null);
    ask()
      .then((n) => {
        if (n) setNote(n);
        load();
      })
      .catch((e) => setProblem(String(e)));
  };
  const learn = () => act(() => api.aircraftFinsLearn().then(learnLine));
  const importCsv = () =>
    act(() =>
      pickFile("Import fin numbers (CSV: airline, fin, registration, from, to)").then((path) =>
        path ? api.aircraftFinsImport(path).then(importLine) : null,
      ),
    );
  const exportCsv = () =>
    act(() =>
      pickSaveCsv("Save the fin numbers as CSV", "keepsake-fins.csv").then((path) =>
        path ? api.aircraftFinsCsv(path).then((p) => `Saved to ${p}.`) : null,
      ),
    );
  const forget = (f: FinDto) =>
    act(() =>
      api
        .aircraftFinForget({
          airline: f.airline,
          fin: f.fin,
          registration: f.registration,
          source: f.source,
        })
        .then(() => null),
    );
  const groups = byAirline(fins);
  return (
    <div className="block" data-testid="aircraft-fins">
      <strong>Fin numbers</strong>
      <p className="muted">
        Many airlines paint a fleet number on the tail or by the nose (Air Canada&apos;s 147, a US
        airline&apos;s ship 3301), one per aircraft. Keepsake learns which registration each stood
        for when you accept a photo with both read, and can fill this in from photos already tagged.
        You can also import a CSV file: airline, fin, registration, and optionally from and to
        dates.
      </p>
      <div className="row">
        <span className="grow muted" data-testid="fins-count">
          {fins.length === 0
            ? "No fins yet."
            : `${fmt.plural(fins.length, "fin")} for ${fmt.plural(groups.length, "airline")}.`}
        </span>
        <button onClick={learn} data-testid="fins-learn">
          Learn from tagged photos
        </button>
        <button onClick={importCsv} data-testid="fins-import">
          Import CSV…
        </button>
        {fins.length > 0 && (
          <>
            <button onClick={exportCsv} data-testid="fins-export">
              Export to CSV…
            </button>
            <button onClick={() => setOpen(!open)} data-testid="fins-show">
              {open ? "Hide" : "Show"}
            </button>
          </>
        )}
      </div>
      {note && (
        <p className="muted" role="status" data-testid="fins-note">
          {note}
        </p>
      )}
      {open && fins.length > 0 && (
        <table className="report" data-testid="fins-table">
          <thead>
            <tr>
              <th>Airline</th>
              <th>Fin</th>
              <th>Registration</th>
              <th>When</th>
              <th>From</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {fins.map((f) => (
              <tr key={`${f.airline}|${f.fin}|${f.registration}|${f.source}`} data-testid="fin-row">
                <td>{f.airline}</td>
                <td>{f.fin}</td>
                <td>
                  {f.registration}
                  {f.conflict && (
                    <span
                      className="muted"
                      title="Another registration has this fin at the same time"
                    >
                      {" "}
                      (two at once)
                    </span>
                  )}
                </td>
                <td className="muted">{finDates(f)}</td>
                <td className="muted">{finSource(f)}</td>
                <td>
                  <button onClick={() => forget(f)} data-testid="fin-forget">
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
