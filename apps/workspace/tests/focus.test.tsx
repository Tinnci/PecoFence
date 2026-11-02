// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { FocusHome, FollowPage, ModelPage, lede, useFocus } from "../src/Focus";

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
    { branch: `${code}_SYS_A16_MP`, total: 3, days: Array(84).fill(0) },
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

beforeEach(() => {
  mocks.focus.mockReset().mockImplementation(async (action: string) => {
    if (action === "list")
      return {
        models: [
          { code: "E6767SM", targets },
          { code: "P6700LV", targets },
          { code: "E6751SM", targets },
        ],
      };
    if (action === "cached" || action === "summary" || action === "refresh")
      return {
        summary: {
          computed_at: "2026-11-01T10:40:00Z",
          models: [e6767, p6700, e6751],
        },
      };
    return {};
  });
});
afterEach(cleanup);

function Harness({ view }: { view: "home" | "model" | "follow" }) {
  const state = useFocus(true);
  const [opened, setOpened] = useState<string | null>(null);
  if (view === "home")
    return (
      <>
        <FocusHome state={state} onModel={setOpened} />
        <output>{opened}</output>
      </>
    );
  if (view === "model")
    return (
      <ModelPage
        state={state}
        code="E6767SM"
        onEdit={() => setOpened("edit")}
      />
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
    expect(list.textContent).toContain("冻结后进入 MP 的新功能");
    expect(list.textContent).toContain("关联 E6767SM-W-10370");
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
    const rules = screen.getByRole("region", { name: "规则" });
    expect(rules.textContent).toContain(
      "冻结后进入 MP 的新功能（10-12）提醒12",
    );
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
});
