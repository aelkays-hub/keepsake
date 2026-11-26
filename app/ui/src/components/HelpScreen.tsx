// Tools → Help: how-to topics with Find, the keyboard shortcuts, and where to
// get help (the version and the diagnostic report).

import { useEffect, useMemo, useState } from "react";

import { api } from "../api";
import { copyDiagnosticReport } from "../diagnostics";
import { HELP_TOPICS, SHORTCUTS, findTopics } from "../help";

export function HelpScreen() {
  const [find, setFind] = useState("");
  const [version, setVersion] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  useEffect(() => {
    api
      .appInfo()
      .then((a) => setVersion(a.version))
      .catch(() => undefined);
  }, []);
  const shown = useMemo(() => findTopics(HELP_TOPICS, find), [find]);
  const mac = navigator.userAgent.includes("Mac");
  return (
    <section className="screen" aria-label="Help" data-testid="help-screen">
      <h1 className="heading">Help</h1>

      <div className="card" data-testid="help-howto">
        <h2>How to</h2>
        <input
          type="search"
          className="licence-find"
          placeholder="Find a topic, e.g. import"
          aria-label="Find a help topic"
          value={find}
          onChange={(e) => setFind(e.target.value)}
          data-testid="help-find"
        />
        {shown.length === 0 && (
          <p className="muted" data-testid="help-none">
            No topic matches “{find}”.
          </p>
        )}
        {shown.map((t) => (
          <details key={t.id} className="help-topic" data-testid="help-topic" open={find !== ""}>
            <summary>{t.title}</summary>
            <ol>
              {t.steps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </details>
        ))}
      </div>

      <div className="card" data-testid="help-shortcuts">
        <h2>Keyboard shortcuts</h2>
        {mac && <p className="muted">On a Mac, use ⌘ where it says Ctrl.</p>}
        <table className="help-keys">
          <tbody>
            {SHORTCUTS.map((k) => (
              <tr key={k.keys}>
                <th scope="row">
                  <kbd>{k.keys}</kbd>
                </th>
                <td>{k.what}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="card" data-testid="help-get">
        <h2>Get help</h2>
        <p data-testid="help-version">Keepsake {version}</p>
        <p className="muted">
          If something goes wrong, copy the diagnostic report and paste it into your bug report. It
          lists versions, tools, library counts, settings and recent log lines, with your home
          folder shown as ~. Logs stay on this computer; Keepsake never sends anything.
        </p>
        <div className="row">
          <button
            className="primary"
            onClick={() =>
              copyDiagnosticReport()
                .then(() => setStatus("Copied to the clipboard."))
                .catch((e) => setStatus(`Could not copy: ${String(e)}`))
            }
            data-testid="help-copy-report"
          >
            Copy diagnostic report
          </button>
          {status && (
            <span className="ok" role="status">
              {status}
            </span>
          )}
        </div>
      </div>
    </section>
  );
}
