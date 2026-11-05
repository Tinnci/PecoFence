import type { WorkItem } from "./contracts/setup";
import { updatedAt } from "./work-activity";

/** Workflow columns left to right; finished work sits apart from work in progress. */
export const FLOW_COLUMNS = [
  "pending",
  "active",
  "review",
  "testing",
  "blocked",
] as const;
const DAY = 86_400_000;
const WEEKS = 12;
/** Work this long in one status is aged, however slow the usual cycle has become. */
const AGED_DAYS = 14;
const MIN_SAMPLE = 5;

/** Days an item has sat in its current status. */
export function statusAge(item: WorkItem, now = Date.now()) {
  const since =
    updatedAt(item.activity.status_since) ?? updatedAt(item.activity.created);
  return since === null ? null : Math.max(0, Math.floor((now - since) / DAY));
}

function percentile(sorted: number[], share: number) {
  if (!sorted.length) return null;
  return sorted[
    Math.min(sorted.length - 1, Math.ceil(sorted.length * share) - 1)
  ];
}

export type FlowMetrics = {
  wip: number;
  blocked: number;
  aged: number;
  /** Days from created to done for work finished in the last twelve weeks. */
  cycleP50: number | null;
  cycleP85: number | null;
  /** The age past which work counts as aged. */
  agedAfter: number;
  /** Per seven days, oldest first; the last entry ends now. */
  finished: number[];
  arrived: number[];
};

/** Kanban's flow measures. Only real status history counts as a finish date. */
export function flowMetrics(items: WorkItem[], now = Date.now()): FlowMetrics {
  const finished = new Array<number>(WEEKS).fill(0);
  const arrived = new Array<number>(WEEKS).fill(0);
  const bucket = (at: number | null, into: number[]) => {
    if (at === null || at > now) return false;
    const week = Math.floor((now - at) / (7 * DAY));
    if (week >= WEEKS) return false;
    into[WEEKS - 1 - week]++;
    return true;
  };
  const cycles: number[] = [];
  for (const item of items) {
    const created = updatedAt(item.activity.created);
    bucket(created, arrived);
    if (
      item.group !== "done" ||
      item.activity.status_since_source !== "history"
    )
      continue;
    const done = updatedAt(item.activity.status_since);
    if (bucket(done, finished) && created !== null && done !== null)
      cycles.push((done - created) / DAY);
  }
  cycles.sort((a, b) => a - b);
  const p85 = percentile(cycles, 0.85);
  const agedAfter =
    cycles.length >= MIN_SAMPLE && p85 !== null
      ? Math.min(AGED_DAYS, Math.ceil(p85))
      : AGED_DAYS;
  const open = items.filter((i) =>
    (FLOW_COLUMNS as readonly string[]).includes(i.group),
  );
  const whole = (n: number | null) => (n === null ? null : Math.round(n));
  return {
    wip: open.filter((i) => i.group !== "pending").length,
    blocked: open.filter((i) => i.group === "blocked").length,
    aged: open.filter((i) => (statusAge(i, now) ?? 0) > agedAfter).length,
    cycleP50: whole(percentile(cycles, 0.5)),
    cycleP85: whole(p85),
    agedAfter,
    finished,
    arrived,
  };
}

/** Column contents, oldest first: on a board, what has waited longest is pulled first. */
export function boardColumns(items: WorkItem[], now = Date.now()) {
  const columns = new Map<string, WorkItem[]>(FLOW_COLUMNS.map((c) => [c, []]));
  for (const item of items) columns.get(item.group)?.push(item);
  for (const list of columns.values())
    list.sort((a, b) => (statusAge(b, now) ?? 0) - (statusAge(a, now) ?? 0));
  return columns;
}

const closedAt = (item: WorkItem) =>
  item.group === "done" || item.group === "cancelled"
    ? updatedAt(item.activity.status_since)
    : null;

/** Running totals of work created and resolved, one point a day, from zero at the start. */
export function createdResolved(
  items: WorkItem[],
  now = Date.now(),
  days = 84,
) {
  const start = now - days * DAY;
  const created = new Array<number>(days + 1).fill(0);
  const resolved = new Array<number>(days + 1).fill(0);
  const add = (at: number | null, into: number[]) => {
    if (at === null || at > now) return;
    into[Math.max(0, Math.ceil((at - start) / DAY))]++;
  };
  for (const item of items) {
    const made = updatedAt(item.activity.created);
    if (made !== null && made >= start) add(made, created);
    const closed = closedAt(item);
    if (closed !== null && closed >= start) add(closed, resolved);
  }
  for (let i = 1; i <= days; i++) {
    created[i] += created[i - 1];
    resolved[i] += resolved[i - 1];
  }
  return { start, created, resolved };
}

export type Burndown = {
  /** Work open on each day; null for days still to come. */
  remaining: (number | null)[];
  /** Everything in the iteration by that day, so added work shows as scope growth. */
  scope: (number | null)[];
  ideal: number[];
};

/** An iteration's burndown. Reopened work counts as open only from its latest status. */
export function burndown(
  items: WorkItem[],
  start: number,
  days: number,
  now = Date.now(),
): Burndown {
  const end = start + days * DAY;
  const list = items.filter((item) => {
    const closed = closedAt(item);
    return (
      item.group !== "cancelled" &&
      (updatedAt(item.activity.created) ?? Infinity) < end &&
      (closed === null || closed >= start)
    );
  });
  const remaining: (number | null)[] = [];
  const scope: (number | null)[] = [];
  for (let day = 0; day <= days; day++) {
    const at = start + day * DAY;
    if (at > now + DAY) {
      remaining.push(null);
      scope.push(null);
      continue;
    }
    const inScope = list.filter(
      (i) => (updatedAt(i.activity.created) ?? Infinity) <= at,
    );
    scope.push(inScope.length);
    remaining.push(
      inScope.filter((i) => {
        const closed = closedAt(i);
        return closed === null || closed > at;
      }).length,
    );
  }
  const first = remaining[0] ?? 0;
  const ideal = remaining.map((_, day) =>
    Math.round((first * (days - day)) / days),
  );
  return { remaining, scope, ideal };
}

/** The Monday the current two-week iteration began, counted from a fixed Monday. */
export function iterationStart(now = Date.now(), weeks = 2) {
  const anchor = Date.UTC(2024, 0, 1) - 8 * 3_600_000;
  const span = weeks * 7 * DAY;
  return anchor + Math.floor((now - anchor) / span) * span;
}
