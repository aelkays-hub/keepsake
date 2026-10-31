// Whether the viewer shows its information panel, remembered on this
// computer.

import { useSyncExternalStore } from "react";

const KEY = "keepsake.viewer.info";
const listeners = new Set<() => void>();

function read(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

let current = read();

/** Show or hide the panel, remembered on this computer. */
export function setViewerInfo(open: boolean) {
  current = open;
  try {
    localStorage.setItem(KEY, open ? "1" : "0");
  } catch {
    // Not remembered: still applies now.
  }
  listeners.forEach((l) => l());
}

export function useViewerInfo(): boolean {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}
