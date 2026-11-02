import { sourceRoutes } from "./job-sources";
import type { FocusModel, JobView } from "./contracts/setup";
import { namedPhase } from "./release-lines";
export type BuildAssociation = {
  kind:
    | "configured"
    | "input"
    | "name"
    | "unconfirmed"
    | "none"
    | "source"
    | "historical";
  jobs: JobView[];
  candidates: JobView[];
  missing: string[];
};
/** Keep unknown-phase candidates separate from confirmed/local associations. */
export function buildAssociation(
  branch: string,
  phase: string,
  jobs: JobView[] = [],
  model?: FocusModel,
  lineJobs?: string[],
): BuildAssociation {
  const routes = sourceRoutes(model, branch);
  const configured = model?.branch_jobs?.[branch] ?? lineJobs;
  const routed = routes.current.map((entry) => entry.job);
  const historical = new Set(routes.historical.map((entry) => entry.job));
  // A job's recorded history applies only to that job, not every build target.
  if (configured !== undefined || routed.length) {
    const names = [...new Set([...(configured ?? []), ...routed])].filter(
      (name) => !historical.has(name) && !model?.ignored_jobs?.includes(name),
    );
    return {
      kind:
        !names.length && historical.size && configured?.length
          ? "historical"
          : configured !== undefined
            ? "configured"
            : "source",
      jobs: jobs.filter((job) => names.includes(job.job)),
      candidates: [],
      missing: names.filter((name) => !jobs.some((job) => job.job === name)),
    };
  }
  jobs = jobs.filter((job) => !historical.has(job.job));
  const active = jobs.filter(
    (j) =>
      !j.disabled && j.phase !== "old" && !model?.ignored_jobs?.includes(j.job),
  );
  const inputs = active.filter((j) =>
    ["BRANCH", "BRANCH_T", "BRANCH_V"].some((key) => {
      const value = j.parameters[key];
      return (
        typeof value === "string" &&
        value.trim().replace(/^refs\/heads\//, "") === branch
      );
    }),
  );
  if (inputs.length)
    return { kind: "input", jobs: inputs, candidates: [], missing: [] };
  const sameVariant = active.filter((j) =>
    branch
      .toUpperCase()
      .startsWith(`${j.name.replace(/_(IR|PB|MP)$/i, "").toUpperCase()}_`),
  );
  const samePhase = sameVariant.filter((j) => j.phase === phase);
  if (samePhase.length)
    return { kind: "name", jobs: samePhase, candidates: [], missing: [] };
  const candidates = sameVariant.filter(
    (j) => j.phase === "main" && namedPhase(j.name) === "main",
  );
  return {
    kind: candidates.length
      ? "unconfirmed"
      : routes.historical.length
        ? "historical"
        : "none",
    jobs: [],
    candidates,
    missing: [],
  };
}
export function withBranchJob(
  model: FocusModel,
  branch: string,
  job: string | null,
): FocusModel {
  const branch_jobs = { ...model.branch_jobs };
  if (job === null) delete branch_jobs[branch];
  else
    branch_jobs[branch] = [...new Set([...(branch_jobs[branch] ?? []), job])];
  return { ...model, branch_jobs };
}

/** Replace one branch's explicit targets; [] intentionally disables automatic matches. */
export function withBranchJobs(
  model: FocusModel,
  branch: string,
  jobs: string[],
): FocusModel {
  return {
    ...model,
    branch_jobs: { ...model.branch_jobs, [branch]: [...new Set(jobs)] },
  };
}
