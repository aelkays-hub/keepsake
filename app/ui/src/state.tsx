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
    const un = events.importDone(() => setRefreshes((n) => n + 1));
    return () => {
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
