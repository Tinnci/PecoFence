// @vitest-environment jsdom
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { ModelOtaTab, nextPlan, OtaPage, otaKey } from "../src/Ota";
import { queryClient } from "../src/focus-cache";
import type { ModelOta, OtaItem, OtaPlan } from "../src/contracts/setup";

const item: OtaItem = {
  id: "7001",
  display_id: "E6765GDL-W-1",
  source: "meegle",
  instance: "project.feishu.cn/x",
  kind: "issues",
  title: "Wi-Fi drops",
  url: "https://project.example/7001",
  status: "OPEN",
  owner: "Pat",
  severity: "",
  labels: ["OTA"],
  tags: ["keep"],
  stage: "tagged",
  must: true,
  customer: false,
  late: false,
  reason: "",
};
const plan: OtaPlan = {
  version: "OTA1",
  pre1: "2026-11-23",
  pre2: "",
  official: "2026-12-13",
  rollout: "2026-12-17",
  state: "upcoming",
  total: 5,
  merged: 3,
  verified: 1,
  late: 1,
  must_open: 2,
  items: [{ ...item, id: "7002", display_id: "E6765GDL-W-2", late: true }],
};
const ota: ModelOta = {
  code: "E6765GDL",
  plans: [
    {
      ...plan,
      version: "OTA0",
      state: "released",
      official: "2026-01-01",
      items: [],
    },
    plan,
  ],
  undecided: [item],
  left_out: [],
};
beforeEach(() => {
  queryClient.clear();
  mocks.focus.mockReset();
  mocks.focus.mockResolvedValue({ ota: { models: [ota], errors: [] } });
  queryClient.setQueryData(otaKey, { models: [ota], errors: [] });
});
afterEach(cleanup);

it("opens the coming OTA and folds what shipped", () => {
  render(<ModelOtaTab code="E6765GDL" />);
  expect(screen.getAllByText("OTA1").length).toBeGreaterThan(0);
  expect(screen.getByText("3/5")).toBeTruthy();
  expect(screen.getByText("E6765GDL-W-2")).toBeTruthy();
  expect(screen.getByText("1 项赶不上")).toBeTruthy();
  expect(screen.queryByText("OTA0")).toBeNull();
});

it("assigns an undecided item with a local tag", async () => {
  const user = userEvent.setup();
  render(<ModelOtaTab code="E6765GDL" />);
  await user.click(
    screen.getByRole("button", { name: "把 E6765GDL-W-1 指定到 OTA1" }),
  );
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("annotate", {
      source: "meegle",
      item: "7001",
      instance: "project.feishu.cn/x",
      code: "E6765GDL",
      op: { op: "tags", tags: ["keep", "OTA1"] },
    }),
  );
});

it("lists every model's coming OTA and opens it on the model", async () => {
  const onModel = vi.fn();
  const user = userEvent.setup();
  render(
    <OtaPage models={[{ code: "E6765GDL", name: "GDL" }]} onModel={onModel} />,
  );
  await user.click(screen.getByRole("button", { name: /^GDLOTA1/ }));
  expect(onModel).toHaveBeenCalledWith("E6765GDL", "ota");
});

it("names the next OTA", () => {
  expect(nextPlan(ota)?.version).toBe("OTA1");
  expect(nextPlan(undefined)).toBeUndefined();
});
