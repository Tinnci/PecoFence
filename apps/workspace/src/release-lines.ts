import type {
  BranchStrip,
  FocusModel,
  JobView,
  ModelSummary,
} from "./contracts/setup";
export const phases = ["main", "ir", "pb", "mp", "old"] as const;
export function namedPhase(name: string) {
  const u = name.toUpperCase();
  const marked = (m: string) =>
    u.endsWith(m) || u.includes(`${m}_`) || u.includes(`${m}-`);
  return marked("_OLD")
    ? "old"
    : marked("_IR")
      ? "ir"
      : marked("_PB")
        ? "pb"
        : marked("_MP")
          ? "mp"
          : "main";
}
export type BranchGroup = {
  id: string;
  phase: string;
  label?: string;
  lifecycle?: string;
  jobs?: string[];
  branches: BranchStrip[];
};
export function releaseBranchGroups(
  m: ModelSummary,
  saved?: FocusModel,
): BranchGroup[] {
  const strips = new Map(m.branches.map((b) => [b.branch, b]));
  for (const branch of saved?.branches ?? [])
    if (!strips.has(branch))
      strips.set(branch, {
        branch,
        phase: namedPhase(branch),
        total: 0,
        days: Array(84).fill(0),
        flagged: 0,
      });
  const assigned = new Set(saved?.release_lines?.flatMap((l) => l.branches));
  const lines: BranchGroup[] = (saved?.release_lines ?? []).map((line) => ({
    ...line,
    branches: line.branches
      .filter((b) => strips.has(b))
      .map((b) => ({ ...strips.get(b)!, phase: line.phase })),
  }));
  const rest = [...strips.values()].filter((b) => !assigned.has(b.branch));
  return [
    ...lines,
    ...phases.map((phase) => ({
      id: `phase:${phase}`,
      phase,
      branches: rest.filter((b) => b.phase === phase),
    })),
  ].filter((g) => g.branches.length);
}
export function releaseJobs(jobs: JobView[], saved?: FocusModel) {
  return jobs
    .filter((j) => !saved?.ignored_jobs?.includes(j.job))
    .map((job) => {
      const line = saved?.release_lines?.find((l) => l.jobs.includes(job.job));
      return line && job.phase !== line.phase
        ? { ...job, phase: line.phase }
        : job;
    });
}
export function releaseJobGroups(jobs: JobView[], saved?: FocusModel) {
  const assigned = new Set(saved?.release_lines?.flatMap((l) => l.jobs));
  return [
    ...(saved?.release_lines ?? []).map((line) => ({
      ...line,
      jobs: jobs.filter((j) => line.jobs.includes(j.job)),
    })),
    ...phases.map((phase) => ({
      id: `phase:${phase}`,
      label: undefined,
      lifecycle: undefined,
      phase,
      jobs: jobs.filter((j) => !assigned.has(j.job) && j.phase === phase),
    })),
  ].filter((g) => g.jobs.length);
}
