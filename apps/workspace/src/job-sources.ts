import type { FocusModel, JobSource, JobView } from "./contracts/setup";
/** A local routing record identifies tracking scope, not a frozen build baseline. */
export function sourceBranches(model: FocusModel): FocusModel {
  const job_sources = Object.fromEntries(
    Object.entries(model.job_sources ?? {}).map(([job, source]) => [
      job,
      {
        ...source,
        branch: source.branch.trim(),
        previous: [
          ...new Set(
            source.previous.map((branch) => branch.trim()).filter(Boolean),
          ),
        ],
      },
    ]),
  );
  const sources = Object.values(job_sources);
  return {
    ...model,
    job_sources,
    branches: [
      ...new Set([
        ...model.branches,
        ...sources
          .flatMap((s) => [
            s.branch,
            ...s.previous,
            ...(s.history ?? []).map((r) => r.branch),
          ])
          .filter(Boolean),
      ]),
    ],
  };
}
export function sourceRoutes(model: FocusModel | undefined, branch: string) {
  const entries = Object.entries(model?.job_sources ?? {}).map(
    ([job, source]) => ({ job, source }),
  );
  return {
    current: entries.filter((e) => e.source.branch === branch),
    historical: entries.filter(
      (e) =>
        e.source.branch !== branch &&
        (e.source.previous.includes(branch) ||
          e.source.history?.some((r) => r.branch === branch)),
    ),
  };
}
/** Unknown cutovers never assign an entire job's history to a new source branch. */
export function sourceTimelineJobs(
  jobs: JobView[],
  branch: string,
  model?: FocusModel,
): JobView[] {
  return jobs.map((job) => {
    const source = model?.job_sources?.[job.job];
    if (!source) return job;
    const builds = job.builds.filter((build) => {
      const segment = source.history?.find(
        (range) =>
          build.number >= range.first_build && build.number <= range.last_build,
      );
      if (segment) return segment.branch === branch;
      if (source.since_build && build.number >= source.since_build)
        return source.branch === branch;
      if (source.history?.length) return false;
      return (
        !!source.since_build &&
        source.previous.length === 1 &&
        source.previous[0] === branch &&
        build.number < source.since_build
      );
    });
    return { ...job, builds };
  });
}
export function withJobSource(
  model: FocusModel,
  job: string,
  source: JobSource,
): FocusModel {
  return sourceBranches({
    ...model,
    job_sources: { ...model.job_sources, [job]: source },
  });
}
