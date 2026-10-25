// The main window: toolbar, sidebar · main area · inspector, status bar.

import { useEffect, useState } from "react";

import { api, type AppInfo } from "./api";
import { CalendarView } from "./components/CalendarView";
import { DetailsView } from "./components/DetailsView";
import { FirstRun } from "./components/FirstRun";
import { GridView } from "./components/GridView";
import { Inspector } from "./components/Inspector";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { Toolbar } from "./components/Toolbar";
import * as fmt from "./format";
import { UiProvider, useUi } from "./state";

function Heading() {
  const { s, dispatch } = useUi();
  const { year, month, day } = s.cal;
  let title: React.ReactNode = "All items";
  if (s.section.kind === "calendar") {
    title =
      day !== null && year !== null && month !== null ? (
        <>
          <button
            onClick={() => dispatch({ t: "cal", cal: { year: null, month: null, day: null } })}
          >
            Calendar
          </button>
          {" › "}
          <button onClick={() => dispatch({ t: "cal", cal: { year, month: null, day: null } })}>
            {year}
          </button>
          {" › "}
          <button onClick={() => dispatch({ t: "cal", cal: { year, month, day: null } })}>
            {fmt.monthName(month)}
          </button>
          {` › ${day}`}
        </>
      ) : (
        "Calendar"
      );
  } else if ("title" in s.section) {
    title = s.section.title;
  }
  return (
    <h1 className="heading" data-testid="heading">
      {title}
    </h1>
  );
}

function Main() {
  const { s, dispatch } = useUi();
  // Enter on an item: show it in the inspector (and open the inspector).
  const open = () => dispatch({ t: "inspector", open: true });
  return (
    <main className="main" aria-label="Items">
      {s.mode !== "calendar" && <Heading />}
      {s.mode === "calendar" ? (
        <CalendarView />
      ) : s.mode === "grid" ? (
        <GridView onOpen={open} />
      ) : (
        <DetailsView onOpen={open} />
      )}
    </main>
  );
}

function Shell() {
  const { s, dispatch } = useUi();
  // F6 cycles sidebar → main → inspector; Ctrl+I toggles the inspector.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F6") {
        const regions = [
          ".sidebar",
          ".main [tabindex='0'], .main button",
          ".inspector button, .inspector input",
        ];
        const i = regions.findIndex((r) => document.activeElement?.closest(r.split(" ")[0]));
        const next = document.querySelector<HTMLElement>(regions[(i + 1) % regions.length]);
        next?.focus();
        e.preventDefault();
      } else if ((e.ctrlKey || e.metaKey) && !e.altKey) {
        // Window-wide shortcuts: work from the sidebar, calendar or grid.
        const k = e.key.toLowerCase();
        const modes = { "1": "calendar", "2": "grid", "3": "details" } as const;
        if (k === "i") dispatch({ t: "inspector", open: !s.inspector });
        else if (k === "f") document.getElementById("search")?.focus();
        else if (k in modes) dispatch({ t: "mode", mode: modes[k as keyof typeof modes] });
        else return;
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dispatch, s.inspector]);
  return (
    <div className={`shell${s.inspector ? "" : " no-inspector"}`}>
      <Toolbar />
      <Sidebar />
      <Main />
      {s.inspector && <Inspector />}
      <StatusBar />
    </div>
  );
}

export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api
      .appInfo()
      .then(setInfo)
      .catch((e) => setError(String(e)));
  }, []);
  if (error) return <p className="error">Keepsake could not start: {error}</p>;
  if (!info) return null;
  if (!info.library) return <FirstRun onReady={setInfo} uiTest={info.uiTest} />;
  return (
    <UiProvider key={info.library}>
      <Shell />
    </UiProvider>
  );
}
