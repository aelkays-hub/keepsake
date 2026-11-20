// When a photo's aircraft was built (ADR-044), for the inspector and the
// viewer's information panel, and the age lines worked out from it now.

import { useEffect, useState } from "react";

import type { Details } from "./api";
import { api } from "./api";
import { ageFacts, builtKeyword, registrationsOf, today } from "./aircraftAge";
import { useUi } from "./state";

/** When the photo's aircraft was built: as kept for its registration (from
 * OpenSky, or typed), else its `Aircraft|Built|…` keyword. */
export interface PhotoBuilt {
  registration: string;
  built: string | null;
  /** `opensky`, `typed`, or null (from the keyword, or not known). */
  source: string | null;
}

export function usePhotoBuilt(d: Details | null): PhotoBuilt | null {
  const { s } = useUi();
  const [kept, setKept] = useState<{ built: string; source: string } | null>(null);
  const reg = d ? (registrationsOf(d.keywords)[0] ?? null) : null;
  useEffect(() => {
    let live = true;
    setKept(null);
    if (!reg) return;
    api
      .aircraftBuilt([reg])
      .then((r) => live && setKept(r[0] ? { built: r[0].built, source: r[0].source } : null))
      .catch(console.error);
    return () => {
      live = false;
    };
  }, [reg, s.revision]);
  if (!d || !reg) return null;
  if (kept) return { registration: reg, ...kept };
  return { registration: reg, built: builtKeyword(d.keywords), source: null };
}

/** The age facts as `[name, value]` lines (none when not known). */
export function ageLines(d: Details, b: PhotoBuilt | null): [string, string][] {
  if (!b?.built) return [];
  const f = ageFacts(b.built, d.captured, today());
  const out: [string, string][] = [];
  if (f.today) out.push(["Aircraft age", f.today]);
  if (f.inPhoto) out.push(["Age in this photo", f.inPhoto]);
  return out;
}
