// Calendar: years → months → days, only those that have items, each cell
// with a count and a cover thumbnail. Clicking a day opens its grid. Arrow
// keys move between cells, Enter opens, Backspace goes up a level.

import { useEffect, useMemo, useRef, useState } from "react";

import { api, fileUrl, type Bucket } from "../api";
import type { CalendarLevelDto } from "../bindings/CalendarLevelDto";
import * as fmt from "../format";
import { useUi } from "../state";
import { effectiveQuery } from "../uiModel";
import { dayLabel } from "../useCalendarTree";

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

  // Cells shown: only the years, months and days that have items (the
  // backend returns no bucket for an empty one).
  const cells: { key: string; label: string; bucket: Bucket }[] = useMemo(
    () =>
      (buckets ?? []).map((b) => {
        const [y, m, d] = b.key.split("-").map(Number);
        const label =
          b.key === "undated"
            ? "No date"
            : year === null
              ? b.key
              : month === null
                ? fmt.monthName(m)
                : dayLabel(y, m, d);
        return { key: b.key, label, bucket: b };
      }),
    [buckets, year, month],
  );

  // Moving between levels redraws every cell: keep keyboard focus in the
  // calendar (on the new level's first cell) instead of losing it.
  const keep = useRef(false);
  const remember = () => {
    keep.current = ref.current?.contains(document.activeElement) ?? false;
  };
  useEffect(() => {
    if (!keep.current || buckets === null) return;
    keep.current = false;
    ref.current?.querySelector<HTMLElement>('[data-cell="0"]')?.focus();
  }, [buckets]);
  const open = (k: string) => {
    remember();
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
    remember();
    if (month !== null) dispatch({ t: "cal", cal: { year, month: null, day: null } });
    else if (year !== null) dispatch({ t: "cal", cal: { year: null, month: null, day: null } });
  };
  const cols = month !== null ? 7 : 4;
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
    // Enter and Space are the cell buttons' own: handling them here too
    // opened two levels at once.
    if (e.key in moves) {
      setFocus((f) => Math.max(0, Math.min(cells.length - 1, f + moves[e.key])));
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
      ) : cells.length === 0 ? (
        <div className="empty">No dated items.</div>
      ) : (
        <div
          ref={ref}
          className={`cal-grid ${month !== null ? "days" : year !== null ? "months" : "years"}`}
          role="group"
          aria-label={month !== null ? "Days" : year !== null ? "Months" : "Years"}
          onKeyDown={onKey}
        >
          {cells.map((c, i) => {
            const b = c.bucket;
            const src = fileUrl(b.cover);
            return (
              <button
                key={c.key}
                data-cell={i}
                data-key={c.key}
                data-testid="cal-cell"
                tabIndex={i === focus ? 0 : -1}
                className="cal-cell"
                onClick={() => open(c.key)}
                onFocus={() => setFocus(i)}
                aria-label={`${c.label}: ${fmt.plural(b.count, "item")}`}
              >
                {src ? <img src={src} alt="" /> : <div className="ph" />}
                <span className="cal-label">{c.label}</span>
                <span className="cal-count">{fmt.count(b.count)}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
