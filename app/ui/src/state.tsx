// UI state: where we are (section, view mode, calendar level), what is
// filtered and sorted, the open view and the selection.

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { api, events, type ViewInfo } from "./api";
import * as sel from "./selection";
import { effectiveQuery, initial, reduce, type Msg, type UiState } from "./uiModel";

export type { CalPos, Mode, Section, UiState } from "./uiModel";

/** Shortest time between re-opening the view while a folder is added. */
const FOLDER_REFRESH_MS = 3000;

interface Ctx {
  s: UiState;
  dispatch: (m: Msg) => void;
  view: ViewInfo | null;
  /** Re-open the view (after an import, or a sort-relevant edit). */
  refresh: () => void;
  refreshes: number;
}

const UiContext = createContext<Ctx | null>(null);

export function UiProvider({ children }: { children: ReactNode }) {
  const [s, dispatch] = useReducer(reduce, initial);
  const [view, setView] = useState<ViewInfo | null>(null);
  const [refreshes, setRefreshes] = useState(0);
  const query = useMemo(() => effectiveQuery(s), [s]);
  const key = JSON.stringify([query, s.sort, refreshes]);
  const latest = useRef(key);
  useEffect(() => {
    latest.current = key;
    const t = setTimeout(() => {
      api
        .viewOpen(query, s.sort)
        .then((v) => {
          if (latest.current === key) {
            setView(v);
            dispatch({ t: "sel", sel: sel.emptySel });
          }
        })
        .catch((e) => console.error("view_open", e));
      // The next turn of the event loop: changes made together (a filter
      // and the sort, say) still make one view.
    }, 0);
    return () => clearTimeout(t);
    // `key` covers query, sort and refreshes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  useEffect(() => {
    const bump = () => setRefreshes((n) => n + 1);
    const a = events.importDone(bump);
    const b = events.folderDone(bump);
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);
  // A folder being added fills in as it is read: the tree and counts follow
  // every saved batch; the view is opened again at most every few seconds,
  // and not while something is selected or in the viewer (that would lose
  // the selection), so it catches up when the scan finishes.
  const busy = useRef(false);
  busy.current = s.viewer || sel.size(s.sel) > 0;
  useEffect(() => {
    let last = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const reopen = () => {
      timer = null;
      if (busy.current) return;
      last = Date.now();
      setRefreshes((n) => n + 1);
    };
    const un = events.folderSaved(() => {
      dispatch({ t: "edited" });
      if (timer) return;
      timer = setTimeout(reopen, Math.max(0, last + FOLDER_REFRESH_MS - Date.now()));
    });
    return () => {
      if (timer) clearTimeout(timer);
      un.then((f) => f());
    };
  }, []);
  const ctx: Ctx = {
    s,
    dispatch,
    view,
    refresh: () => setRefreshes((n) => n + 1),
    refreshes,
  };
  return <UiContext.Provider value={ctx}>{children}</UiContext.Provider>;
}

// A context module: the provider plus its hook.
// eslint-disable-next-line react-refresh/only-export-components
export function useUi(): Ctx {
  const c = useContext(UiContext);
  if (!c) throw new Error("useUi outside UiProvider");
  return c;
}
