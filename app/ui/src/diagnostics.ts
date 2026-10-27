// Error reporting to the local log only (no telemetry), and copying the
// diagnostic report to the clipboard.

import { api } from "./api";

let ready = false;

/** The window shows its first content: tell the app once (start-up time),
 * after the browser has painted it. */
export function markReady(what: string): void {
  if (ready) return;
  ready = true;
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      api.uiReady(what).catch(() => undefined);
    }),
  );
}

/** Copy text to the clipboard (the async API, else a hidden text box). */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Fall back below (older webviews, or no permission).
  }
  const box = document.createElement("textarea");
  box.value = text;
  box.setAttribute("readonly", "");
  box.style.position = "fixed";
  box.style.opacity = "0";
  document.body.appendChild(box);
  box.select();
  const ok = document.execCommand("copy");
  box.remove();
  if (!ok) throw new Error("the clipboard is not available");
}

/** Make the report and copy it. Returns the report text. */
export async function copyDiagnosticReport(): Promise<string> {
  const text = await api.diagnosticReport();
  await copyText(text);
  return text;
}

function describe(e: unknown): { message: string; detail: string | null } {
  if (e instanceof Error) return { message: e.message || e.name, detail: e.stack ?? null };
  return { message: String(e), detail: null };
}

/** Send an error to the local log. Never throws. */
export function logError(e: unknown, where?: string): void {
  const { message, detail } = describe(e);
  const msg = where ? `${where}: ${message}` : message;
  api.logUiError(msg, detail).catch(() => undefined);
}

/** Uncaught errors and unhandled promise failures go to the log. */
export function installErrorLogging(): void {
  window.addEventListener("error", (ev) => logError(ev.error ?? ev.message, "uncaught"));
  window.addEventListener("unhandledrejection", (ev) => logError(ev.reason, "unhandled promise"));
}
