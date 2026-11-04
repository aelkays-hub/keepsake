// Shown once after Keepsake closed unexpectedly: what happened, and the
// diagnostic report to copy. Dismissing marks the crash report as seen.

import { useEffect, useState } from "react";

import { api, type CrashNotice } from "../api";
import { copyDiagnosticReport } from "../diagnostics";

export function CrashBanner() {
  const [notice, setNotice] = useState<CrashNotice | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  useEffect(() => {
    api
      .crashNotice()
      .then(setNotice)
      .catch(() => undefined);
  }, []);
  if (!notice) return null;
  const dismiss = () => {
    setNotice(null);
    void api.crashDismiss().catch(() => undefined);
  };
  return (
    <div
      className="crash-banner"
      role="alert"
      aria-labelledby="crash-title"
      data-testid="crash-banner"
    >
      <div className="grow">
        <strong id="crash-title">Keepsake closed unexpectedly last time.</strong>{" "}
        <span className="muted">
          Your library is fine. A report was saved on this computer only ({notice.file}).
        </span>
        {status && (
          <div role="status" className="muted">
            {status}
          </div>
        )}
      </div>
      <button
        onClick={() =>
          copyDiagnosticReport()
            .then(() => setStatus("Copied to the clipboard. Paste it into your bug report."))
            .catch((e) => setStatus(`Could not copy: ${String(e)}`))
        }
      >
        Copy diagnostic report
      </button>
      <button onClick={dismiss}>Dismiss</button>
    </div>
  );
}
