// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", async (original) => ({
  ...(await original<typeof import("../src/api")>()),
  focus: mocks.focus,
}));
import { WorkItemsTab, ageDays, isStale } from "../src/WorkItems";
import { queryClient } from "../src/focus-cache";

const day = 86_400_000;
const iso = (daysAgo: number) =>
  new Date(Date.now() - daysAgo * day).toISOString();
const item = (id: string, group: string, extra = {}) => ({
  id,
  display_id: id,
  title: `title ${id}`,
  url: `http://jira.test/browse/${id}`,
  source: "jira",
  scope_id: "jira-0",
  status:
    group === "blocked" ? "Wait Info" : group === "done" ? "关闭" : "Open",
  group,
  resolution: "",
  owner: "沈霞",
  severity: "",
  updated: iso(1),
  software_evidence: "领域 = Setting",
  origin: "",
  origin_kind: "",
  origin_evidence: "",
  commits: [],
  ...extra,
});
const source = {
  id: "jira-0",
  source: "jira",
  label: "Jira PLV 中缺陷的",
  filter: "project = PLV AND issuetype = 缺陷",
  software_filter: "领域：软件类取值",
  total: 1314,
  matched: 1141,
  excluded: 173,
  unclassified: 0,
  updated_at: iso(0),
  stale: false,
  error: null,
  classification: {
    field: "customfield_17302",
    field_name: "领域",
    configured: false,
    values: [
      { value: "Setting", count: 363, software: true },
      { value: "硬件", count: 76, software: false },
    ],
  },
};
const saved = {
  code: "P6700LV",
  issues: [],
  branches: [],
  jobs: [],
  rules: {},
  targets: { severe_pct: 95, overall_pct: 85, fatal_max: 0 },
  accepted: [],
  tracked: [],
};
const state = { save: vi.fn(async () => {}) } as never as Parameters<
  typeof WorkItemsTab
>[0]["state"];

beforeEach(() => {
  queryClient.clear();
  (state as unknown as { save: ReturnType<typeof vi.fn> }).save.mockClear();
  mocks.focus.mockReset().mockImplementation(async (action: string) => {
    if (action.startsWith("issues"))
      return {
        work_items: {
          code: "P6700LV",
          kind: "issues",
          checked_at: iso(0),
          sources: [source],
          items: [
            item("PLV-1", "blocked", {
              updated: iso(20.5),
              commits: [80020, 79397],
            }),
            item("PLV-2", "testing"),
            item("PLV-3", "done"),
          ],
        },
      };
    return {
      work_items: {
        code: "P6700LV",
        kind: "requirements",
        checked_at: iso(0),
        sources: [
          {
            ...source,
            classification: undefined,
            software_filter: "issuetype = 软件需求",
          },
        ],
        items: [
          item("PLV-1507", "testing", {
            origin: "customer",
            origin_evidence: "标题含“客户需求”",
          }),
          item("PLV-1495", "pending", {
            origin: "internal",
            origin_kind: "internet",
            origin_evidence: "标题含“内部需求”",
          }),
          item("PLV-1488", "done", {
            origin: "internal",
            origin_evidence: "标题含“软件需求”",
          }),
        ],
      },
    };
  });
});
afterEach(cleanup);

describe("software issues and requirements", () => {
  it("shows open issues by stage, the scope and which domain values count", async () => {
    const user = userEvent.setup();
    render(
      <WorkItemsTab
        code="P6700LV"
        kind="issues"
        saved={saved as never}
        state={state}
      />,
    );
    expect(await screen.findByText("title PLV-1")).toBeTruthy();
    expect(screen.queryByText("title PLV-3")).toBeNull();
    expect(document.body.textContent).toContain(
      "范围 1314 · 软件 1141 · 排除 173",
    );
    expect(
      screen
        .getByRole("radio", { name: /未完成 2/ })
        .getAttribute("aria-checked"),
    ).toBe("true");
    expect(document.body.textContent).toContain("2 个提交");
    expect(document.body.textContent).toContain("20 天前更新");
    await user.click(screen.getByRole("switch", { name: /14 天没更新 1/ }));
    expect(screen.queryByText("title PLV-2")).toBeNull();
    await user.click(screen.getByRole("radio", { name: /全部 3/ }));
    await user.click(screen.getByRole("switch", { name: /14 天没更新/ }));
    expect(screen.getByText("title PLV-3")).toBeTruthy();
    await user.click(screen.getByText(/领域 怎样算软件/));
    await user.click(screen.getByRole("switch", { name: /硬件/ }));
    await user.click(
      screen.getByRole("button", { name: "保存分类并重新读取" }),
    );
    const model = (state as unknown as { save: ReturnType<typeof vi.fn> }).save
      .mock.calls[0][0];
    expect(model.tracking.jira_software_field).toBe("customfield_17302");
    expect(model.tracking.jira_software_values).toEqual(["Setting", "硬件"]);
  });

  it("splits requirements into customer and internal, and saves a correction", async () => {
    const user = userEvent.setup();
    render(
      <WorkItemsTab
        code="P6700LV"
        kind="requirements"
        saved={saved as never}
        state={state}
      />,
    );
    expect(await screen.findByText("title PLV-1507")).toBeTruthy();
    const customer = screen.getByRole("radio", { name: /客户需求/ });
    expect(customer.textContent).toContain("1共 1");
    await user.click(screen.getByRole("radio", { name: /其中互联网\/运营/ }));
    expect(screen.queryByText("title PLV-1507")).toBeNull();
    expect(screen.getByText("title PLV-1495")).toBeTruthy();
    expect(document.body.textContent).toContain("内部需求 · 互联网/运营");
    await user.click(screen.getByRole("button", { name: "改为客户" }));
    await waitFor(() =>
      expect(
        (state as unknown as { save: ReturnType<typeof vi.fn> }).save,
      ).toHaveBeenCalled(),
    );
    const model = (state as unknown as { save: ReturnType<typeof vi.fn> }).save
      .mock.calls[0][0];
    expect(model.tracking.origin_overrides).toEqual({ "PLV-1495": "customer" });
  });

  it("reads Jira and Meegle update times and marks open items untouched for two weeks", () => {
    const now = Date.parse("2026-11-02T12:00:00+08:00");
    expect(ageDays("2026-10-27T16:47:40.000+0800", now)).toBe(5);
    expect(ageDays(String(now - 3 * day), now)).toBe(3);
    expect(
      isStale(
        item("A", "pending", {
          updated: "2026-10-01T00:00:00.000+0800",
        }) as never,
        now,
      ),
    ).toBe(true);
    expect(
      isStale(
        item("B", "done", { updated: "2026-10-01T00:00:00.000+0800" }) as never,
        now,
      ),
    ).toBe(false);
  });
});
