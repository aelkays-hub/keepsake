// The main window: toolbar, sidebar · main area · inspector, status bar.

import { useEffect, useState } from "react";

import { api, type AppInfo } from "./api";
import { BackupScreen } from "./components/BackupScreen";
import { CalendarView } from "./components/CalendarView";
import { CrashBanner } from "./components/CrashBanner";
import { DetailsView } from "./components/DetailsView";
import { FirstRun } from "./components/FirstRun";
import { GridView } from "./components/GridView";
import { Inspector } from "./components/Inspector";
import { SettingsScreen } from "./components/SettingsScreen";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TimeShiftDialog } from "./components/TimeShiftDialog";
import { Toolbar } from "./components/Toolbar";
import { Viewer } from "./components/Viewer";
import * as fmt from "./format";
import * as sel from "./selection";
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
  // Enter or double-click on an item: the full-screen viewer.
  const open = (i: number) => {
    dispatch({ t: "sel", sel: sel.has(s.sel, i) ? { ...s.sel, focus: i } : sel.only(i) });
    dispatch({ t: "viewer", open: true });
  };
  if (s.screen === "backup")
    return (
      <main className="main screen-main" aria-label="Backup">
        <BackupScreen />
      </main>
    );
  if (s.screen === "settings")
    return (
      <main className="main screen-main" aria-label="Settings">
        <SettingsScreen />
      </main>
    );
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
  const { s, dispatch, view } = useUi();
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
        else if (k === ",") dispatch({ t: "screen", screen: "settings" });
        else if (
          k === "t" &&
          s.screen === "library" &&
          (s.sel.focus !== null || s.sel.ranges.length)
        )
          dispatch({ t: "dialog", dialog: "timeShift" });
        else if (k === "o" && view && s.sel.focus !== null && !s.viewer)
          api
            .viewRows(view.view, s.sel.focus, 1)
            .then(([r]) => r && api.openExternal(r.id, "primary"))
            .catch(console.error);
        else if (k === "f") document.getElementById("search")?.focus();
        else if (k in modes) dispatch({ t: "mode", mode: modes[k as keyof typeof modes] });
        else return;
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dispatch, s.inspector, s.screen, s.sel, s.viewer, view]);
  return (
    <div className={`shell${s.inspector && s.screen === "library" ? "" : " no-inspector"}`}>
      <Toolbar />
      <Sidebar />
      <Main />
      {s.inspector && s.screen === "library" && <Inspector />}
      <StatusBar />
      <CrashBanner />
      {s.viewer && <Viewer />}
      {s.dialog === "timeShift" && <TimeShiftDialog />}
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
