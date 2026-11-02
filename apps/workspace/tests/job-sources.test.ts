import { expect, it } from "vitest";
import {
  sourceBranches,
  sourceRoutes,
  sourceTimelineJobs,
  withJobSource,
} from "../src/job-sources";
import { buildAssociation } from "../src/build-association";
import type { FocusModel, JobView } from "../src/contracts/setup";
const old = "P6900DG_A16_IR",
  current = "P6900EVF_SYS_A16_IR",
  path = "folder/P6900DG_IR";
const job: JobView = {
  job: path,
  name: "P6900DG_IR",
  phase: "ir",
  disabled: false,
  parameters: {},
  changes: [],
  builds: [12, 11, 10, 9].map((number) => ({
    number,
    timestamp: number * 1000,
    duration: 1,
    estimated: 1,
    building: false,
  })),
};
const model = (since_build?: number): FocusModel => ({
  code: "P6900DG",
  branches: [old, current],
  jobs: [],
  issues: [],
  rules: {},
  targets: { severe_pct: 95, overall_pct: 85, fatal_max: 0 },
  accepted: [],
  tracked: [],
  branch_jobs: { [old]: [path] },
  job_sources: {
    [path]: {
      branch: current,
      previous: [old],
      since_build,
      note: "User record",
    },
  },
});
it("routes a renamed source independently from the Jenkins job name and stale local bindings", () => {
  expect(buildAssociation(old, "ir", [job], model(), [path])).toMatchObject({
    kind: "historical",
    jobs: [],
  });
  expect(buildAssociation(current, "ir", [job], model())).toMatchObject({
    kind: "source",
    jobs: [job],
  });
  expect(
    sourceRoutes(model(), old).historical.map((r) => r.source.branch),
  ).toEqual([current]);
});
it("never assigns all history to a new source when the cutover build is unknown", () => {
  expect(sourceTimelineJobs([job], current, model())[0].builds).toEqual([]);
  expect(sourceTimelineJobs([job], old, model())[0].builds).toEqual([]);
  expect(job.builds).toHaveLength(4);
});
it("partitions historical and current records at a known cutover without leaking to other branches", () => {
  expect(
    sourceTimelineJobs([job], current, model(10))[0].builds.map(
      (b) => b.number,
    ),
  ).toEqual([12, 11, 10]);
  expect(
    sourceTimelineJobs([job], old, model(10))[0].builds.map((b) => b.number),
  ).toEqual([9]);
  expect(sourceTimelineJobs([job], "other", model(10))[0].builds).toEqual([]);
  const ambiguous = model(10);
  ambiguous.job_sources![path].previous.push("older");
  expect(sourceTimelineJobs([job], old, ambiguous)[0].builds).toEqual([]);
});
it("adds recorded source branches to the follow scope while preserving existing branches", () => {
  const base = { ...model(), branches: [old], job_sources: {} };
  const updated = withJobSource(base, path, {
    branch: current,
    previous: [old, ""],
    note: "Local record",
  });
  expect(updated.branches).toEqual([old, current]);
  expect(sourceBranches(updated).job_sources![path].previous).toEqual([old]);
  expect(base.branches).toEqual([old]);
});
it("supports explicit historical ranges and repeated switches without filling unassigned gaps", () => {
  const assigned = model();
  assigned.job_sources![path].previous = [];
  assigned.job_sources![path].history = [
    { branch: old, first_build: 1, last_build: 5, note: "manual" },
    { branch: current, first_build: 6, last_build: 8, note: "manual" },
    { branch: old, first_build: 9, last_build: 10, note: "manual" },
  ];
  expect(
    sourceTimelineJobs([job], old, assigned)[0].builds.map((b) => b.number),
  ).toEqual([10, 9]);
  expect(sourceTimelineJobs([job], current, assigned)[0].builds).toEqual([]);
  expect(sourceRoutes(assigned, old).historical).toHaveLength(1);
  assigned.job_sources![path].history!.push({
    branch: current,
    first_build: 11,
    last_build: 12,
    note: "manual",
  });
  expect(
    sourceTimelineJobs([job], current, assigned)[0].builds.map((b) => b.number),
  ).toEqual([12, 11]);
  expect(sourceRoutes(assigned, current).historical).toEqual([]);
});
