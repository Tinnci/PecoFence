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
import { useState } from "react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { FocusHome, FollowPage, lede, useFocus } from "../src/Focus";
import { ModelPage, jobsFor, phaseOf, progress, span } from "../src/Model";
import type { ModelTab } from "../src/api";

import { queryClient } from "../src/focus-cache";

const targets = { severe_pct: 95, overall_pct: 85, fatal_max: 0 };
const summary = (code: string, measures: number[], extra = {}) => ({
  code,
  targets,
  issues: measures[3],
  sources: [`飞书项目「问题管理」中关联项目为 ${code} 的`],
  measures: {
    severe_resolved: measures[0],
    severe_total: measures[1],
    overall_resolved: measures[2],
    overall_total: measures[3],
    fatal_open: measures[4],
  },
  new_issues: 0,
  days: Array(84).fill(0),
  days_from: "2026-08-10",
  week_changes: 0,
  week_merged: 0,
  branches: [
    {
      branch: `${code}_SYS_A16_MP`,
      phase: "mp",
      total: 3,
      days: Array(84).fill(0),
      flagged: 1,
    },
  ],
  flagged: [],
  rule_counts: { revert: 17, feature_after_freeze: 12 },
  rules: {
    revert: "alert",
    feature_after_freeze: "alert",
    customer_feedback: "track",
    other_model: "log",
    no_issue_id: "log",
    automation: "ignore",
  },
  freeze: "2026-10-12",
  errors: [],
  ...extra,
});
const e6767 = summary("E6767SM", [96, 100, 281, 342, 0], {
  new_issues: 24,
  week_changes: 13,
  week_merged: 12,
  flagged: [
    {
      number: 80020,
      url: "https://review.example.test/c/mobile/+/80020",
      subject: "[HM][E6767SM_VND_A13_MP][Version][Feature]:update WIFI Tx pwr",
      project: "alps/custom",
      branch: "E6767SM_VND_A13_MP",
      owner: "wangningshen",
      status: "NEW",
      created: "2026-10-30",
      rules: ["feature_after_freeze", "no_issue_id"],
      issues: [],
      tracked: false,
    },
    {
      number: 80153,
      subject:
        "[HM][E6767SM][Launcher3|E6767SM-W-10370][Bugfix]: 【V05客户反馈】",
      project: "Launcher3",
      branch: "E6767SM_SYS_A16_MP",
      owner: "wangfeifan",
      status: "MERGED",
      created: "2026-10-31",
      rules: ["customer_feedback"],
      issues: ["E6767SM-W-10370"],
      tracked: false,
    },
  ],
});
const p6700 = summary("P6700LV", [422, 618, 866, 1314, 3]);
const e6751 = summary("E6751SM", [111, 115, 297, 321, 0]);

const followed = (code: string, freeze?: string) => ({
  code,
  issues: [],
  branches: [`${code}_SYS_A16_MP`],
  rules: e6767.rules,
  ...(freeze ? { freeze } : {}),
  targets,
  accepted: [],
  tracked: [],
});

const hour = 3600000;
/** Visible text: NumberFlow carries its stylesheet inside the element. */
const text = (el: Element) => {
  const copy = el.cloneNode(true) as Element;
  copy.querySelectorAll("style").forEach((s) => s.remove());
  return copy.textContent ?? "";
};
const started = Date.now() - hour;
const run = (number: number, extra = {}) => ({
  number,
  result: "SUCCESS",
  url: `https://ci.example.test/job/Mobile/${number}/`,
  building: false,
  timestamp: started - (100 - number) * 24 * hour,
  duration: 3 * hour,
  estimated: 3 * hour,
  cause: "Started by timer",
  node: "fast_build_server-97",
  ...extra,
});
const builds = {
  code: "E6767SM",
  folder: "Project_Daily_Build/Project_E6767SM",
  computed_at: "2026-11-02T01:00:00Z",
  errors: [],
  jobs: [
    {
      job: "Project_Daily_Build/Project_E6767SM/E6767SM",
      name: "E6767SM",
      phase: "main",
      disabled: false,
      builds: [
        run(96, {
          result: undefined,
          building: true,
          timestamp: started,
          duration: 0,
        }),
        run(95),
      ],
      parameters: {
        BRANCH: "E6767SM_SYS_A16_MP",
        VARIANT: "user",
        SIGN: "<hidden>",
      },
      changes: [
        { build: 96, name: "VARIANT", before: "userdebug", after: "user" },
      ],
    },
    {
      job: "Project_Daily_Build/Project_E6767SM/E6767SM_IR",
      name: "E6767SM_IR",
      phase: "ir",
      disabled: false,
      builds: [run(17, { result: "FAILURE" })],
      parameters: { BRANCH: "E6767SM_SYS_A16_IR" },
      changes: [],
    },
    {
      job: "Project_Daily_Build/Project_E6767SM/E6767SM_old",
      name: "E6767SM_old",
      phase: "old",
      disabled: false,
      builds: [run(90)],
      parameters: {},
      changes: [],
    },
  ],
};
const listed = (number: number, state: string, rules: string[] = []) => ({
  number,
  subject: `change ${number}`,
  url: `https://review.example.test/c/mobile/+/${number}`,
  project: "alps/custom",
  branch: "E6767SM_SYS_A16_MP",
  owner: "dev",
  status: "MERGED",
  created: "2026-10-30",
  rules,
  issues: [],
  state,
});

beforeEach(() => {
  queryClient.clear();
  mocks.focus.mockReset().mockImplementation(async (action: string) => {
    if (action === "list")
      return {
        models: [
          followed("E6767SM", "2026-10-12"),
          followed("P6700LV"),
          followed("E6751SM"),
        ],
      };
    if (action === "cached" || action === "summary" || action === "refresh")
      return {
        summary: {
          computed_at: "2026-11-01T10:40:00Z",
          models: [e6767, p6700, e6751],
        },
      };
    if (action === "builds" || action === "builds_cached") return { builds };
    if (action === "changes")
      return {
        changes: [
          listed(3, "flagged", ["revert"]),
          listed(2, "logged", ["no_issue_id"]),
          listed(1, "clean"),
        ],
      };
    if (action === "preview")
      return {
        preview: {
          before: {
            flagged: 2,
            alerts: 1,
            rule_counts: { feature_after_freeze: 0 },
          },
          after: {
            flagged: 3,
            alerts: 2,
            rule_counts: { feature_after_freeze: 12 },
          },
          entering: [e6767.flagged[0]],
          leaving: [],
          changes: 214,
        },
      };
    return {};
  });
});
afterEach(async () => {
  cleanup();
  await queryClient.cancelQueries();
  queryClient.clear();
});

function Harness({
  view,
  code = "E6767SM",
  initialTab = "overview",
}: {
  view: "home" | "model" | "follow";
  code?: string;
  initialTab?: ModelTab;
}) {
  const state = useFocus(true);
  const [opened, setOpened] = useState<string | null>(null);
  const [tab, setTab] = useState<ModelTab>(initialTab);
  if (view === "home")
    return (
      <>
        <FocusHome state={state} onModel={setOpened} />
        <output>{opened}</output>
      </>
    );
  if (view === "model")
    return (
      <>
        <ModelPage
          state={state}
          code={code}
          tab={tab}
          onTab={setTab}
          onEdit={() => setOpened("edit")}
          onRemoved={() => setOpened("removed")}
        />
        <output>{opened}</output>
      </>
    );
  return <FollowPage state={state} onDone={setOpened} onCancel={() => {}} />;
}

describe("followed models", () => {
  it("orders the ledger by shortfall and says who is behind and who was busiest", async () => {
    expect(lede([e6767 as never, p6700 as never, e6751 as never])).toBe(
      "E6767SM 落后最多：整体还差 10 个。E6767SM 本周最忙，新增 24 个问题，合入 12 个变更。".replace(
        "E6767SM 落后最多：整体还差 10 个。",
        "P6700LV 落后最多：严重问题还差 166 个，3 个致命问题未解决。",
      ),
    );
    const user = userEvent.setup();
    render(<Harness view="home" />);
    await screen.findByText(/P6700LV 落后最多/);
    const rows = screen
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label"));
    expect(rows).toEqual(["P6700LV", "E6767SM", "E6751SM"]);
    const p = screen.getByRole("button", { name: "P6700LV" });
    expect(p.textContent).toContain("68.3%");
    expect(p.textContent).toContain("还差 166 个");
    expect(
      screen.getByRole("button", { name: "E6767SM" }).textContent,
    ).toContain("2 个需要留意");
    await user.click(screen.getByRole("button", { name: "E6751SM" }));
    expect(screen.getByText("E6751SM", { selector: "output" })).toBeTruthy();
  });

  it("accepts a flagged change and tracks another", async () => {
    const user = userEvent.setup();
    render(<Harness view="model" />);
    const list = await screen.findByRole("region", { name: "需要留意" });
    await waitFor(() =>
      expect(list.textContent).toContain("update WIFI Tx pwr"),
    );
    expect(
      within(list)
        .getByRole("link", { name: /update WIFI Tx pwr/ })
        .getAttribute("href"),
    ).toBe("https://review.example.test/c/mobile/+/80020");
    expect(list.textContent).toContain("冻结后进入 MP 的新功能");
    expect(list.textContent).toContain("关联 E6767SM-W-10370");
    expect(document.querySelector("button a, a button, a a")).toBeNull();
    const accepts = screen.getAllByRole("button", { name: "接受" });
    await user.click(accepts[0]);
    expect(list.textContent).not.toContain("update WIFI Tx pwr");
    expect(mocks.focus).toHaveBeenCalledWith("review", {
      code: "E6767SM",
      change: 80020,
      review: "accept",
    });
    await user.click(screen.getByRole("button", { name: "继续跟踪" }));
    expect(screen.getByRole("button", { name: "正在跟踪" })).toBeTruthy();
    expect(mocks.focus).toHaveBeenCalledWith("review", {
      code: "E6767SM",
      change: 80153,
      review: "track",
    });
    const side = screen.getByRole("region", { name: "构建" });
    await waitFor(() =>
      expect(side.textContent).toContain("#96 已运行 1 小时"),
    );
    expect(side.textContent).not.toContain("E6767SM_old");
    expect(screen.getByRole("tab", { name: /构建1 个构建中/ })).toBeTruthy();
    await user.click(screen.getByRole("tab", { name: /设置/ }));
    const rules = screen.getByRole("region", { name: "规则" });
    expect(rules.textContent).toMatch(/冻结后进入 MP 的新功能/);
    expect(
      (screen.getByLabelText("MP 冻结日期") as HTMLInputElement).value,
    ).toBe("2026-10-12");
  });

  it("shows each job's progress, history and current configuration", async () => {
    const user = userEvent.setup();
    render(<Harness view="model" initialTab="builds" />);
    const main = await screen.findByRole("article", { name: "E6767SM" });
    expect(main.textContent).toMatch(/#96 已运行 1 小时 · 预计 \d\d:\d\d 完成/);
    expect(
      main.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow"),
    ).toBe("33");
    expect(
      within(main).getByRole("link", { name: "构建日志" }).getAttribute("href"),
    ).toBe("https://ci.example.test/job/Mobile/96/console");
    expect(main.textContent).toContain("#96 改了 1 项");
    const variant = main.querySelector(".config-grid .changed")!;
    expect(variant.textContent).toBe("VARIANTuserdebuguser");
    expect(main.textContent).toContain("<hidden>");
    const ir = screen.getByRole("article", { name: "E6767SM_IR" });
    expect(ir.textContent).toContain("#17 失败");
    expect(screen.queryByRole("article", { name: "E6767SM_old" })).toBeNull();
    await user.click(
      screen.getByRole("button", { name: "旧的和已停用的任务 1 个" }),
    );
    expect(screen.getByRole("article", { name: "E6767SM_old" })).toBeTruthy();
  });

  it("lists a branch's changes by what the rules made of them and the jobs that build it", async () => {
    const user = userEvent.setup();
    render(<Harness view="model" initialTab="branches" />);
    const detail = await screen.findByRole("region", {
      name: "E6767SM_SYS_A16_MP",
    });
    await waitFor(() => expect(detail.textContent).toContain("change 3"));
    expect(
      within(detail)
        .getByRole("link", { name: "change 3" })
        .getAttribute("href"),
    ).toBe("https://review.example.test/c/mobile/+/3");
    expect(detail.textContent).not.toContain("change 2");
    await waitFor(() => expect(detail.textContent).toContain("#96 已运行"));
    const filter = screen.getByRole("radiogroup", { name: "筛选" });
    expect(filter.textContent).toContain("需要留意 1");
    await user.click(screen.getByRole("radio", { name: /全部/ }));
    expect(detail.textContent).toContain("change 2");
    expect(detail.textContent).toContain("只记录 · 没有问题单号");
    expect(mocks.focus).toHaveBeenCalledWith("changes", {
      code: "E6767SM",
      branch: "E6767SM_SYS_A16_MP",
    });
  });

  it("previews what new settings change before saving", async () => {
    render(<Harness view="model" initialTab="settings" />);
    const panel = await screen.findByRole("complementary", {
      name: "保存前后",
    });
    await waitFor(() => expect(text(panel)).toContain("需要留意2"));
    fireEvent.change(screen.getByLabelText("MP 冻结日期"), {
      target: { value: "2026-10-01" },
    });
    await waitFor(() => expect(text(panel)).toContain("保存后"));
    await waitFor(() => expect(text(panel)).toMatch(/需要留意2 → 3\+1/));
    expect(text(panel)).toMatch(/冻结后进入 MP 的新功能0 → 12\+12/);
    expect(text(panel)).toContain("新出现 1");
    expect(text(panel)).toContain("按上次读取的 214 个变更试算");
    await waitFor(() =>
      expect(
        mocks.focus.mock.calls.some(
          (c) => c[0] === "preview" && c[1].model.freeze === "2026-10-01",
        ),
      ).toBe(true),
    );
    expect(mocks.focus).not.toHaveBeenCalledWith("save", expect.anything());
  });

  it("says when the opened model is not followed", async () => {
    render(<Harness view="model" code="E6765SM" />);
    expect(await screen.findByText(/没有关注 E6765SM/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "关注 E6765SM" })).toBeTruthy();
  });

  it("persists a suffixless job association locally without refreshing sources or starting a build", async () => {
    const branch = "E6751SM-A_SYS_A16_MP";
    const model = summary("E6751SM", [1, 1, 1, 1, 0], {
      branches: [
        { branch, phase: "mp", total: 31, flagged: 1, days: Array(84).fill(0) },
      ],
    });
    let saved = {
      ...followed("E6751SM"),
      jobs: [],
      branches: [branch],
    } as import("../src/contracts/setup").FocusModel;
    const candidate = {
      ...builds.jobs[0],
      job: "folder/E6751SM-A",
      name: "E6751SM-A",
      phase: "main",
      parameters: {},
      builds: [run(57)],
    };
    mocks.focus.mockImplementation(async (action, args) => {
      if (action === "list") return { models: [saved] };
      if (["summary", "cached", "refresh"].includes(action))
        return { summary: { models: [model] } };
      if (["builds", "builds_cached"].includes(action))
        return { builds: { ...builds, code: "E6751SM", jobs: [candidate] } };
      if (action === "changes") return { changes: [] };
      if (action === "save") {
        saved = args.model;
        return { models: [saved] };
      }
      return {};
    });
    render(<Harness view="model" code="E6751SM" initialTab="branches" />);
    await screen.findByText("找到同型号候选任务，关联待确认");
    expect(screen.queryByText(/Jenkins #57/)).toBeNull();
    mocks.focus.mockClear();
    await userEvent.click(
      screen.getByRole("button", { name: "关联到当前分支" }),
    );
    await screen.findByText("本地配置关联", { exact: false });
    expect(saved.branch_jobs).toEqual({ [branch]: ["folder/E6751SM-A"] });
    expect(mocks.focus.mock.calls.map((c) => c[0])).toEqual(["save"]);
    expect(screen.getByText(/Jenkins #57/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "恢复自动识别" }));
    await screen.findByText("找到同型号候选任务，关联待确认");
    expect(saved.branch_jobs).toEqual({});
  });

  it("defers Jenkins reads while editing settings", async () => {
    render(<Harness view="model" initialTab="settings" />);
    await screen.findByRole("region", { name: "规则" });
    expect(
      mocks.focus.mock.calls.some(
        (call) => call[0] === "builds" || call[0] === "builds_cached",
      ),
    ).toBe(false);
  });

  it("reads phases and progress the way SPM does", () => {
    expect(phaseOf("E6751SM-A_SYS_A16_IR")).toBe("ir");
    expect(phaseOf("P6700LV_MP_VND_A16")).toBe("mp");
    expect(phaseOf("E6751SM-A_PB_old")).toBe("old");
    expect(phaseOf("P6700LV_a15")).toBe("main");
    expect(span(2 * hour + 43 * 60000)).toBe("2 小时 43 分");
    const late = progress(
      run(1, { building: true, timestamp: 0, estimated: hour }),
      2 * hour,
    );
    expect(late).toMatchObject({ pct: 99, late: true });
    const jobs = builds.jobs.map((j) => ({ ...j, parameters: {} }));
    expect(
      jobsFor("E6767SM_SYS_A16_IR", "ir", jobs as never).map((j) => j.name),
    ).toEqual(["E6767SM_IR"]);
    expect(jobsFor("E6765SM_E6767SM_VND_A13_IR", "ir", jobs as never)).toEqual(
      [],
    );
    expect(jobsFor("E6767SM_SYS_A16_MP", "mp", jobs as never)).toEqual([]);
  });

  it("sets a freeze date and targets in place and saves them together", async () => {
    const user = userEvent.setup();
    mocks.focus.mockImplementation(async (action: string) => {
      if (action === "list") return { models: [followed("P6700LV")] };
      if (action === "save") return { models: [followed("P6700LV")] };
      return { summary: { models: [p6700] } };
    });
    render(<Harness view="model" code="P6700LV" initialTab="settings" />);
    await screen.findByText("设置 MP 冻结日期后才会命中");
    expect(screen.getByText(/未设 MP 冻结日期/)).toBeTruthy();
    expect(screen.queryByText("有未保存的改动")).toBeNull();
    fireEvent.change(screen.getByLabelText("MP 冻结日期"), {
      target: { value: "2026-10-20" },
    });
    const severe = screen.getByRole("spinbutton", {
      name: "严重问题解决率门槛",
    });
    await user.clear(severe);
    await user.type(severe, "90");
    expect(screen.getByText("有未保存的改动")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("save", expect.anything()),
    );
    const saved = mocks.focus.mock.calls.find((c) => c[0] === "save")![1].model;
    expect(saved.freeze).toBe("2026-10-20");
    expect(saved.targets).toEqual({
      severe_pct: 90,
      overall_pct: 85,
      fatal_max: 0,
    });
    expect(saved.branches).toEqual(["P6700LV_SYS_A16_MP"]);
    expect(mocks.focus).toHaveBeenCalledWith("refresh", { code: "P6700LV" });
  });

  it("asks before it stops following a model", async () => {
    const user = userEvent.setup();
    render(<Harness view="model" initialTab="settings" />);
    await user.click(await screen.findByRole("button", { name: "停止关注" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("停止关注 E6767SM？");
    expect(mocks.focus).not.toHaveBeenCalledWith("remove", expect.anything());
    await user.click(
      screen.getAllByRole("button", { name: "停止关注" }).at(-1)!,
    );
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("remove", { code: "E6767SM" }),
    );
    expect(screen.getByText("removed", { selector: "output" })).toBeTruthy();
  });

  it("finds candidates, keeps the picked ones and saves the model with its rules", async () => {
    const meegle = (id: string, label: string) => ({
      source: "meegle",
      space: "efzuhc",
      type_key: "6942",
      work_item_type: "wentiguanli",
      type_name: "问题管理",
      filter: {
        field: "field_b86050",
        field_name: "关联项目",
        values: [{ id, label }],
      },
    });
    mocks.focus.mockImplementation(async (action: string) => {
      if (action === "list") return { models: [] };
      if (action === "suggest")
        return {
          suggestions: {
            code: "E6751SM",
            stem: "E6751",
            meegle: [
              {
                name: "E6751SM-A",
                count: 178,
                picked: true,
                scope: meegle("7057504519", "E6751SM-A"),
              },
              {
                name: "E6751SM-B",
                count: 143,
                picked: true,
                scope: meegle("7057314696", "E6751SM-B"),
              },
              {
                name: "E6751OB",
                count: 169,
                picked: false,
                scope: meegle("7112430906", "E6751OB"),
              },
            ],
            jira: [
              {
                name: "ESK1 · E6751SK",
                count: 752,
                picked: false,
                scope: { source: "jira", project: "ESK1" },
              },
            ],
            branches: [
              { name: "E6751SM-A_SYS_A16_IR", count: 76, picked: true },
              {
                name: "E6751SK_SYS_A16_MP_Softlock",
                count: 165,
                picked: false,
              },
            ],
            rule_counts: { revert: 7, no_issue_id: 68 },
          },
        };
      if (action === "save") return { models: [] };
      return { summary: { models: [] } };
    });
    const user = userEvent.setup();
    render(<Harness view="follow" />);
    await user.type(
      screen.getByRole("textbox", { name: "机型代号" }),
      "e6751sm{Enter}",
    );
    await screen.findByRole("checkbox", { name: /E6751SM-A178/ });
    expect(
      screen.getByText(
        /2 个问题来源，共 321 个问题 · 1 个分支 · 2 条规则会提醒你/,
      ),
    ).toBeTruthy();
    await user.click(
      screen.getByRole("checkbox", { name: /E6751SK_SYS_A16_MP_Softlock/ }),
    );
    const noId = screen.getByRole("radiogroup", { name: "没有问题单号" });
    await user.click(noId.querySelector('[aria-checked="false"]')!);
    await user.click(screen.getByRole("button", { name: "关注 E6751SM" }));
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("save", expect.anything()),
    );
    const saved = mocks.focus.mock.calls.find((c) => c[0] === "save")![1].model;
    expect(saved.issues).toHaveLength(1);
    expect(
      saved.issues[0].filter.values.map((v: { label: string }) => v.label),
    ).toEqual(["E6751SM-A", "E6751SM-B"]);
    expect(saved.branches).toEqual([
      "E6751SM-A_SYS_A16_IR",
      "E6751SK_SYS_A16_MP_Softlock",
    ]);
    expect(saved.rules.no_issue_id).toBe("alert");
    expect(mocks.focus).toHaveBeenCalledWith("refresh", { code: "E6751SM" });
  });

  it("moves between model pages with the arrow keys", async () => {
    const user = userEvent.setup();
    render(<Harness view="model" />);
    const overview = await screen.findByRole("tab", { name: /概览/ });
    expect(overview.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tablist")).toBeTruthy();
    overview.focus();
    await user.keyboard("{ArrowRight}");
    const issues = screen.getByRole("tab", { name: /问题/ });
    expect(document.activeElement).toBe(issues);
    await user.keyboard("{Enter}");
    await waitFor(() =>
      expect(issues.getAttribute("aria-selected")).toBe("true"),
    );
    expect(screen.getByRole("tabpanel")).toBeTruthy();
  });
});
