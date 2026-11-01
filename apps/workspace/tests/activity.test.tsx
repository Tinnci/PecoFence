// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
const mocks = vi.hoisted(() => ({ discover: vi.fn() }));
vi.mock("../src/api", () => mocks);
import ActivityPage, {
  level,
  monthLabels,
  thresholds,
  weeks,
} from "../src/Activity";

const day = (date: string, created: number, merged = 0) => ({
  date,
  created,
  merged,
});
afterEach(() => {
  cleanup();
});

describe("activity grid", () => {
  it("pads to Monday-first weeks and grades days at the quartiles of busy days", () => {
    // 2026-11-04 is a Wednesday: two padding cells, then the days.
    const columns = weeks([day("2026-11-04", 1), day("2026-11-05", 0)]);
    expect(columns[0].slice(0, 3).map((c) => c?.date ?? null)).toEqual([
      null,
      null,
      "2026-11-04",
    ]);
    const days = [1, 2, 3, 4, 5, 6, 7, 8].map((n, i) =>
      day(`2026-01-0${i + 1}`, n),
    );
    const cuts = thresholds(days);
    expect(cuts).toEqual([3, 5, 7]);
    expect([0, 1, 3, 4, 7, 8].map((n) => level(n, cuts))).toEqual([
      0, 1, 1, 2, 3, 4,
    ]);
  });

  it("drops a month label that would collide with the next one", () => {
    const cols = [
      "2026-10-26",
      "2026-11-02",
      "2026-11-09",
      "2026-11-16",
      "2026-11-23",
      "2026-11-30",
      "2026-12-07",
    ].map((d) => [day(d, 0)]);
    expect(monthLabels(cols)).toEqual(["", "11月", "", "", "", "", "12月"]);
  });

  it("finds a repository, shows its year and filters by branch", async () => {
    const activity = (branch?: string) => ({
      project: "packages/apps/DreamCamera2",
      branch,
      from: "2026-10-26",
      to: "2026-11-01",
      days: [
        day("2026-10-26", 3, 2),
        day("2026-10-27", 0),
        day("2026-11-01", branch ? 1 : 5, 4),
      ],
      total: branch ? 1 : 8,
      merged: branch ? 1 : 6,
      open: 1,
      abandoned: 1,
      branches: branch
        ? [{ branch, count: 1 }]
        : [
            { branch: "mtk_sys_sprocomm_OS_A16", count: 6 },
            { branch: "mtk_sys_OS3.3_A16", count: 2 },
          ],
    });
    mocks.discover.mockImplementation(
      async (list: string, args: { branch?: string }) =>
        list === "gerrit_projects"
          ? {
              gerrit_projects: [
                "alps/device/mediatek/common",
                "packages/apps/DreamCamera2",
              ],
            }
          : { activity: activity(args.branch) },
    );
    const user = userEvent.setup();
    render(<ActivityPage />);
    const input = await screen.findByPlaceholderText("搜索 2 个仓库");
    await user.type(input, "camera");
    await user.click(
      screen.getByRole("option", { name: "packages/apps/DreamCamera2" }),
    );
    const card = await screen.findByRole("region", { name: "变更分布" });
    await waitFor(() =>
      expect(card.textContent).toContain("8个变更 · 过去一年"),
    );
    expect(card.textContent).toContain("已合入6");
    expect(mocks.discover).toHaveBeenCalledWith("gerrit_activity", {
      repo: "packages/apps/DreamCamera2",
    });
    fireEvent.mouseEnter(card.querySelector('[title^="2026-11-01"]')!);
    expect(card.textContent).toContain("2026-11-01 · 5 个变更，已合入 4 个");
    await user.click(screen.getByRole("option", { name: /mtk_sys_OS3.3_A16/ }));
    await waitFor(() =>
      expect(card.textContent).toContain(
        "1个变更 · 过去一年 · mtk_sys_OS3.3_A16",
      ),
    );
    expect(mocks.discover).toHaveBeenLastCalledWith("gerrit_activity", {
      repo: "packages/apps/DreamCamera2",
      branch: "mtk_sys_OS3.3_A16",
    });
    // The repository's branches stay listed while one is shown.
    expect(
      screen.getByRole("option", { name: /mtk_sys_sprocomm_OS_A16/ }),
    ).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "全部分支" }));
    await waitFor(() => expect(card.textContent).toContain("8个变更"));
  });
});
