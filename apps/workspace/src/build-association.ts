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
  if (routes.current.length) {
    const names = routes.current.map((e) => e.job);
    return {
      kind: "source",
      jobs: jobs.filter((j) => names.includes(j.job)),
      candidates: [],
      missing: names.filter((name) => !jobs.some((j) => j.job === name)),
    };
  }
  if (routes.historical.length)
    return { kind: "historical", jobs: [], candidates: [], missing: [] };
  // A known current source elsewhere overrides both old local links and name heuristics.
  jobs = jobs.filter(
    (job) => !model?.job_sources?.[job.job]?.previous.includes(branch),
  );
  const configured = model?.branch_jobs?.[branch] ?? lineJobs;
  if (configured !== undefined)
    return {
      kind: "configured",
      jobs: jobs.filter((j) => configured.includes(j.job)),
      candidates: [],
      missing: configured.filter((name) => !jobs.some((j) => j.job === name)),
    };
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
    kind: candidates.length ? "unconfirmed" : "none",
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
