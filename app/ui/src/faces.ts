// Recognise faces (ADR-040): what Settings says about it.

import type { FacesStatus } from "./api";
import * as fmt from "./format";

/** One line on how far finding faces has got. */
export function facesProgress(st: FacesStatus): string {
  if (!st.aiEnabled) return "Needs On-device AI (just above).";
  if (!st.enabled) return "Off.";
  if (st.problem) return `Not running: ${st.problem}`;
  if (!st.accel && st.pending > 0) return "Starting…";
  const found =
    st.faces === 0
      ? `${fmt.count(st.scanned)} of ${fmt.count(st.photos)} photos searched; no faces yet`
      : `${fmt.count(st.faces)} ${st.faces === 1 ? "face" : "faces"} in ` +
        `${fmt.count(st.withFaces)} of ${fmt.count(st.scanned)} photos searched ` +
        `(${fmt.count(st.photos)} photos in all)`;
  const where = st.accel ? ` On the ${st.accel}.` : "";
  return st.pending > 0
    ? `${found}; ${fmt.count(st.pending)} waiting.${where}`
    : `${found}.${where}`;
}
