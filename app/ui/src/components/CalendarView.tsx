// Calendar: years → months → days, each cell with a count and a cover
// thumbnail. Clicking a day opens its grid. Arrow keys move between cells,
// Enter opens, Backspace goes up a level.

import { useEffect, useMemo, useRef, useState } from "react";

import { api, fileUrl, type Bucket } from "../api";
import type { CalendarLevelDto } from "../bindings/CalendarLevelDto";
import * as fmt from "../format";
import { useUi } from "../state";
import { effectiveQuery } from "../uiModel";

export function CalendarView() {
  const { s, dispatch, refreshes } = useUi();
  const { year, month } = s.cal;
  const [buckets, setBuckets] = useState<Bucket[] | null>(null);
  const [focus, setFocus] = useState(0);
  const level: CalendarLevelDto =
    year === null
      ? { level: "years" }
      : month === null
        ? { level: "months", year }
        : { level: "days", year, month };
  const query = useMemo(() => effectiveQuery({ ...s, section: { kind: "all" } }), [s]);
  const key = JSON.stringify([query, level, refreshes]);
  useEffect(() => {
    let live = true;
    setBuckets(null);
    api
      .calendar(query, level)
      .then((b) => live && setBuckets(b))
      .catch((e) => console.error("calendar", e));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  // Cells shown: every month / every day of the month, with or without items.
  const cells: { key: string; label: string; bucket?: Bucket; pad?: boolean }[] = useMemo(() => {
    const by = new Map((buckets ?? []).map((b) => [b.key, b]));
    if (year === null)
      return (buckets ?? []).map((b) => ({
        key: b.key,
        label: b.key === "undated" ? "No date" : b.key,
        bucket: b,
      }));
    if (month === null)
      return Array.from({ length: 12 }, (_, i) => {
        const k = `${year}-${String(i + 1).padStart(2, "0")}`;
        return { key: k, label: fmt.monthName(i + 1), bucket: by.get(k) };
      });
    const pad = Array.from({ length: fmt.firstWeekday(year, month) }, (_, i) => ({
      key: `pad${i}`,
      label: "",
      pad: true,
    }));
    const days = Array.from({ length: fmt.daysIn(year, month) }, (_, i) => {
      const k = `${year}-${String(month).padStart(2, "0")}-${String(i + 1).padStart(2, "0")}`;
      return { key: k, label: String(i + 1), bucket: by.get(k) };
    });
    return [...pad, ...days];
  }, [buckets, year, month]);

  const open = (k: string) => {
    if (k === "undated") {
      dispatch({ t: "filters", filters: { ...s.filters, undated: true } });
      dispatch({ t: "mode", mode: "grid" });
      return;
    }
    const [y, m, d] = k.split("-").map(Number);
    if (year === null) dispatch({ t: "cal", cal: { year: y, month: null, day: null } });
    else if (month === null) dispatch({ t: "cal", cal: { year: y, month: m, day: null } });
    else dispatch({ t: "cal", cal: { year: y, month: m, day: d } });
  };
  const up = () => {
    if (month !== null) dispatch({ t: "cal", cal: { year, month: null, day: null } });
    else if (year !== null) dispatch({ t: "cal", cal: { year: null, month: null, day: null } });
  };
  const cols = month !== null ? 7 : 4;
  const real = cells.filter((c) => !c.pad);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => setFocus(0), [year, month]);
  useEffect(() => {
    const el = ref.current?.querySelector<HTMLElement>(`[data-cell="${focus}"]`);
    if (el && ref.current?.contains(document.activeElement)) el.focus();
  }, [focus]);
  const onKey = (e: React.KeyboardEvent) => {
    const moves: Record<string, number> = {
      ArrowLeft: -1,
      ArrowRight: 1,
      ArrowUp: -cols,
      ArrowDown: cols,
    };
    if (e.key in moves) {
      setFocus((f) => Math.max(0, Math.min(real.length - 1, f + moves[e.key])));
    } else if (e.key === "Enter" || e.key === " ") {
      const c = real[focus];
      if (c?.bucket) open(c.key);
    } else if (e.key === "Backspace") up();
    else return;
    e.preventDefault();
  };
  return (
    <div className="calendar" data-testid="calendar">
      <nav className="crumbs" aria-label="Calendar level">
        <button onClick={() => dispatch({ t: "cal", cal: { year: null, month: null, day: null } })}>
          Calendar
        </button>
        {year !== null && (
          <>
            {" › "}
            <button onClick={() => dispatch({ t: "cal", cal: { year, month: null, day: null } })}>
              {year}
            </button>
          </>
        )}
        {month !== null && <> › {fmt.monthName(month)}</>}
      </nav>
      {buckets === null ? (
        <div className="empty">Loading…</div>
      ) : real.length === 0 ? (
        <div className="empty">No dated items.</div>
      ) : (
        <div
          ref={ref}
          className={`cal-grid ${month !== null ? "days" : year !== null ? "months" : "years"}`}
          role="grid"
          aria-label={month !== null ? "Days" : year !== null ? "Months" : "Years"}
          onKeyDown={onKey}
        >
          {month !== null &&
            ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map((d) => (
              <div key={d} className="weekday" aria-hidden>
                {d}
              </div>
            ))}
          {cells.map((c) => {
            if (c.pad) return <div key={c.key} className="cal-cell pad" aria-hidden />;
            const i = real.indexOf(c);
            const b = c.bucket;
            const src = fileUrl(b?.cover);
            return (
              <button
                key={c.key}
                data-cell={i}
                data-key={c.key}
                data-testid="cal-cell"
                tabIndex={i === focus ? 0 : -1}
                className={`cal-cell${b ? "" : " none"}`}
                disabled={!b}
                onClick={() => b && open(c.key)}
                onFocus={() => setFocus(i)}
                aria-label={`${c.label}: ${b ? fmt.plural(b.count, "item") : "no items"}`}
              >
                {src ? <img src={src} alt="" /> : <div className="ph" />}
                <span className="cal-label">{c.label}</span>
                {b && <span className="cal-count">{fmt.count(b.count)}</span>}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
