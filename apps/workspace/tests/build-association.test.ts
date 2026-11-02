import { expect, it } from "vitest";
import {
  buildAssociation,
  withBranchJob,
  withBranchJobs,
} from "../src/build-association";
import type { FocusModel, JobView } from "../src/contracts/setup";
const job = (name: string, phase = "main", extra = {}): JobView => ({
  job: `folder/${name}`,
  name,
  phase,
  disabled: false,
  parameters: {},
  builds: [],
  changes: [],
  ...extra,
});
const branch = "E6751SM-A_SYS_A16_MP";
it("finds the bare A variant as a candidate without silently treating it as MP", () => {
  const result = buildAssociation(branch, "mp", [
    job("E6751SM-A"),
    job("E6751SM-B"),
    job("E6751SM-A_IR", "ir"),
    job("E6751SM-A_PB", "pb"),
    job("E6751SM-A_old", "old"),
    job("E6751SM-A", "main", { disabled: true }),
  ]);
  expect(result.kind).toBe("unconfirmed");
  expect(result.jobs).toEqual([]);
  expect(result.candidates.map((j) => j.name)).toEqual(["E6751SM-A"]);
});
it("prefers declared phases and explicit input refs over unknown phase names", () => {
  expect(
    buildAssociation(branch, "mp", [
      job("E6751SM-A"),
      job("E6751SM-A_MP", "mp"),
    ]).jobs.map((j) => j.name),
  ).toEqual(["E6751SM-A_MP"]);
  expect(
    buildAssociation(branch, "mp", [
      job("E6751SM-A", "main", {
        parameters: { BRANCH: `refs/heads/${branch}` },
      }),
    ]).kind,
  ).toBe("input");
  expect(
    buildAssociation(branch, "mp", [
      job("unrelated", "main", { parameters: { DESCRIPTION: branch } }),
    ]).kind,
  ).toBe("none");
});
it("honors configured jobs including cache gaps, without replacing them with guesses", () => {
  const model = {
    branch_jobs: { [branch]: ["folder/E6751SM-A"] },
  } as unknown as FocusModel;
  expect(
    buildAssociation(branch, "mp", [job("E6751SM-A")], model),
  ).toMatchObject({ kind: "configured", candidates: [], missing: [] });
  expect(
    buildAssociation(branch, "mp", [job("E6751SM-A_MP", "mp")], model),
  ).toMatchObject({
    kind: "configured",
    jobs: [],
    missing: ["folder/E6751SM-A"],
  });
  expect(
    buildAssociation(
      branch,
      "mp",
      [job("E6751SM-A"), job("explicit", "mp")],
      model,
      ["folder/explicit"],
    ).jobs.map((j) => j.name),
  ).toEqual(["E6751SM-A"]);
});
it("preserves other branch mappings and never changes source scope when associating or resetting", () => {
  const model = {
    branches: [branch],
    branch_jobs: { other: ["folder/other"] },
    release_lines: [],
  } as unknown as FocusModel;
  const mapped = withBranchJob(model, branch, "folder/E6751SM-A");
  expect(mapped.branch_jobs).toEqual({
    other: ["folder/other"],
    [branch]: ["folder/E6751SM-A"],
  });
  expect(mapped.branches).toBe(model.branches);
  expect(model.branch_jobs).toEqual({ other: ["folder/other"] });
  expect(withBranchJob(mapped, branch, null).branch_jobs).toEqual(
    model.branch_jobs,
  );
});

it("tracks A and B variants independently in the same model", () => {
  const jobs = [job("E6751SM-A"), job("E6751SM-B")];
  for (const variant of ["A", "B"]) {
    for (const layer of ["SYS_A16", "VND_A13"]) {
      const branch = `E6751SM-${variant}_${layer}_MP`;
      const candidate = buildAssociation(branch, "mp", jobs);
      expect(candidate.candidates.map((j) => j.name)).toEqual([
        `E6751SM-${variant}`,
      ]);
      const model = {
        branch_jobs: { [branch]: [`folder/E6751SM-${variant}`] },
      } as unknown as FocusModel;
      expect(
        buildAssociation(branch, "mp", jobs, model).jobs.map((j) => j.name),
      ).toEqual([`E6751SM-${variant}`]);
    }
  }
});

it("explicitly shares A source across A and B tasks and preserves missing targets", () => {
  const jobs = [job("E6751SM-A"), job("E6751SM-B")];
  const base = { branches: [branch], branch_jobs: {} } as unknown as FocusModel;
  const shared = withBranchJobs(
    base,
    branch,
    jobs.map((j) => j.job),
  );
  expect(buildAssociation(branch, "mp", jobs, shared).jobs).toEqual(jobs);
  expect(
    buildAssociation(branch, "mp", jobs.slice(0, 1), shared).missing,
  ).toEqual(["folder/E6751SM-B"]);
  expect(
    buildAssociation(branch, "mp", jobs, withBranchJobs(shared, branch, []))
      .jobs,
  ).toEqual([]);
  expect(shared.branch_jobs![branch]).toHaveLength(2);
});
it("a source switch for one target preserves a second target sharing the old source", () => {
  const a = job("E6751SM-A"),
    b = job("E6751SM-B");
  const model = {
    branch_jobs: { [branch]: [a.job, b.job] },
    job_sources: {
      [a.job]: { branch: "PARENT_SYS_MP", previous: [branch], note: "" },
    },
  } as unknown as FocusModel;
  expect(buildAssociation(branch, "mp", [a, b], model).jobs).toEqual([b]);
  model.branch_jobs = { PARENT_SYS_MP: [b.job] };
  expect(buildAssociation("PARENT_SYS_MP", "mp", [a, b], model).jobs).toEqual([
    a,
    b,
  ]);
});
