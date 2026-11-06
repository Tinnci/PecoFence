// @vitest-environment jsdom
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { ModelOtaTab, nextPlan, OtaPage, otaKey } from "../src/Ota";
import { queryClient } from "../src/focus-cache";
import type {
  ModelOta,
  OtaItem,
  OtaPlan,
  OtaSource,
} from "../src/contracts/setup";

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

const source: OtaSource = {
  code: "E6765GDL",
  kind: "issues",
  id: "jira-0",
  label: "Jira",
  checked_at: "2026-11-06T06:32:00Z",
  updated_at: "2026-11-06T06:32:00Z",
  stale: false,
  error: null,
};

it("keeps the last OTA visible while reading and confirms source success", async () => {
  let finish!: (value: unknown) => void;
  mocks.focus.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<OtaPage models={[]} onModel={vi.fn()} />);
  expect(screen.getByRole("status").textContent).toContain("正在读取");
  expect(screen.getByText("3/5")).toBeTruthy();
  expect(
    screen
      .getByRole("button", { name: "重新读取来源" })
      .hasAttribute("disabled"),
  ).toBe(true);
  await act(async () =>
    finish({ ota: { models: [ota], errors: [], sources: [source] } }),
  );
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain("已更新"),
  );
  expect(
    screen
      .getByRole("button", { name: "重新读取来源" })
      .hasAttribute("disabled"),
  ).toBe(false);
});

it("shows partial source failures and clears the warning after a successful retry", async () => {
  const user = userEvent.setup();
  const failed = {
    ...source,
    kind: "requirements",
    stale: true,
    error: "Sign in again",
  };
  mocks.focus.mockResolvedValue({
    ota: { models: [ota], errors: [], sources: [source, failed] },
  });
  render(<OtaPage models={[]} onModel={vi.fn()} />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain("部分更新"),
  );
  expect(screen.getByRole("alert").textContent).toContain("沿用上次结果");
  expect(screen.getByRole("alert").textContent).toContain("Sign in again");
  mocks.focus.mockResolvedValue({
    ota: {
      models: [ota],
      errors: [],
      sources: [source, { ...failed, stale: false, error: null }],
    },
  });
  await user.click(screen.getByRole("button", { name: "重新读取来源" }));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(screen.getByRole("status").textContent).toContain("已更新");
});

it("shows request failures without losing cached items or their successful timestamp", async () => {
  queryClient.setQueryData(otaKey, {
    models: [ota],
    errors: [],
    sources: [source],
  });
  mocks.focus.mockRejectedValue({ code: "spm_timeout" });
  render(<ModelOtaTab code="E6765GDL" />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain(
      "读取失败，沿用上次结果",
    ),
  );
  expect(screen.getByRole("status").textContent).toContain("上次更新");
  expect(screen.getByText("E6765GDL-W-2")).toBeTruthy();
  expect(screen.getByRole("alert").textContent).toContain("未及时响应");
});

it("does not announce success or an empty portfolio for a malformed response", async () => {
  queryClient.clear();
  mocks.focus.mockResolvedValue({});
  render(<OtaPage models={[]} onModel={vi.fn()} />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain("未能读取来源"),
  );
  expect(screen.queryByText("还没有 OTA 项")).toBeNull();
  expect(screen.getByRole("alert")).toBeTruthy();
});

it("scopes source warnings to the open model", async () => {
  mocks.focus.mockResolvedValue({
    ota: {
      models: [ota],
      errors: [],
      sources: [
        source,
        { ...source, code: "OTHER", stale: true, error: "Other model offline" },
      ],
    },
  });
  render(<ModelOtaTab code="E6765GDL" />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain("已更新"),
  );
  expect(screen.queryByRole("alert")).toBeNull();
});

it("does not treat an older backend without source health as a successful update", async () => {
  render(<OtaPage models={[]} onModel={vi.fn()} />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain("尚未确认更新"),
  );
});
