// Reading THIRD-PARTY-NOTICES.txt (scripts/make-notices.py, ADR-047) for
// Tools → About: the programs shipped beside Keepsake, the packages built
// into it with their licences, and the licence texts they refer to.

/** A crate or npm package built into Keepsake. */
export interface NoticePackage {
  name: string;
  version: string;
  licence: string;
  repo: string;
  kind: "rust" | "js";
  /** The licence texts it refers to, by their [hash]. */
  refs: string[];
  /** It carries no licence file; the text is its licence's standard one. */
  standard: boolean;
}

export interface Notices {
  programs: string;
  packages: NoticePackage[];
  texts: Map<string, string>;
}

const RULE = "\n\n" + "=".repeat(76) + "\n\n";
const SUB = "\n\n" + "-".repeat(76) + "\n\n";

function packages(part: string, kind: NoticePackage["kind"]): NoticePackage[] {
  const out: NoticePackage[] = [];
  const lines = part.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const m = /^(\S+) (\S+) {2}— {2}(.*)$/.exec(lines[i]);
    if (!m) continue;
    const [, name, version, rest] = m;
    const r = /^(.*?) {2}(https?:\/\/\S+)$/.exec(rest);
    const note = lines[i + 1]?.startsWith("    Licence text:") ? lines[i + 1] : "";
    out.push({
      name,
      version,
      licence: r ? r[1] : rest,
      repo: r ? r[2] : "",
      kind,
      refs: [...note.matchAll(/\[([0-9a-f]{12})\]/g)].map((x) => x[1]),
      standard: note.includes("standard text of its licence"),
    });
  }
  return out;
}

/** Read the notices file. Parts it does not find are left empty. */
export function parseNotices(text: string): Notices {
  const parts = text.replace(/\r\n/g, "\n").split(RULE);
  const body = (p: string | undefined) => (p ?? "").replace(/^[^\n]*\n\n/, "");
  const texts = new Map<string, string>();
  for (const t of (parts[4] ?? "").split(SUB)) {
    const m = /^\[([0-9a-f]{12})\] used by: [^\n]*\n\n([\s\S]*)$/.exec(t);
    if (m) texts.set(m[1], m[2].replace(/\n+$/, ""));
  }
  return {
    programs: body(parts[1]).trimEnd(),
    packages: [...packages(parts[2] ?? "", "rust"), ...packages(parts[3] ?? "", "js")],
    texts,
  };
}

/** The packages whose name or licence has `find` in it (any case). */
export function findPackages(list: NoticePackage[], find: string): NoticePackage[] {
  const f = find.trim().toLowerCase();
  if (!f) return list;
  return list.filter(
    (p) => p.name.toLowerCase().includes(f) || p.licence.toLowerCase().includes(f),
  );
}

/** A credit an online service or data source asks Keepsake to show. */
export interface Credit {
  what: string;
  line: string;
  url: string;
}

/** The URLs must match `about::LINKS` in the app, the only ones it opens. */
export const CREDITS: Credit[] = [
  {
    what: "Flickr (Upload to Flickr, Report → Flickr)",
    line: "This product uses the Flickr API but is not endorsed or certified by SmugMug, Inc.",
    url: "https://www.flickr.com/services/api/",
  },
  {
    what: "OpenSky Network (Add route, build years)",
    line: "Flight and aircraft data from The OpenSky Network, https://opensky-network.org.",
    url: "https://opensky-network.org",
  },
  {
    what: "Transport Canada's Canadian Civil Aircraft Register",
    line: "Contains information licensed under the Open Government Licence – Canada.",
    url: "https://open.canada.ca/en/open-government-licence-canada",
  },
  {
    what: "FAA Releasable Aircraft Database",
    line: "Aircraft registration data from the US Federal Aviation Administration (public domain).",
    url: "https://www.faa.gov/licenses_certificates/aircraft_certification/aircraft_registry/releasable_aircraft_download",
  },
  {
    what: "OurAirports (airport codes and countries)",
    line: "Airport data from OurAirports (public domain).",
    url: "https://ourairports.com/data/",
  },
  {
    what: "adsbdb (registration look-up)",
    line: "Aircraft data from adsbdb.com, from PlaneBase.",
    url: "https://www.adsbdb.com",
  },
  {
    what: "Flightradar24 (with your own API key)",
    line: "Flight data from Flightradar24.",
    url: "https://www.flightradar24.com",
  },
];
