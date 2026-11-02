import { expect, it } from "vitest";
import { buildAssociation, withBranchJob } from "../src/build-association";
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
