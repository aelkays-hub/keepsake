// Read aircraft (ADR-041): what Settings and the progress window say.

import type { AircraftStatus } from "./api";
import * as fmt from "./format";

/** One line on whether Read aircraft is ready, and how the type is found. */
export function aircraftProgress(st: AircraftStatus): string {
  if (st.installing !== null) return "Downloading the text models…";
  if (!st.enabled) return "Off.";
  if (!st.runtimeFound) return "Not running: ONNX Runtime is missing.";
  if (st.problem) return `Not running: ${st.problem}`;
  const ready = st.accel ? `Ready, on the ${st.accel}.` : "Ready.";
  return (
    ready +
    (st.looks
      ? " The type is read when it is painted on the aircraft, else guessed from its look when that is clear."
      : " The type is read when it is painted on the aircraft. To have it guessed from the look otherwise, turn on On-device AI.")
  );
}

/** How far reading the picked photos has got. */
export interface ReadStatus {
  items: number;
  analysed: number;
  pending: number;
  failed?: number;
  problem?: string | null;
}

/** The tags a folder or dates are read for. */
export const AIRCRAFT_TAGS = "Airplane, Helicopter or Airport";

/** The progress window's line for Read aircraft on a folder or dates
 * (`where`: "in this folder", "on these dates"). */
export function aircraftBatchLine(st: ReadStatus, stopped: boolean, where: string): string {
  if (st.items === 0)
    return `No photos tagged ${AIRCRAFT_TAGS} ${where}. Tag the photos first (Tag photos).`;
  if (!stopped && st.pending === 0)
    return `${fmt.count(st.analysed)} of ${fmt.count(st.items)} aircraft photos read.`;
  return aircraftJobLine(st, stopped);
}

/** The progress window's line for Read aircraft. */
export function aircraftJobLine(st: ReadStatus, stopped: boolean): string {
  const of = `${fmt.count(st.analysed)} of ${fmt.count(st.items)}`;
  if (st.items === 0) return "Nothing to read.";
  if (stopped && st.pending === 0) return `Stopped: ${of} read. Keywords already added stay.`;
  if (st.pending > 0) return `${of} read…`;
  return (
    `${of} photos read.` +
    (st.failed ? ` ${fmt.plural(st.failed, "photo")} could not be: ${st.problem ?? "?"}` : "")
  );
}
