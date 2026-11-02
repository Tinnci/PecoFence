// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import {
  releaseBranchGroups,
  releaseJobs,
  releaseJobGroups,
} from "../src/release-lines";
import { ReleaseLinesEditor } from "../src/ReleaseLinesEditor";
import { branchQueryBrowserUrl } from "../src/browser-links";
import type { FocusModel, JobView, ModelSummary } from "../src/contracts/setup";
const saved: FocusModel = {
  code: "MODEL",
  branches: ["MODEL_A15_MP", "MODEL_a13", "MODEL_a15", "MODEL_MP_VND_A16"],
  jobs: [],
  issues: [],
  rules: {},
  accepted: [],
  tracked: [],
  targets: { severe_pct: 95, overall_pct: 85, fatal_max: 0 },
  ignored_jobs: ["folder/MODEL_A16_PB"],
  release_lines: [
    {
      id: "a15",
      label: "Android 15",
      phase: "mp",
      lifecycle: "maintenance",
      branches: ["MODEL_A15_MP", "MODEL_a13", "MODEL_a15"],
      jobs: ["folder/MODEL_MP"],
    },
    {
      id: "a16",
      label: "Android 16",
      phase: "mp",
      lifecycle: "upgrade",
      branches: ["MODEL_MP_VND_A16"],
      jobs: ["folder/MODEL_A16"],
    },
  ],
};
const summary = {
  code: "MODEL",
  branches: saved.branches.map((branch) => ({
    branch,
    phase: "main",
    days: [],
    total: 1,
    flagged: 0,
  })),
} as unknown as ModelSummary;
const jobs: JobView[] = ["MODEL_MP", "MODEL_A16", "MODEL_A16_PB"].map(
  (name) => ({
    job: `folder/${name}`,
    name,
    phase: name.endsWith("MP") ? "mp" : name.endsWith("PB") ? "pb" : "main",
    disabled: false,
    parameters: {},
    builds: [],
    changes: [],
  }),
);
afterEach(cleanup);
it("keeps A13 vendor inputs in the A15 release line, A16 as upgrade MP and PB excluded", () => {
  const groups = releaseBranchGroups(summary, saved);
  expect(
    groups.map((g) => [g.label, g.phase, g.lifecycle, g.branches.length]),
  ).toEqual([
    ["Android 15", "mp", "maintenance", 3],
    ["Android 16", "mp", "upgrade", 1],
  ]);
  const active = releaseJobs(jobs, saved);
  expect(active.map((j) => [j.name, j.phase])).toEqual([
    ["MODEL_MP", "mp"],
    ["MODEL_A16", "mp"],
  ]);
  expect(releaseJobGroups(active, saved).map((g) => g.label)).toEqual([
    "Android 15",
    "Android 16",
  ]);
  expect(jobs[1].phase).toBe("main");
});
it("keeps unassigned and shared source branches reachable", () => {
  const model = {
    ...saved,
    branches: [...saved.branches, "shared"],
    release_lines: saved.release_lines!.map((l) => ({
      ...l,
      branches: [...l.branches, "shared"],
    })),
  };
  expect(
    releaseBranchGroups(summary, model).filter((g) =>
      g.branches.some((b) => b.branch === "shared"),
    ),
  ).toHaveLength(2);
  expect(releaseBranchGroups(summary).map((g) => g.id)).toEqual(["phase:main"]);
});
it("edits lifecycle separately from phase and can exclude a task without deleting branches", () => {
  const onChange = vi.fn();
  render(
    <ReleaseLinesEditor
      model={saved}
      lines={saved.release_lines!}
      ignored={saved.ignored_jobs!}
      jobs={jobs}
      onChange={onChange}
    />,
  );
  fireEvent.change(screen.getAllByLabelText("生命周期")[1], {
    target: { value: "maintenance" },
  });
  expect(onChange.mock.lastCall![0][1]).toMatchObject({
    phase: "mp",
    lifecycle: "maintenance",
  });
  fireEvent.click(
    screen
      .getByRole("group", { name: "排除的临时构建任务" })
      .querySelectorAll("input")[0],
  );
  expect(onChange.mock.lastCall![1]).toContain("folder/MODEL_MP");
  expect(onChange.mock.lastCall![0][0].branches).toEqual(
    saved.release_lines![0].branches,
  );
  expect(onChange.mock.lastCall![0][0].jobs).toEqual([]);
});
it("links the engineer tool on the existing Jenkins context without accepting executable URLs", () => {
  expect(
    branchQueryBrowserUrl({
      url: "https://ci.test/jenkins/job/folder/job/model/23/",
    }),
  ).toBe(
    "https://ci.test/jenkins/job/SCM-Server/job/Engineer-tools/job/Branch_query_tool/",
  );
  expect(branchQueryBrowserUrl({ url: "javascript:alert(1)" })).toBeUndefined();
});

import { manifestInputs } from "../src/ManifestInputs";
it("keeps T/V manifest refs distinct without guessing SYS/VND roles or source revisions", () => {
  const inputs = manifestInputs({
    ...jobs[0],
    parameters: {
      REPO_URL_T: " https://review.test/platform/cus-manifest ",
      BRANCH_T: "vendor/android13",
      REPO_URL_V: "https://review.test/platform/cus-manifest",
      BRANCH_V: "system/android16",
    },
  });
  expect(inputs).toEqual([
    {
      slot: "T",
      repository: "https://review.test/platform/cus-manifest",
      ref: "vendor/android13",
    },
    {
      slot: "V",
      repository: "https://review.test/platform/cus-manifest",
      ref: "system/android16",
    },
  ]);
  expect(manifestInputs(jobs[0])).toEqual([]);
});
