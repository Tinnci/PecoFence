import type { FocusModel, JobView } from "./contracts/setup";
import { namedPhase } from "./release-lines";
export type BuildAssociation = {
  kind: "configured" | "input" | "name" | "unconfirmed" | "none";
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
  const configured = lineJobs ?? model?.branch_jobs?.[branch];
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
