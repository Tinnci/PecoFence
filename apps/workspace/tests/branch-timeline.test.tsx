// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { branchTimeline, branchInputMatches } from "../src/branch-timeline";
import { BuildMarker } from "../src/BuildMarker";
import { queryClient } from "../src/focus-cache";
import { ContextMenuProvider } from "../src/components/context-menu";
import type { BuildRun, JobView, Listed } from "../src/contracts/setup";
const run: BuildRun = {
  number: 23,
  timestamp: 2000,
  duration: 500,
  estimated: 0,
  building: false,
  result: "SUCCESS",
};
const job: JobView = {
  job: "folder/model",
  name: "model",
  phase: "mp",
  disabled: false,
  builds: [run],
  parameters: {},
  changes: [],
};
const change = (number: number, created_at?: number): Listed => ({
  number,
  created_at,
  created: "2026-10-24",
  subject: "change",
  project: "p",
  branch: "main",
  owner: "dev",
  status: "MERGED",
  rules: [],
  issues: [],
  state: "clean",
});
afterEach(() => {
  cleanup();
  queryClient.clear();
  mocks.focus.mockReset();
});
it("interleaves exact times and keeps date-only cache entries at the end", () => {
  const events = branchTimeline(
    [change(1, 1000), change(2, 3000), change(3), change(4, NaN)],
    [job],
  );
  expect(events.map((e) => e.key)).toEqual([
    "change:2",
    "build:folder/model:23",
    "change:1",
    "change:3",
    "change:4",
  ]);
  expect(events[3].time).toBeNull();
  expect(
    branchTimeline([], [{ ...job, builds: [{ ...run, timestamp: 0 }] }]),
  ).toEqual([]);
});
it("uses each run's explicit branch input without treating BUILD_TAG as a ref", () => {
  expect(
    branchInputMatches(
      { ...run, source_parameters: { BRANCH_T: "refs/heads/main" } },
      "main",
    ),
  ).toBe(true);
  expect(
    branchInputMatches(
      { ...run, source_parameters: { BUILD_TAG: "main" } },
      "main",
    ),
  ).toBe(false);
  expect(branchInputMatches(run, "main")).toBe(false);
});
it("reads console evidence only on request, distinguishes software number and reuses query cache", async () => {
  mocks.focus.mockResolvedValue({
    build_evidence: {
      job: job.job,
      number: 23,
      software_number: "14",
      system_manifest: "MODEL_system_14_date.xml",
      vendor_skipped: true,
      windows: [{ start: 0, bytes: 1024 }],
      archive_commit: "123abcd",
    },
  });
  const props = { code: "MODEL", branch: "main", job, run };
  const view = render(
    <ContextMenuProvider>
      <ol>
        <BuildMarker {...props} />
      </ol>
    </ContextMenuProvider>,
  );
  expect(mocks.focus).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "检查源码线索" }));
  expect(await screen.findByText("MODEL_system_14_date.xml")).toBeTruthy();
  expect(screen.getByText("软件版本号 14")).toBeTruthy();
  expect(screen.getByText(/Jenkins #23/)).toBeTruthy();
  expect(mocks.focus).toHaveBeenCalledWith("build_evidence", {
    code: "MODEL",
    job: job.job,
    build: 23,
  });
  view.unmount();
  render(
    <ContextMenuProvider>
      <ol>
        <BuildMarker {...props} />
      </ol>
    </ContextMenuProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "检查源码线索" }));
  expect(screen.getByText("MODEL_system_14_date.xml")).toBeTruthy();
  expect(mocks.focus).toHaveBeenCalledTimes(1);
});
