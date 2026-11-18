// The aircraft's age (ADR-044), for a photo with an `Aircraft|Registration|…`
// keyword: in the inspector (with an "Aircraft built" field to type it once
// per registration) and the viewer's information panel. Worked out when the
// photo is shown, from when the aircraft was built: never stored.

import { useEffect, useState } from "react";

import { api, type Details } from "../api";
import { builtSource } from "../aircraftAge";
import { ageLines, usePhotoBuilt } from "../photoBuilt";
import { useUi } from "../state";

/** The inspector's block: both ages, and the field to type the build year. */
export function AircraftAge({ d }: { d: Details }) {
  const { dispatch } = useUi();
  const b = usePhotoBuilt(d);
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
