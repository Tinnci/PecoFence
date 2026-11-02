import type { BuildRun, JobView, Listed } from "./contracts/setup";

export type TimelineEvent =
  | { kind: "change"; key: string; time: number | null; change: Listed }
  | { kind: "build"; key: string; time: number; job: JobView; run: BuildRun };
const timestamp = (value?: number | null) =>
  value && Number.isFinite(value) && value > 0 && value <= 8.64e15
    ? value
    : null;

/** Dates without times stay in their own section instead of inventing midnight. */
export function branchTimeline(
  changes: Listed[],
  jobs: JobView[],
): TimelineEvent[] {
  const events: TimelineEvent[] = changes.map((change) => ({
    kind: "change",
    key: `change:${change.number}`,
    time: timestamp(change.created_at),
    change,
  }));
  for (const job of jobs)
    for (const run of job.builds) {
      const time = timestamp(run.timestamp);
      if (time !== null)
        events.push({
          kind: "build",
          key: `build:${job.job}:${run.number}`,
          time,
          job,
          run,
        });
    }
  return events.sort(
    (a, b) =>
      (b.time ?? -Infinity) - (a.time ?? -Infinity) ||
      a.key.localeCompare(b.key),
  );
}

export function branchInputMatches(run: BuildRun, branch: string) {
  return Object.entries(run.source_parameters ?? {}).some(
    ([key, value]) =>
      ["BRANCH", "BRANCH_T", "BRANCH_V"].includes(key) &&
      value.replace(/^refs\/heads\//, "") === branch,
  );
}
