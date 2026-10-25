// Edits of the current selection, shared by keyboard, grid and inspector.

import { useCallback } from "react";

import { api, type Selection } from "./api";
import { actionFor, moveIndex, type KeyLike } from "./keys";
import * as sel from "./selection";
import { useUi } from "./state";

/** The selection as the backend wants it (the focused item if none). */
export function selectionPayload(view: number | null, s: sel.Sel): Selection {
  const ranges = s.ranges.length
    ? s.ranges
    : s.focus !== null
      ? [[s.focus, s.focus + 1] as sel.Range]
      : [];
  return { ids: [], view, ranges };
}

export function useEdits() {
  const { s, dispatch, view } = useUi();
  const payload = selectionPayload(view?.view ?? null, s.sel);
  const empty = payload.ranges.length === 0;
  const done = useCallback(() => dispatch({ t: "edited" }), [dispatch]);
  return {
    empty,
    count: s.sel.ranges.length ? sel.size(s.sel) : s.sel.focus !== null ? 1 : 0,
    rate: (stars: number) => {
      if (!empty) api.editRating(payload, stars).then(done).catch(console.error);
    },
    label: (label: string | null) => {
      if (!empty) api.editLabel(payload, label).then(done).catch(console.error);
    },
    keywords: (add: string[], remove: string[]) => {
      if (!empty) api.editKeywords(payload, add, remove).then(done).catch(console.error);
    },
  };
}

/**
 * Keyboard handling for a list of `count` items laid out in `cols` columns
 * (1 for the details table). `open` is Enter on the focused item.
 */
export function useListKeys(
  count: number,
  cols: number,
  pageRows: number,
  open: (i: number) => void,
) {
  const { s, dispatch } = useUi();
  const edits = useEdits();
  return (e: React.KeyboardEvent | (KeyLike & { preventDefault: () => void })) => {
    const a = actionFor(e);
    if (!a) return;
    switch (a.kind) {
      case "move": {
        const to = moveIndex(s.sel.focus, a.by, count, cols, pageRows);
        if (to === null) return;
        dispatch({ t: "sel", sel: a.extend ? sel.extend(s.sel, to) : sel.only(to) });
        break;
      }
      case "toggleSelect":
        if (s.sel.focus !== null) dispatch({ t: "sel", sel: sel.toggle(s.sel, s.sel.focus) });
        break;
      case "selectAll":
        dispatch({ t: "sel", sel: sel.all(count) });
        break;
      case "clear":
        dispatch({ t: "sel", sel: sel.emptySel });
        break;
      case "open":
        if (s.sel.focus !== null) open(s.sel.focus);
        break;
      case "up":
        if (s.section.kind === "calendar" && s.cal.day !== null) {
          dispatch({ t: "cal", cal: { ...s.cal, day: null } });
        } else return;
        break;
      case "rate":
        edits.rate(a.stars);
        break;
      case "label":
        edits.label(a.label);
        break;
      case "view":
      case "search":
        // Handled window-wide (App.tsx).
        return;
    }
    e.preventDefault();
  };
}
