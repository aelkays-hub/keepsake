// When the UI itself fails: say so plainly, log it, and offer the
// diagnostic report and a reload. The library is not affected.

import { Component, useEffect, useRef, useState, type ReactNode } from "react";

import { copyDiagnosticReport, logError } from "../diagnostics";

export function ErrorScreen({ error }: { error: unknown }) {
  const [copied, setCopied] = useState<string | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  // A frame later: the click that caused the failure may still be landing.
  useEffect(() => {
    const t = setTimeout(() => heading.current?.focus(), 50);
    return () => clearTimeout(t);
  }, []);
  return (
    <main className="error-screen" role="alert" aria-labelledby="err-title">
      <h1 id="err-title" ref={heading} tabIndex={-1}>
        Something went wrong
      </h1>
      <p>
        Keepsake hit a problem showing this window. Your photos, videos and library are not
        affected. Reloading usually fixes it.
      </p>
      <p className="muted">{error instanceof Error ? error.message : String(error)}</p>
      <div className="row">
        <button className="primary" onClick={() => window.location.reload()}>
          Reload
        </button>
        <button
          onClick={() =>
            copyDiagnosticReport()
              .then(() => setCopied("Copied to the clipboard."))
              .catch((e) => setCopied(`Could not copy: ${String(e)}`))
          }
        >
          Copy diagnostic report
        </button>
      </div>
      {copied && <p role="status">{copied}</p>}
    </main>
  );
}

/** Catches render errors below it. */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: unknown }> {
  state: { error: unknown } = { error: null };

  static getDerivedStateFromError(error: unknown) {
    return { error };
  }

  componentDidCatch(error: unknown) {
    logError(error, "render");
  }

  render() {
    if (this.state.error !== null) return <ErrorScreen error={this.state.error} />;
    return this.props.children;
  }
}
