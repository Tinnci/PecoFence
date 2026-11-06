// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
vi.mock("../src/Model", () => ({ TabLine: () => <span /> }));
vi.mock("../src/focus-cache", () => ({ prefetchBuilds: vi.fn() }));
import type { Page } from "../src/api";
import { isSystem, Rail } from "../src/Rail";

const onPage = vi.fn();
const onModel = vi.fn();
const onFollow = vi.fn();
const remove = vi.fn();
function rail(page: Page = "home", activeCode: string | null = null) {
  return (
    <Rail
      models={Array.from({ length: 30 }, (_, i) => ({
        code: `MODEL_${i}`,
        name: `机型-${i}`,
        short: false,
      }))}
      page={page}
      activeCode={activeCode}
      onPage={onPage}
      onModel={onModel}
      onFollow={onFollow}
      onSettings={vi.fn()}
      onWarm={vi.fn()}
      onCancelWarm={vi.fn()}
      actions={() => [{ id: "remove", label: "取消关注", run: remove }]}
    />
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
});
afterEach(cleanup);

it("goes to each area and marks where you are", async () => {
  const user = userEvent.setup();
  render(rail("traces"));
  expect(
    screen.getByRole("button", { name: "系统" }).getAttribute("aria-current"),
  ).toBe("page");
  await user.click(screen.getByRole("button", { name: "我的工作" }));
  expect(onPage).toHaveBeenCalledWith("mine");
  await user.click(screen.getByRole("button", { name: "机型-29" }));
  expect(onModel).toHaveBeenCalledWith("MODEL_29");
  await user.click(screen.getByRole("button", { name: "关注机型" }));
  expect(onFollow).toHaveBeenCalled();
});

it("reaches a model's actions without the right mouse button", async () => {
  const user = userEvent.setup();
  render(rail());
  await user.click(screen.getByRole("button", { name: "机型-3 的操作" }));
  await user.click(await screen.findByRole("menuitem", { name: "取消关注" }));
  expect(remove).toHaveBeenCalled();
});

it("folds, and stays folded next time", async () => {
  const user = userEvent.setup();
  const { unmount } = render(rail());
  await user.click(screen.getByRole("button", { name: "收起侧栏" }));
  expect(screen.queryByText("我的工作")).toBeNull();
  expect(screen.getByRole("button", { name: "我的工作" })).toBeTruthy();
  unmount();
  render(rail());
  expect(screen.getByRole("button", { name: "展开侧栏" })).toBeTruthy();
});

it("counts the project wizard as part of System", () => {
  expect(isSystem("project")).toBe(true);
  expect(isSystem("logs")).toBe(true);
  expect(isSystem("mine")).toBe(false);
});
