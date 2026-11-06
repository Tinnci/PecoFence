import type { CalEvent } from "./contracts/setup";

const DAYS = 7;
const pad = (n: number) => String(n).padStart(2, "0");

export const isoDay = (at: Date) =>
  `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;

/** The weeks that show a month, Monday first, as ISO days. */
export function monthWeeks(year: number, month: number) {
  const first = new Date(year, month, 1);
  const cursor = new Date(year, month, 1 - ((first.getDay() + 6) % DAYS));
  const weeks: string[][] = [];
  do {
    const week: string[] = [];
    for (let i = 0; i < DAYS; i++) {
      week.push(isoDay(cursor));
      cursor.setDate(cursor.getDate() + 1);
    }
    weeks.push(week);
  } while (cursor.getMonth() === month);
  return weeks;
}

export type Bar = {
  event: CalEvent;
  column: number;
  span: number;
  lane: number;
  /** It carries on past this week's edge, so the bar stays open on that side. */
  before: boolean;
  after: boolean;
};

/** A stage's length, counting both ends. */
export function spanDays(event: CalEvent) {
  const day = (text: string) => Date.parse(`${text}T00:00:00Z`);
  return Math.round((day(event.end) - day(event.start)) / 86_400_000) + 1;
}

/** A week's events as bars: dated milestones take the top lanes so a fold never hides them,
 * then each window takes the first free lane. */
export function weekBars(week: string[], events: CalEvent[]) {
  const first = week[0];
  const last = week[DAYS - 1];
  const within = events
    .filter((e) => e.start <= last && e.end >= first)
    .map((event) => {
      const from = event.start < first ? first : event.start;
      const to = event.end > last ? last : event.end;
      const column = week.indexOf(from);
      return {
        event,
        column,
        span: week.indexOf(to) - column + 1,
        before: event.start < first,
        after: event.end > last,
      };
    })
    .sort(
      (a, b) =>
        Number(b.event.milestone) - Number(a.event.milestone) ||
        a.column - b.column ||
        b.span - a.span ||
        a.event.project.localeCompare(b.event.project),
    );
  const reached: [number, number][][] = [];
  return within.map((bar): Bar => {
    const end = bar.column + bar.span - 1;
    let lane = reached.findIndex((used) =>
      used.every(([from, to]) => to < bar.column || from > end),
    );
    if (lane < 0) lane = reached.length;
    (reached[lane] ??= []).push([bar.column, end]);
    return { ...bar, lane };
  });
}

export type CalendarView = {
  scope: "followed" | "all";
  delayed: boolean;
  words: string;
};

/** A row marked 已延期 was replaced by the one after it, so it stays out unless asked for. */
export function visibleEvents(events: CalEvent[], view: CalendarView) {
  const words = view.words.trim().toLowerCase();
  return events.filter(
    (e) =>
      (view.scope === "all" || (e.model !== "" && !e.legacy)) &&
      (view.delayed || e.status !== "已延期") &&
      (!words ||
        `${e.project} ${e.version} ${e.customer} ${e.owner}`
          .toLowerCase()
          .includes(words)),
  );
}

export function shiftMonth(year: number, month: number, by: number) {
  const at = new Date(year, month + by, 1);
  return { year: at.getFullYear(), month: at.getMonth() };
}
