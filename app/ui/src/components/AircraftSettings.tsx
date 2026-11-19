// Settings → Read aircraft (ADR-041): turn it on (downloads two text
// models once) or off. Reading itself is started from the photos:
// right-click them and choose Read aircraft. Everything stays on this
// computer; originals are only read, never changed.

import { useCallback, useEffect, useState } from "react";

import { aircraftProgress } from "../aircraft";
import { api, events, type AircraftStatus } from "../api";
import type { RouteStatusDto } from "../bindings/RouteStatusDto";
import * as fmt from "../format";

export function AircraftSettings() {
  const [st, setSt] = useState<AircraftStatus | null>(null);
  const [download, setDownload] = useState<{ done: number; total: number } | null>(null);
  // Add route (ADR-044): its setting and the OpenSky API client.
  const [route, setRoute] = useState<RouteStatusDto | null>(null);
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [tested, setTested] = useState<string | null>(null);
  const [fr24Token, setFr24Token] = useState("");
  // Downloading OpenSky's aircraft database (build years, ADR-044).
  const [dbDownload, setDbDownload] = useState<{ done: number; total: number } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const refresh = useCallback(() => {
    api
      .aircraftRouteStatus()
      .then(setRoute)
      .catch(() => setRoute(null));
    api
      .aircraftStatus()
      .then((x) => {
        setSt(x);
        if (x.installing !== null)
          setDownload((d) => d ?? { done: x.installing ?? 0, total: x.modelsBytes });
        if (x.builtInstalling !== null)
          setDbDownload((d) => d ?? { done: x.builtInstalling ?? 0, total: x.builtBytes });
      })
      .catch((e) => setProblem(String(e)));
  }, []);
  useEffect(() => {
    refresh();
    const un = [
      events.aircraftStatus(refresh),
      events.aiStatus(refresh),
      events.aircraftInstall((p) => {
        if (p.finished) {
          setDownload(null);
          if (p.error && p.error !== "cancelled") setProblem(`The download failed: ${p.error}`);
          refresh();
        } else setDownload({ done: p.done, total: p.total });
      }),
      events.aircraftBuiltInstall((p) => {
        if (p.finished) {
          setDbDownload(null);
          if (p.error && p.error !== "cancelled") setProblem(`The download failed: ${p.error}`);
          refresh();
        } else setDbDownload({ done: p.done, total: p.total });
      }),
    ];
    return () => {
      un.forEach((p) => void p.then((f) => f()).catch(() => undefined));
    };
  }, [refresh]);
  const run = (p: Promise<unknown>) => {
    setProblem(null);
    p.then(refresh).catch((e) => setProblem(String(e)));
  };
  if (!st) return null;
  return (
    <div className="card" data-testid="aircraft-settings">
      <h2>Read aircraft</h2>
      <p className="muted">
        For the photos you pick (right-click them and choose Read aircraft), Keepsake reads the
        registration and the airline painted on the aircraft and works out its type. A window then
        shows what it found: you choose how sure each kind must be, untick mistakes and accept, and
        only what you accept becomes keywords under Aircraft. It runs on this computer: nothing is
        sent anywhere, and your originals are only read, never changed.
      </p>
      <label className="check">
        <input
          type="checkbox"
          checked={st.enabled || download !== null}
          disabled={download !== null}
          onChange={(e) => run(api.aircraftEnable(e.target.checked))}
          data-testid="aircraft-enabled"
        />
        Read aircraft
      </label>
      {!st.modelsInstalled && !st.enabled && download === null && (
        <p className="muted">
          Turning it on downloads two text models and an aircraft finder once (
          {fmt.bytes(st.modelsBytes)}).
        </p>
      )}
      {download && (
        <div className="row" role="status" data-testid="aircraft-download">
          <progress max={download.total} value={download.done} />
          <span>
            Downloading the models: {fmt.bytes(download.done)} of {fmt.bytes(download.total)}
          </span>
          <button onClick={() => run(api.aircraftInstallCancel())}>Cancel</button>
        </div>
      )}
      {st.enabled && !st.finderInstalled && download === null && (
        <div className="row" data-testid="aircraft-finder">
          <span className="muted">
            The aircraft finder is not downloaded: every text in a photo is read, signs and
            signatures too.
          </span>
          <button onClick={() => run(api.aircraftEnable(true))} data-testid="aircraft-finder-get">
            Download it ({fmt.bytes(st.finderBytes)})
          </button>
        </div>
      )}
      <p role="status" data-testid="aircraft-progress">
        {aircraftProgress(st)}
      </p>
      <label className="check">
        <input
          type="checkbox"
          checked={st.lookup}
          disabled={!st.enabled}
          onChange={(e) => run(api.aircraftSetLookup(e.target.checked))}
          data-testid="aircraft-lookup"
        />
        Look registrations up online (adsbdb.com)
      </label>
      <p className="muted">
        Sends only the registration read (for example C-FIUJ), never a photo, to the free
        adsbdb.com, and offers the type and airline it knows in the Read aircraft window. Each
        registration is asked about once. With OpenSky's aircraft database on this computer (below),
        the year the aircraft was built is offered too: it is looked up here, and nothing more is
        sent.
      </p>
      {st.lookup && (
        <div className="block" data-testid="aircraft-built-db">
          <strong>Build years (OpenSky)</strong>
          <p className="muted">
            The aircraft's age is worked out from the year it was built. OpenSky's aircraft database
            (a crowd-sourced list, April 2024, free for non-commercial use) is downloaded once from
            opensky-network.org, and its build years are kept on this computer. About one airliner
            in four has one; for the others, type it in the inspector (Aircraft built). Each
            aircraft's transponder code is kept too, so Add route can find aircraft adsbdb does not
            know.
          </p>
          {dbDownload ? (
            <div className="row" role="status" data-testid="aircraft-built-download">
              <progress max={dbDownload.total} value={dbDownload.done} />
              <span>
                Downloading: {fmt.bytes(dbDownload.done)} of {fmt.bytes(dbDownload.total)}
              </span>
              <button onClick={() => run(api.aircraftBuiltCancel())}>Cancel</button>
            </div>
          ) : st.builtInstalled && !st.builtCodes ? (
            <div className="row">
              <span className="muted" data-testid="aircraft-built-old">
                On this computer, without the transponder codes. Download it again to keep them.
              </span>
              <button
                onClick={() => run(api.aircraftBuiltInstall())}
                data-testid="aircraft-built-again"
              >
                Download again ({fmt.bytes(st.builtBytes)})
              </button>
            </div>
          ) : st.builtInstalled ? (
            <div className="row">
              <span className="muted" data-testid="aircraft-built-ready">
                On this computer.
              </span>
              <button
                onClick={() => run(api.aircraftBuiltRemove())}
                data-testid="aircraft-built-remove"
              >
                Delete it
              </button>
            </div>
          ) : (
            <button
              onClick={() => run(api.aircraftBuiltInstall())}
              data-testid="aircraft-built-get"
            >
              Download it ({fmt.bytes(st.builtBytes)})
            </button>
          )}
        </div>
      )}
      <label className="check">
        <input
          type="checkbox"
          checked={route?.enabled ?? false}
          disabled={!st.enabled}
          onChange={(e) => run(api.aircraftSetRoute(e.target.checked))}
          data-testid="aircraft-route"
        />
        Find the route online
      </label>
      <p className="muted">
        Sends the aircraft's transponder code and the day and time the photo was taken (within 12
        hours) to opensky-network.org to find the flight. This tells OpenSky when you photographed
        that aircraft. No photo is sent. Right-click photos and choose Add route. OpenSky's history
        goes back to about 2023; a free account allows about 1,000 photos a day.
      </p>
      {route?.enabled && (
        <label className="check">
          <input
            type="checkbox"
            checked={route.eastern}
            onChange={(e) => run(api.aircraftSetRouteEastern(e.target.checked))}
            data-testid="aircraft-route-eastern"
          />
          Camera clock on Eastern time, changed for daylight saving
        </label>
      )}
      {route?.enabled && (
        <p className="muted">
          For a camera that marks every photo UTC−05:00 though its clock is put forward in summer:
          from the second Sunday of March to the first Sunday of November, Add route reads its
          photos as UTC−04:00. Photos without a time zone are read as Eastern time too; other time
          zones are kept. The photos themselves are not changed.
        </p>
      )}
      {route?.enabled && (
        <fieldset className="block" data-testid="route-source">
          <legend>Flights from</legend>
          <label className="check">
            <input
              type="radio"
              name="route-source"
              checked={route.source !== "flightradar"}
              onChange={() => run(api.aircraftSetRouteSource("opensky"))}
              data-testid="route-source-opensky"
            />
            OpenSky (free; by transponder code, from about 2023)
          </label>
          <label className="check">
            <input
              type="radio"
              name="route-source"
              checked={route.source === "flightradar"}
              onChange={() => run(api.aircraftSetRouteSource("flightradar"))}
              data-testid="route-source-flightradar"
            />
            Flightradar24 (your API subscription; by registration; how far back depends on the plan)
          </label>
        </fieldset>
      )}
      {route?.enabled && route.source === "flightradar" && (
        <div className="block" data-testid="fr24-client">
          <strong>Flightradar24 API token</strong>
          <p className="muted">
            Sends the registration and the photo's UTC day to fr24api.flightradar24.com. No photo is
            sent. Each flight it returns costs 3 credits of your plan (a day with no flights costs
            none). Its terms allow keeping its answers 30 days: they are deleted then; the keywords
            you accept stay on your photos. Create the token in Key management on
            fr24api.flightradar24.com. It is kept in{" "}
            {navigator.userAgent.includes("Mac") ? "the Keychain" : "Windows Credential Manager"},
            never in a file.
          </p>
          {route.flightradar ? (
            <div className="row">
              <span className="muted" data-testid="fr24-saved">
                Saved.
              </span>
              <button
                onClick={() => {
                  setTested(null);
                  api
                    .flightradarTest()
                    .then(setTested)
                    .catch((e) => setProblem(String(e)));
                }}
                title="Asks Flightradar24 something with no answer: no credits are used"
                data-testid="fr24-test"
              >
                Test
              </button>
              <button onClick={() => run(api.flightradarSetToken(""))} data-testid="fr24-remove">
                Remove
              </button>
            </div>
          ) : (
            <div className="row">
              <input
                type="password"
                value={fr24Token}
                placeholder="API token"
                autoComplete="off"
                onChange={(e) => setFr24Token(e.target.value)}
                data-testid="fr24-token"
              />
              <button
                disabled={!fr24Token.trim()}
                onClick={() => {
                  run(api.flightradarSetToken(fr24Token));
                  setFr24Token("");
                }}
                data-testid="fr24-save"
              >
                Save
              </button>
            </div>
          )}
          {tested && (
            <p role="status" data-testid="fr24-tested">
              {tested}
            </p>
          )}
        </div>
      )}
      {route?.enabled && route.source !== "flightradar" && (
        <div className="block" data-testid="opensky-client">
          <strong>OpenSky API client</strong>
          <p className="muted">
            Make a free account at opensky-network.org and create an API client on your account
            page. The client ID and secret are kept in{" "}
            {navigator.userAgent.includes("Mac") ? "the Keychain" : "Windows Credential Manager"},
            never in a file.
          </p>
          {route.opensky ? (
            <div className="row">
              <span className="muted" data-testid="opensky-saved">
                Saved.
              </span>
              <button
                onClick={() => {
                  setTested(null);
                  api
                    .openskyTest()
                    .then(setTested)
                    .catch((e) => setProblem(String(e)));
                }}
                data-testid="opensky-test"
              >
                Test
              </button>
              <button
                onClick={() => run(api.openskySetCredentials("", ""))}
                data-testid="opensky-remove"
              >
                Remove
              </button>
            </div>
          ) : (
            <div className="row">
              <input
                value={clientId}
                placeholder="Client ID"
                autoComplete="off"
                onChange={(e) => setClientId(e.target.value)}
                data-testid="opensky-id"
              />
              <input
                type="password"
                value={secret}
                placeholder="Client secret"
                autoComplete="off"
                onChange={(e) => setSecret(e.target.value)}
                data-testid="opensky-secret"
              />
              <button
                disabled={!clientId.trim() || !secret.trim()}
                onClick={() => {
                  run(api.openskySetCredentials(clientId, secret));
                  setSecret("");
                }}
                data-testid="opensky-save"
              >
                Save
              </button>
            </div>
          )}
          {tested && (
            <p role="status" data-testid="opensky-tested">
              {tested}
            </p>
          )}
          {!route.lookup && (
            <p className="muted" data-testid="opensky-needs-lookup">
              Add route needs Look registrations up online too: the transponder code comes from it.
            </p>
          )}
        </div>
      )}
      <p className="muted" data-testid="aircraft-confidence">
        Offered from: registration {st.confidence.registrationPct}%, airline{" "}
        {st.confidence.airlinePct}%, type {st.confidence.typePct}% sure (set with the sliders in the
        Read aircraft window).
      </p>
      {st.modelsInstalled && !st.enabled && (
        <button onClick={() => run(api.aircraftRemoveModels())} data-testid="aircraft-remove">
          Delete the downloaded models
        </button>
      )}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
