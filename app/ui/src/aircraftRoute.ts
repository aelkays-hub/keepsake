// Add route (ADR-044): what the window says for each photo, and the
// keywords it offers. Pure functions (unit-tested); the flights come from
// the `aircraft_route_review` command.

import type { RouteChoiceDto } from "./bindings/RouteChoiceDto";
import type { RouteItemDto } from "./bindings/RouteItemDto";

/** OpenSky's flight history goes back about this far (checked 2026-11). */
export const HISTORY_FROM = "2023";

/** `CYYZ` → `YYZ`; "Not known" when OpenSky did not see it. */
export function placeLabel(code: string | null, country: string | null): string {
  if (!code) return "Not known";
  return country ? `${code} (${country})` : code;
}

/** `YYZ (Canada) → LHR (United Kingdom)`. */
export function routeLine(c: RouteChoiceDto): string {
  return `${placeLabel(c.from, c.fromCountry)} → ${placeLabel(c.to, c.toCountry)}`;
}

/** `Flight AC856 (callsign ACA856)`, or just the callsign. */
export function flightLine(c: RouteChoiceDto): string {
  if (!c.flight) return "Flight not known";
  return c.callsign && c.callsign !== c.flight
    ? `Flight ${c.flight} (callsign ${c.callsign})`
    : `Flight ${c.flight}`;
}

/** `14:02` UTC from an ISO time. */
function hm(iso: string): string {
  return iso.slice(11, 16);
}

/** Where the photo is in the flight: `Taken 15 min before landing (22:36 UTC)`. */
export function whenLine(c: RouteChoiceDto): string {
  const dep = c.at === "departure";
  const time = hm(dep ? c.departed : c.arrived);
  const end = dep ? "take-off" : "landing";
  if (c.gapMin === 0) {
    return dep
      ? `Taken in the air, soon after take-off (${time} UTC)`
      : `Taken in the air, before landing (${time} UTC)`;
  }
  return `Taken ${c.gapMin} min ${dep ? "before" : "after"} ${end} (${time} UTC)`;
}

/** What the window says for a photo without a flight to offer. */
export function stateLine(item: RouteItemDto): string | null {
  switch (item.state) {
    case "no-registration":
      return "No registration: read the aircraft first (Read aircraft), or add the keyword Aircraft|Registration|….";
    case "no-time":
      return "The photo has no capture time.";
    case "no-transponder":
      return `The transponder code of ${item.registration ?? "this aircraft"} is not known: turn on Look registrations up online in Settings → Read aircraft.`;
    case "waiting":
      return "Asking OpenSky…";
    case "failed":
      return item.error ?? "OpenSky could not be asked.";
    case "nothing":
      return `OpenSky knows no flights of this aircraft that day. Its history goes back to about ${HISTORY_FROM}, and only where its receivers are.`;
    case "none-near":
      return "No flight within three hours of this time. If the camera's clock was on summer time but the photo says UTC−05:00, tick Camera clock on Eastern time above; if it was off otherwise, correct it with Shift time. Then Ask again.";
    default:
      return null;
  }
}

/** The keyword's parts: `Aircraft|From|YYZ` → `From YYZ`. */
export function keywordLabel(k: string): string {
  const [, kind = "", ...name] = k.split("|");
  return `${kind} ${name.join("|")}`;
}

/** The keywords left ticked of a choice (`unticked` holds those cleared). */
export function tickedKeywords(c: RouteChoiceDto, unticked: Set<string> | undefined): string[] {
  return c.keywords.filter((k) => !unticked?.has(k));
}

/** The progress line. */
export function routeProgress(pending: number, total: number, stopped: boolean): string {
  if (total === 0) return "No photos.";
  if (stopped) return "Stopped. Flights already found stay.";
  if (pending > 0) return `Asking OpenSky about ${pending} of ${total} photos…`;
  return `${total} ${total === 1 ? "photo" : "photos"}.`;
}

/** `UTC−04:00` from minutes. */
export function offsetLabel(min: number): string {
  const a = Math.abs(min);
  const hh = String(Math.floor(a / 60)).padStart(2, "0");
  const mm = String(a % 60).padStart(2, "0");
  return `UTC${min < 0 ? "−" : "+"}${hh}:${mm}`;
}

/** When the photo was taken, as Add route reads it: the camera's time
 * with its offset, and UTC (OpenSky's times are UTC). */
export function takenLine(item: RouteItemDto): string | null {
  if (!item.taken) return null;
  const utc = item.taken.slice(0, 16).replace("T", " ");
  if (item.local == null || item.offsetMin == null) {
    return `Taken ${utc}, time zone not known (read as UTC)`;
  }
  const local = item.local.slice(0, 16).replace("T", " ");
  return `Taken ${local} (${offsetLabel(item.offsetMin)}) · ${utc.slice(11)} UTC`;
}
