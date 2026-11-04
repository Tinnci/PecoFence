import type { BuildCapacity, CapacityTask } from "./contracts/setup";
export type QueueForecast = {
  id: number;
  reason:
    | "estimated"
    | "stale"
    | "constraints"
    | "blocked"
    | "offline"
    | "releaseUnknown"
    | "orderUnknown";
  online: number;
  idle: number;
  offline: number;
  ahead: number;
  startLow: number | null;
  startHigh: number | null;
};
const simpleLabel = (label: string | null) =>
  !!label && /^[a-zA-Z0-9_.:-]+$/.test(label);
const finite = (n: number) => (Number.isFinite(n) ? n : null);
/** Two FIFO scenarios over compatible executor slots; unknown durations keep the upper time unknown. */
export function forecastQueue(
  capacity: BuildCapacity,
  now: number,
): Map<number, QueueForecast> {
  const snapshot = Date.parse(capacity.observed_at);
  const stale =
    !Number.isFinite(snapshot) ||
    snapshot > now + 5000 ||
    now - snapshot > 120000;
  const slots = capacity.slots.map((slot) => ({
    ...slot,
    early: slot.idle
      ? snapshot
      : slot.started && slot.low && slot.started <= snapshot
        ? Math.max(snapshot, slot.started + slot.low)
        : snapshot,
    late: slot.idle
      ? snapshot
      : slot.started &&
          slot.high &&
          slot.started <= snapshot &&
          slot.started + slot.high > snapshot
        ? slot.started + slot.high
        : Infinity,
  }));
  const jobEarly = new Map<string, number>();
  const jobLate = new Map<string, number>();
  for (const slot of slots.filter((s) => s.online && !s.idle && s.job)) {
    jobEarly.set(
      slot.job!,
      Math.max(jobEarly.get(slot.job!) ?? snapshot, slot.early),
    );
    jobLate.set(
      slot.job!,
      Math.max(jobLate.get(slot.job!) ?? snapshot, slot.late),
    );
  }
  const tasks = [...capacity.tasks].sort(
    (a, b) => (a.since ?? Infinity) - (b.since ?? Infinity) || a.id - b.id,
  );
  const forecasts = new Map<number, QueueForecast>();
  const scheduled: CapacityTask[] = [];
  // A ready task with unknown eligibility could consume any pool. Do not silently skip it.
  let missingConstraints = false;
  const missingOrder = tasks.some(
    (task) => task.buildable && !task.blocked && !task.since,
  );
  for (const task of tasks) {
    const compatible = slots.filter(
      (slot) => simpleLabel(task.label) && slot.labels.includes(task.label!),
    );
    const online = compatible.filter((slot) => slot.online);
    const result: QueueForecast = {
      id: task.id,
      reason: "estimated",
      online: online.length,
      idle: online.filter((slot) => slot.idle).length,
      offline: compatible.length - online.length,
      ahead: scheduled.filter(
        (earlier) =>
          compatible.some(
            (slot) => !!earlier.label && slot.labels.includes(earlier.label),
          ) || earlier.job === task.job,
      ).length,
      startLow: null,
      startHigh: null,
    };
    forecasts.set(task.id, result);
    if (stale) {
      result.reason = "stale";
      continue;
    }
    if (task.blocked || !task.buildable) {
      result.reason = "blocked";
      continue;
    }
    if (!simpleLabel(task.label) || missingConstraints) {
      result.reason = "constraints";
      missingConstraints = true;
      continue;
    }
    if (missingOrder) {
      result.reason = "orderUnknown";
      continue;
    }
    if (!online.length) {
      result.reason = "offline";
      continue;
    }
    const earliest = [...online].sort((a, b) => a.early - b.early)[0];
    const latest = [...online].sort((a, b) => a.late - b.late)[0];
    // Unknown per-job concurrency is conservatively serialized in the late scenario.
    const early = Math.max(
      earliest.early,
      task.concurrent === false
        ? (jobEarly.get(task.job) ?? snapshot)
        : snapshot,
    );
    const late = Math.max(
      latest.late,
      task.concurrent !== true ? (jobLate.get(task.job) ?? snapshot) : snapshot,
    );
    result.startLow = finite(Math.min(early, late));
    result.startHigh = finite(Math.max(early, late));
    if (!Number.isFinite(late)) result.reason = "releaseUnknown";
    earliest.early = early + (task.low && task.low > 0 ? task.low : 0);
    latest.late = late + (task.high && task.high > 0 ? task.high : Infinity);
    jobEarly.set(task.job, earliest.early);
    jobLate.set(task.job, latest.late);
    scheduled.push(task);
  }
  return forecasts;
}
