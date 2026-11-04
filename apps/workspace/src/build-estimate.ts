import type { BuildRun, JobView } from "./contracts/setup";

const usable = (run: BuildRun) =>
  !run.building &&
  run.result === "SUCCESS" &&
  Number.isFinite(run.duration) &&
  run.duration > 0;
function percentile(sorted: number[], q: number) {
  const index = (sorted.length - 1) * q;
  const lower = Math.floor(index);
  return Math.round(
    sorted[lower] +
      (sorted[Math.ceil(index)] - sorted[lower]) * (index - lower),
  );
}
/** Job-local successful history. Prefer matching branch inputs and node only with >=3 samples. */
export function durationHistory(job: JobView) {
  const current = job.builds[0];
  let runs = job.builds.filter(usable).slice(0, 10);
  let matched = false;
  const keys = ["BRANCH", "BRANCH_T", "BRANCH_V"].filter(
    (key) => current?.source_parameters?.[key],
  );
  if (keys.length) {
    const same = runs.filter((run) =>
      keys.every(
        (key) =>
          run.source_parameters?.[key] === current.source_parameters?.[key],
      ),
    );
    if (same.length >= 3) {
      runs = same;
      matched = true;
    }
  }
  let sameNode = false;
  if (current?.node) {
    const same = runs.filter((run) => run.node === current.node);
    if (same.length >= 3) {
      runs = same;
      sameNode = true;
    }
  }
  const durations = runs.map((run) => run.duration).sort((a, b) => a - b);
  return {
    count: runs.length,
    matched,
    sameNode,
    median: durations.length ? percentile(durations, 0.5) : null,
    low: durations.length >= 3 ? percentile(durations, 0.25) : null,
    high: durations.length >= 3 ? percentile(durations, 0.75) : null,
  };
}
export function runtimeEstimate(job: JobView, now: number) {
  const run = job.builds[0];
  const history = durationHistory(job);
  const elapsed =
    run &&
    Number.isFinite(run.timestamp) &&
    run.timestamp > 0 &&
    run.timestamp <= now
      ? now - run.timestamp
      : null;
  const source =
    history.count >= 3
      ? "history"
      : run && run.estimated > 0
        ? "jenkins"
        : "unknown";
  const expected =
    source === "history"
      ? history.median
      : source === "jenkins"
        ? run!.estimated
        : null;
  const low = source === "history" ? history.low : expected;
  const high = source === "history" ? history.high : expected;
  const overdue = elapsed !== null && high !== null && elapsed >= high;
  return {
    history,
    source,
    elapsed,
    expected,
    low,
    high,
    overdue,
    remainingLow:
      elapsed !== null && low !== null ? Math.max(0, low - elapsed) : null,
    remainingHigh:
      elapsed !== null && high !== null ? Math.max(0, high - elapsed) : null,
    progress:
      elapsed !== null && expected && !overdue
        ? Math.min(95, Math.round((elapsed / expected) * 100))
        : null,
    finishLow: run && low ? run.timestamp + low : null,
    finishHigh: run && high ? run.timestamp + high : null,
  };
}
