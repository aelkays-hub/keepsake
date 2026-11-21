// Tools → About: the version, the credits the data and services ask for,
// and the licences of the software Keepsake includes (ADR-047). The same
// licences are in the install folder, licenses/THIRD-PARTY-NOTICES.txt.

import { useEffect, useMemo, useState } from "react";

import { api } from "../api";
import { CREDITS, findPackages, parseNotices, type Notices } from "../licences";

export function AboutScreen() {
  const [version, setVersion] = useState("");
  const [notices, setNotices] = useState<Notices | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [find, setFind] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  useEffect(() => {
    api
      .appInfo()
      .then((a) => setVersion(a.version))
      .catch(() => undefined);
    api
      .thirdPartyNotices()
      .then((t) => setNotices(parseNotices(t)))
      .catch((e) => setProblem(String(e)));
  }, []);
  const shown = useMemo(
    () => (notices ? findPackages(notices.packages, find) : []),
    [notices, find],
  );
  const visit = (url: string) => api.aboutOpen(url).catch((e) => setProblem(String(e)));
  return (
    <section className="screen" aria-label="About" data-testid="about-screen">
      <h1 className="heading">About Keepsake</h1>
      <p data-testid="about-version">
        Keepsake {version}. Imports camera cards safely and keeps your photos and videos in a
        calendar-first library, on your own computer.
      </p>

      <div className="card" data-testid="about-credits">
        <h2>Data and services</h2>
        <p className="muted">
          Some features use these sources. Each is used only when you turn it on.
        </p>
        <ul className="about-credits">
          {CREDITS.map((c) => (
            <li key={c.url}>
              <strong>{c.what}</strong>
              <br />
              {c.line}{" "}
              <button className="link" onClick={() => visit(c.url)} title={c.url}>
                Open its page
              </button>
            </li>
          ))}
        </ul>
      </div>

      <div className="card" data-testid="licences">
        <h2>Licences</h2>
        <p className="muted">
          Keepsake includes software made by others, under the licences below. The same text is in
          Keepsake&apos;s install folder, in <code>licenses/THIRD-PARTY-NOTICES.txt</code>.
        </p>
        {problem && (
          <p className="error" role="alert">
            {problem}
          </p>
        )}
        {!notices && !problem && <p className="muted">Reading the licences…</p>}
        {notices && (
          <>
            <h3>Programs shipped beside Keepsake</h3>
            <pre className="licence-text" data-testid="licence-programs">
              {notices.programs}
            </pre>

            <h3>Built into Keepsake ({notices.packages.length} packages)</h3>
            <input
              type="search"
              className="licence-find"
              placeholder="Find a package or licence"
              aria-label="Find a package or licence"
              value={find}
              onChange={(e) => setFind(e.target.value)}
              data-testid="licence-find"
            />
            <table className="history report licence-table" data-testid="licence-table">
              <thead>
                <tr>
                  <th scope="col">Package</th>
                  <th scope="col">Version</th>
                  <th scope="col">Licence</th>
                  <th scope="col">Text</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((p) => {
                  const key = `${p.kind} ${p.name} ${p.version}`;
                  const isOpen = open === key;
                  return [
                    <tr key={key} data-testid="licence-row">
                      <th scope="row">
                        {p.name}
                        <span className="muted small">
                          {" "}
                          · {p.kind === "rust" ? "Rust" : "JavaScript"}
                        </span>
                      </th>
                      <td>{p.version}</td>
                      <td>{p.licence}</td>
                      <td>
                        {p.refs.length > 0 && (
                          <button
                            className="link"
                            aria-expanded={isOpen}
                            onClick={() => setOpen(isOpen ? null : key)}
                            data-testid="licence-show"
                          >
                            {isOpen ? "Hide licence" : "Show licence"}
                          </button>
                        )}
                      </td>
                    </tr>,
                    isOpen && (
                      <tr key={`${key} text`} data-testid="licence-text-row">
                        <td colSpan={4}>
                          {p.standard && (
                            <p className="muted small">
                              The package carries no licence file; this is the standard text of its
                              licence.
                            </p>
                          )}
                          {p.refs.map((r) => (
                            <pre className="licence-text" key={r} data-testid="licence-text">
                              {notices.texts.get(r) ?? ""}
                            </pre>
                          ))}
                          {p.repo && <p className="muted small">Source: {p.repo}</p>}
                        </td>
                      </tr>
                    ),
                  ];
                })}
                {shown.length === 0 && (
                  <tr>
                    <td colSpan={4} className="muted" data-testid="licence-none">
                      Nothing matches “{find.trim()}”.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </>
        )}
      </div>
    </section>
  );
}
