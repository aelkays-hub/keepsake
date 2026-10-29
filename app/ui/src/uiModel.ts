// The UI state model: pure types, the reducer and the effective query
// (unit-tested). The React provider lives in state.tsx.

import { emptyQuery, type Query, type SortSpec } from "./api";
import * as sel from "./selection";

export type Section =
  | { kind: "all" }
  | { kind: "calendar" }
  | { kind: "event"; id: string; title: string }
  | { kind: "volume"; id: string; title: string }
  /** A folder of a volume and everything below it. */
  | { kind: "folder"; id: string; path: string; title: string }
  | { kind: "import"; id: string; title: string };

export type Mode = "calendar" | "grid" | "details";

/** The library, or one of the full-window screens. */
export type Screen = "library" | "folders" | "backup" | "settings";

export type Dialog = "timeShift" | null;

/** Calendar position: no year = years level. A day opens a grid. */
export interface CalPos {
  year: number | null;
  month: number | null;
  day: number | null;
}

export interface UiState {
  section: Section;
  mode: Mode;
  filters: Query;
  sort: SortSpec;
  cal: CalPos;
  sel: sel.Sel;
  /** Bumped after edits so rows are fetched again. */
  revision: number;
  inspector: boolean;
  screen: Screen;
  /** The full-screen viewer shows the focused item. */
  viewer: boolean;
  dialog: Dialog;
}

export type Msg =
  | { t: "section"; section: Section }
  | { t: "mode"; mode: Mode }
  | { t: "filters"; filters: Query }
  | { t: "sort"; sort: SortSpec }
  | { t: "cal"; cal: CalPos }
  | { t: "sel"; sel: sel.Sel }
  | { t: "edited" }
  | { t: "inspector"; open: boolean }
  | { t: "screen"; screen: Screen }
  | { t: "viewer"; open: boolean }
  | { t: "dialog"; dialog: Dialog };

export const initial: UiState = {
  section: { kind: "all" },
  mode: "grid",
  filters: emptyQuery,
  sort: { field: "date", descending: true },
  cal: { year: null, month: null, day: null },
  sel: sel.emptySel,
  revision: 0,
  inspector: true,
  screen: "library",
  viewer: false,
  dialog: null,
};

export function reduce(s: UiState, m: Msg): UiState {
  switch (m.t) {
    case "section": {
      const cal = { year: null, month: null, day: null };
      const mode: Mode =
        m.section.kind === "calendar" ? "calendar" : s.mode === "calendar" ? "grid" : s.mode;
      return { ...s, section: m.section, cal, mode, sel: sel.emptySel, screen: "library" };
    }
    case "mode":
      return {
        ...s,
        mode: m.mode,
        screen: "library",
        section: m.mode === "calendar" ? { kind: "calendar" } : s.section,
      };
    case "filters":
      return { ...s, filters: m.filters, sel: sel.emptySel };
    case "sort":
      return { ...s, sort: m.sort, sel: sel.emptySel };
    case "cal":
      return {
        ...s,
        cal: m.cal,
        section: { kind: "calendar" },
        mode: m.cal.day !== null ? "grid" : "calendar",
        sel: sel.emptySel,
      };
    case "sel":
      return { ...s, sel: m.sel };
    case "edited":
      return { ...s, revision: s.revision + 1 };
    case "inspector":
      return { ...s, inspector: m.open };
    case "screen":
      return { ...s, screen: m.screen, viewer: false };
    case "viewer":
      return { ...s, viewer: m.open && s.sel.focus !== null };
    case "dialog":
      return { ...s, dialog: m.dialog };
  }
}

const two = (n: number) => String(n).padStart(2, "0");

/** The query for the grid/details: filters + section + calendar scope. */
export function effectiveQuery(s: UiState): Query {
  const q: Query = { ...s.filters };
  switch (s.section.kind) {
    case "event":
      q.event = s.section.id;
      break;
    case "volume":
      q.volume = s.section.id;
      break;
    case "folder":
      q.volume = s.section.id;
      q.folder = s.section.path;
      break;
    case "import":
      q.session = s.section.id;
      break;
    case "calendar": {
      const { year, month, day } = s.cal;
      if (year !== null && month !== null && day !== null) {
        const d = `${year}-${two(month)}-${two(day)}`;
        q.from = d;
        q.to = d;
      }
      break;
    }
  }
  return q;
}
