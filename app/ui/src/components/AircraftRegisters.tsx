// National aircraft registers (ADR-045): Transport Canada's and the FAA's,
// downloaded and imported when asked (Settings → Read aircraft). They give
// Canadian and US aircraft their type, airline, build year and transponder
// code, and put misread registrations right. Update now downloads only when
// the file changed. No owner who is a person, and no address, is kept.

import { useCallback, useEffect, useState } from "react";

import { api, events } from "../api";
import type { RegisterStatusDto } from "../bindings/RegisterStatusDto";
import * as fmt from "../format";

export function AircraftRegisters() {
  const [regs, setRegs] = useState<RegisterStatusDto[] | null>(null);
  const [notes, setNotes] = useState<Map<string, string>>(new Map());
  const [problem, setProblem] = useState<string | null>(null);
  const load = useCallback(() => {
    api
      .aircraftRegisters()
      .then(setRegs)
      .catch((e) => setProblem(String(e)));
  }, []);
  useEffect(load, [load]);
  useEffect(() => {
    const off = events.aircraftRegister((p) => {
      if (p.finished) {
        setNotes((m) => new Map(m).set(p.id, p.error ?? p.message ?? ""));
        load();
        return;
      }
      setRegs(
        (rs) =>
          rs?.map((r) =>
            r.id === p.id ? { ...r, running: true, done: p.done, importing: p.importing } : r,
          ) ?? rs,
      );
    });
    return () => {
      void off.then((f) => f());
    };
  }, [load]);
  if (!regs) return null;
  // The note is cleared before asking: an answer can come at once.
  const act = (ask: () => Promise<unknown>, id: string) => {
    setProblem(null);
    setNotes((m) => {
      const n = new Map(m);
      n.delete(id);
      return n;
    });
    ask()
      .then(load)
      .catch((e) => setProblem(String(e)));
  };
  return (
    <div className="block" data-testid="aircraft-registers">
      <strong>National aircraft registers</strong>
      <p className="muted">
        Canadian (C-) and US (N) aircraft: their type, airline, build year and transponder code,
        from Transport Canada's and the FAA's own registers (open data, updated daily). A
        registration read one character off a registered aircraft is offered as that one. Only
        company owners are kept, never a person's name or address.
      </p>
      {regs.map((r) => (
        <div key={r.id} className="row" data-testid={`register-${r.id}`}>
          <span className="grow">
            <strong>{r.name}</strong>{" "}
            <span className="muted" data-testid="register-state">
              {r.running
                ? r.importing
                  ? "Importing…"
                  : `Downloading… ${fmt.bytes(r.done)} of about ${fmt.bytes(r.bytes)}`
                : r.downloaded
                  ? `${r.rows.toLocaleString("en-US")} aircraft, file of ${r.fileModified ?? "?"} (imported ${r.imported ?? "?"})`
                  : "Not downloaded."}
            </span>
            {notes.get(r.id) && (
              <span className="muted" role="status" data-testid="register-note">
                {" "}
                {notes.get(r.id)}
              </span>
            )}
          </span>
          {r.running ? (
            !r.importing && (
              <button
                onClick={() => act(() => api.aircraftRegisterCancel(r.id), r.id)}
                data-testid="register-cancel"
              >
                Stop
              </button>
            )
          ) : (
            <>
              <button
                onClick={() => act(() => api.aircraftRegisterUpdate(r.id), r.id)}
                data-testid="register-update"
              >
                {r.downloaded ? "Update now" : `Download (about ${fmt.bytes(r.bytes)})`}
              </button>
              {r.downloaded && (
                <button
                  onClick={() => act(() => api.aircraftRegisterRemove(r.id), r.id)}
                  data-testid="register-remove"
                >
                  Remove
                </button>
              )}
            </>
          )}
        </div>
      ))}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
