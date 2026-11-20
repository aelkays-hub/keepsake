// The aircraft's age (ADR-044), for a photo with an `Aircraft|Registration|…`
// keyword: in the inspector (with an "Aircraft built" field to type it once
// per registration) and the viewer's information panel. Worked out when the
// photo is shown, from when the aircraft was built: never stored.

import { useEffect, useState } from "react";

import { api, type Details } from "../api";
import type { Fr24AircraftDto } from "../bindings/Fr24AircraftDto";
import { builtSource } from "../aircraftAge";
import { ageLines, usePhotoBuilt } from "../photoBuilt";
import { useUi } from "../state";

/** The inspector's block: both ages, and the field to type the build year. */
export function AircraftAge({ d }: { d: Details }) {
  const { dispatch } = useUi();
  const b = usePhotoBuilt(d);
  const fr24 = useFr24(b?.registration ?? null);
  const [text, setText] = useState("");
  const [note, setNote] = useState<string | null>(null);
  useEffect(() => {
    setText(b?.source === "typed" ? (b.built ?? "") : "");
    setNote(null);
  }, [b?.registration, b?.built, b?.source]);
  if (!b) return null;
  const save = (built: string | null) => {
    setNote(null);
    api
      .aircraftSetBuilt(b.registration, built)
      .then(() => dispatch({ t: "edited" }))
      .catch((e) => setNote(String(e)));
  };
  const lines = ageLines(d, b);
  return (
    <div className="block" data-testid="aircraft-age">
      <dl className="meta">
        {lines.map(([k, v]) => (
          <div key={k} className="meta-row">
            <dt>{k}</dt>
            <dd data-testid={k === "Aircraft age" ? "aircraft-age-today" : "aircraft-age-photo"}>
              {v}
            </dd>
          </div>
        ))}
        {b.built && (
          <div className="meta-row">
            <dt>Built</dt>
            <dd className="muted" data-testid="aircraft-built-source">
              {b.registration}: {builtSource(b.source)}
            </dd>
          </div>
        )}
      </dl>
      {fr24 && <Fr24Rows a={fr24} />}
      {!b.built && (
        <p className="muted" data-testid="aircraft-age-unknown">
          When {b.registration} was built is not known. Type it below, or turn on the OpenSky
          database in Settings → Read aircraft.
        </p>
      )}
      <label className="row">
        Aircraft built
        <input
          value={text}
          placeholder="2014 or 2014-03-05"
          size={12}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") save(text.trim() || null);
            e.stopPropagation();
          }}
          data-testid="aircraft-built-input"
        />
        <button onClick={() => save(text.trim() || null)} data-testid="aircraft-built-save">
          Save
        </button>
        {b.source === "typed" && (
          <button onClick={() => save(null)} data-testid="aircraft-built-forget">
            Forget
          </button>
        )}
      </label>
      <p className="muted small">
        For every photo of {b.registration}. The age is worked out each time, so it goes up by
        itself.
      </p>
      {note && (
        <p className="error" role="alert">
          {note}
        </p>
      )}
    </div>
  );
}

/** What Flightradar24 showed of `registration`'s aircraft (Settings → Look
 * up aircraft on Flightradar24), while it may be kept. */
function useFr24(registration: string | null): Fr24AircraftDto | null {
  const { s } = useUi();
  const [a, setA] = useState<Fr24AircraftDto | null>(null);
  useEffect(() => {
    let live = true;
    setA(null);
    if (!registration) return;
    api
      .fr24Aircraft(registration)
      .then((r) => live && setA(r))
      .catch(console.error);
    return () => {
      live = false;
    };
  }, [registration, s.revision]);
  return a;
}

/** The Flightradar24 rows: type, airline, transponder code, when seen. */
function Fr24Rows({ a }: { a: Fr24AircraftDto }) {
  if (!a.found) {
    return (
      <p className="muted" data-testid="fr24-aircraft">
        Flightradar24: {a.registration} did not fly in the four weeks asked about (asked again after{" "}
        {a.keptUntil}).
      </p>
    );
  }
  const type =
    a.aircraftType && a.icaoType && a.aircraftType !== a.icaoType
      ? `${a.aircraftType} (${a.icaoType})`
      : (a.aircraftType ?? a.icaoType);
  const airline = a.airline ?? a.livery;
  const rows: [string, string | null][] = [
    ["Type", type],
    ["Airline", airline && a.livery && airline !== a.livery ? `${airline} (${a.livery})` : airline],
    ["Transponder", a.hex],
    ["Seen flying", a.seen],
  ];
  return (
    <dl className="meta" data-testid="fr24-aircraft">
      <div className="meta-row">
        <dt>Flightradar24</dt>
        <dd className="muted">kept until {a.keptUntil}</dd>
      </div>
      {rows
        .filter((r): r is [string, string] => Boolean(r[1]))
        .map(([k, v]) => (
          <div key={k} className="meta-row">
            <dt>{k}</dt>
            <dd data-testid={`fr24-${k.toLowerCase().replace(" ", "-")}`}>{v}</dd>
          </div>
        ))}
    </dl>
  );
}
