// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", async (original) => ({
  ...(await original<typeof import("../src/api")>()),
  focus: mocks.focus,
}));
import {
  WorkItemsTab,
  WorkSummary,
  ageDays,
  isStale,
  remoteOf,
  shortVersion,
} from "../src/WorkItems";
import { queryClient } from "../src/focus-cache";

const day = 86_400_000;
const iso = (daysAgo: number) =>
  new Date(Date.now() - daysAgo * day).toISOString();
const activity = (extra = {}) => ({
  created: iso(30.5),
  status_since: iso(3.5),
  status_since_source: "history",
  last_comment: null,
  comment_count: 0,
  progress: "",
  found_versions: [],
  target_versions: [],
  reopened: 0,
  last_activity: iso(3.5),
  ...extra,
});
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
  updated: iso(0.5),
  software_evidence: "领域 = Setting",
  origin: "",
  origin_kind: "",
  origin_evidence: "",
  commits: [],
  activity: activity(),
  ...extra,
});
const source = {
  id: "jira-0",
  source: "jira",
  label: "Jira PLV 中缺陷的问题",
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
const save = vi.fn(async (_model: unknown) => {});
const state = { save } as never as Parameters<typeof WorkItemsTab>[0]["state"];
const issues = () => ({
  work_items: {
    code: "P6700LV",
    kind: "issues",
    checked_at: iso(0),
    sources: [source],
    items: [
      item("PLV-1", "blocked", {
        // Touched yesterday by a field edit, but no person has moved it in 20 days.
        updated: iso(1),
        commits: [80020, 79397],
        activity: activity({
          status_since: iso(20.5),
          last_activity: iso(20.5),
          reopened: 2,
          comment_count: 3,
          found_versions: ["V09"],
          target_versions: ["V12"],
          last_comment: {
            at: iso(20.5),
            by: "李志宏",
            text: "帮忙提供一下包名信息",
          },
        }),
        local: {
          tags: ["MP必解"],
          corrections: [
            {
              field: "found_version",
              value: "V09",
              remote: "V10",
              reason: "V09 已复现",
              at: iso(1),
              state: "active",
              remote_now: "V10",
            },
          ],
          notes: [{ at: iso(1), text: "已转测试" }],
          next_check: "2026-01-01",
        },
      }),
      item("PLV-2", "testing"),
      item("PLV-3", "done"),
    ],
  },
});

beforeEach(() => {
  queryClient.clear();
  save.mockClear();
  mocks.focus.mockReset().mockImplementation(async (action: string) => {
    if (action === "annotate") return { local: {} };
    if (action === "history")
      return {
        history: {
          local: null,
          events: [],
          transitions: [{ from: "OPEN", to: "回归测试", at: iso(1) }],
        },
      };
    if (action.startsWith("issues")) return issues();
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
            activity: activity({ progress: "等待 UI 稿" }),
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
  it("shows open issues by stage with their activity, versions and local marks", async () => {
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
    const text = document.body.textContent ?? "";
    expect(text).toContain("范围 1314 · 软件 1141 · 排除 173");
    expect(text).toContain("当前状态 20 天");
    expect(text).toContain("重开 2 次");
    expect(text).toContain("李志宏");
    expect(text).toContain("帮忙提供一下包名信息");
    expect(text).toContain("V09 → V12");
    expect(text).toContain("发现版本：V09V10本地修正");
    expect(text).toContain("MP必解");
    expect(text).toContain("01-01 复查");
    expect(text).toContain("2 个提交");
    // Stale follows the last human activity, not the field edit from yesterday.
    const look = screen.getByRole("group", { name: "需要关注" });
    expect(
      within(look)
        .getAllByRole("button")
        .map((b) => b.textContent),
    ).toEqual([
      "阻塞 1",
      "14 天无动态 1",
      "到期复查 1",
      "本地修正 1",
      "# MP必解 1",
    ]);
    const stale = within(look).getByRole("button", { name: "14 天无动态 1" });
    await user.click(stale);
    expect(stale.getAttribute("aria-pressed")).toBe("true");
    expect(screen.queryByText("title PLV-2")).toBeNull();
    await user.click(stale);
    expect(screen.getByText("title PLV-2")).toBeTruthy();
    await user.click(within(look).getByRole("button", { name: "# MP必解 1" }));
    expect(screen.queryByText("title PLV-2")).toBeNull();
    await user.click(screen.getByText(/领域 怎样算软件/));
    await user.click(screen.getByRole("switch", { name: /硬件/ }));
    await user.click(
      screen.getByRole("button", { name: "保存分类并重新读取" }),
    );
    const model = save.mock.calls[0][0] as never as {
      tracking: { jira_software_field: string; jira_software_values: string[] };
    };
    expect(model.tracking.jira_software_field).toBe("customfield_17302");
    expect(model.tracking.jira_software_values).toEqual(["Setting", "硬件"]);
  });

  it("tags, corrects and notes an item locally against the source's value", async () => {
    const user = userEvent.setup();
    render(
      <WorkItemsTab
        code="P6700LV"
        kind="issues"
        saved={saved as never}
        state={state}
      />,
    );
    await screen.findByText("title PLV-2");
    const row = screen.getByText("title PLV-1").closest("li")!;
    await user.click(within(row).getByRole("button", { name: "本地标记" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(
      within(dialog).getByRole("textbox", { name: "新标签" }),
      "客户关注",
    );
    await user.click(
      within(dialog).getAllByRole("button", { name: "添加" })[0],
    );
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("annotate", {
        source: "jira",
        item: "PLV-1",
        code: "P6700LV",
        op: { op: "tags", tags: ["MP必解", "客户关注"] },
      }),
    );
    expect(within(dialog).getByText("远端现在：V10")).toBeTruthy();
    await user.type(
      within(dialog).getByRole("textbox", { name: "修正为" }),
      "V08",
    );
    await user.type(
      within(dialog).getByRole("textbox", { name: "原因" }),
      "V08 已出现",
    );
    await user.click(within(dialog).getByRole("button", { name: "保存修正" }));
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("annotate", {
        source: "jira",
        item: "PLV-1",
        code: "P6700LV",
        op: {
          op: "correct",
          field: "found_version",
          value: "V08",
          remote: "V10",
          reason: "V08 已出现",
        },
      }),
    );
    await user.type(
      within(dialog).getByRole("textbox", { name: "记一笔" }),
      "周四复测",
    );
    await user.click(
      within(dialog).getAllByRole("button", { name: "添加" })[1],
    );
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("annotate", {
        source: "jira",
        item: "PLV-1",
        code: "P6700LV",
        op: { op: "note", text: "周四复测" },
      }),
    );
    fireEvent.change(
      within(dialog).getByLabelText("下次复查", { selector: "input" }),
      {
        target: { value: "2026-11-09" },
      },
    );
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("annotate", {
        source: "jira",
        item: "PLV-1",
        code: "P6700LV",
        op: { op: "next_check", date: "2026-11-09" },
      }),
    );
    expect(mocks.focus).toHaveBeenCalledWith("issues_cached", {
      code: "P6700LV",
    });
    expect(await within(dialog).findByText("OPEN → 回归测试")).toBeTruthy();
  });

  it("splits requirements into customer and internal, and corrects the origin locally", async () => {
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
    const origins = screen.getByRole("group", { name: "需求来源" });
    expect(
      within(origins).getByRole("button", { name: /客户需求/ }).textContent,
    ).toContain("1共 1");
    await user.click(
      within(origins).getByRole("button", { name: /其中互联网\/运营/ }),
    );
    expect(screen.queryByText("title PLV-1507")).toBeNull();
    expect(document.body.textContent).toContain("内部需求 · 互联网/运营");
    expect(document.body.textContent).toContain("进展等待 UI 稿");
    await user.click(screen.getByRole("button", { name: "本地标记" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("远端现在：内部需求")).toBeTruthy();
    await user.click(within(dialog).getByRole("combobox", { name: "修正为" }));
    await user.click(await screen.findByRole("option", { name: "客户需求" }));
    // SPM returns the item with its new layer; it replaces the old copy without a reread.
    const base = mocks.focus.getMockImplementation()!;
    const read = await base("requirements", { code: "P6700LV" });
    const shown = read.work_items.items.find(
      (i: { id: string }) => i.id === "PLV-1495",
    );
    mocks.focus.mockImplementation(async (action: string, args: unknown) =>
      action === "annotate"
        ? { local: {}, work_item: { kind: "requirements", item: shown } }
        : base(action, args),
    );
    const rereads = () =>
      mocks.focus.mock.calls.filter((c) => c[0] === "requirements_cached")
        .length;
    const before = rereads();
    await user.click(within(dialog).getByRole("button", { name: "保存修正" }));
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("annotate", {
        source: "jira",
        item: "PLV-1495",
        code: "P6700LV",
        op: {
          op: "correct",
          field: "origin",
          value: "customer",
          remote: "internal",
          reason: "",
        },
      }),
    );
    expect(rereads()).toBe(before);
    expect(save).not.toHaveBeenCalled();
  });

  it("dates activity from people, not edits, and keeps the source value behind a correction", () => {
    const now = Date.parse("2026-11-02T12:00:00+08:00");
    expect(ageDays("2026-10-27T16:47:40.000+0800", now)).toBe(5);
    expect(ageDays(String(now - 3 * day), now)).toBe(3);
    expect(ageDays("2026-10-30 12:00:00", now)).toBe(3);
    expect(ageDays("2026-10-30", now)).toBe(3);
    expect(ageDays("2026-10-30T12:00:00+08:00", now)).toBe(3);
    const old = activity({ last_activity: "2026-10-01T00:00:00.000+0800" });
    expect(
      isStale(
        item("A", "pending", { updated: iso(0), activity: old }) as never,
        now,
      ),
    ).toBe(true);
    expect(isStale(item("B", "done", { activity: old }) as never, now)).toBe(
      false,
    );
    const corrected = issues().work_items.items[0];
    expect(remoteOf(corrected as never, "found_version")).toBe("V10");
    expect(remoteOf(corrected as never, "owner")).toBe("沈霞");
  });

  it("opens 问题 from 概览 on what needs a look, and shortens build names", async () => {
    const user = userEvent.setup();
    const opened: string[] = [];
    render(
      <>
        <WorkSummary
          code="P6700LV"
          issues={issues().work_items as never}
          requirements={null}
          onOpen={(kind) => opened.push(kind)}
        />
      </>,
    );
    const card = screen.getByRole("region", { name: "问题与需求" });
    expect(card.textContent).toContain("问题2未完成");
    await user.click(
      within(card).getByRole("button", { name: "1 14 天无动态" }),
    );
    expect(opened).toEqual(["issues"]);
    cleanup();
    render(
      <WorkItemsTab
        code="P6700LV"
        kind="issues"
        saved={saved as never}
        state={state}
      />,
    );
    const stale = await screen.findByRole("button", { name: "14 天无动态 1" });
    expect(stale.getAttribute("aria-pressed")).toBe("true");
    expect(screen.queryByText("title PLV-2")).toBeNull();
    expect(shortVersion("LAVA_LXX516_SW_V_V14_HW_V1.0_20260923_user")).toBe(
      "V14 · 09-23",
    );
    expect(shortVersion("MAX_100_HW1_V4_2026101316_user")).toBe("V4 · 10-13");
    expect(shortVersion("helio_26_HW1_V5_20261022")).toBe("V5 · 10-22");
    expect(shortVersion("V09")).toBe("V09");
  });
});
