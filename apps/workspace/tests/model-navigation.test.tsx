// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
vi.mock("../src/Model", () => ({ TabLine: () => <span /> }));
vi.mock("../src/focus-cache", () => ({ prefetchBuilds: vi.fn() }));
import { ModelNavigation, type ModelNavItem } from "../src/ModelNavigation";
const models: ModelNavItem[] = Array.from({ length: 30 }, (_, i) => ({
  code: `MODEL_${i}`,
  name: `很长的机型名称-${i}-Android16-系统维护项目`,
  short: false,
}));
const onModel = vi.fn();
function navigation(activeCode: string | null = null) {
  return (
    <ModelNavigation
      models={models}
      activeCode={activeCode}
      homeActive={!activeCode}
      onHome={vi.fn()}
      onModel={onModel}
      onWarm={vi.fn()}
      onCancelWarm={vi.fn()}
      actions={() => []}
    />
  );
}
beforeEach(() => {
  onModel.mockReset();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("finds an offscreen model by code and opens it with Enter", async () => {
  const user = userEvent.setup();
  render(navigation());
  await user.click(screen.getByRole("button", { name: "全部机型（30）" }));
  const dialog = screen.getByRole("dialog");
  await user.type(within(dialog).getByRole("textbox"), "model_29");
  expect(
    within(dialog)
      .getAllByRole("button")
      .some((b) => b.textContent?.includes(models[29].name)),
  ).toBe(true);
  await user.keyboard("{Enter}");
  expect(onModel).toHaveBeenCalledWith("MODEL_29");
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("shows the full name in the title and handles a search without matches", async () => {
  const user = userEvent.setup();
  render(navigation("MODEL_0"));
  expect(screen.getByRole("button", { name: models[0].name }).title).toBe(
    `${models[0].name}\nMODEL_0`,
  );
  await user.click(screen.getByRole("button", { name: "全部机型（30）" }));
  await user.type(
    within(screen.getByRole("dialog")).getByRole("textbox"),
    "no such model",
  );
  expect(screen.getByRole("status").textContent).toBe("没有匹配的机型");
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(onModel).not.toHaveBeenCalled();
});
it("scrolls only the model strip when overflowed and reveals a selected model", () => {
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(
    function (this: HTMLElement) {
      return this.classList.contains("model-tabs") ? 200 : 0;
    },
  );
  vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockImplementation(
    function (this: HTMLElement) {
      return this.classList.contains("model-tabs") ? 1200 : 0;
    },
  );
  const move = vi.fn(function (this: HTMLElement, options: ScrollToOptions) {
    this.scrollLeft += options.left ?? 0;
    this.dispatchEvent(new Event("scroll"));
  });
  Object.defineProperty(HTMLElement.prototype, "scrollBy", {
    configurable: true,
    value: move,
  });
  const { rerender } = render(navigation());
  expect(
    (screen.getByRole("button", { name: "向左滚动机型" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "向右滚动机型" }));
  expect(move).toHaveBeenCalledWith(expect.objectContaining({ left: 150 }));
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      const left = this.getAttribute("aria-current") === "page" ? 350 : 0;
      return {
        left,
        right: left + 200,
        top: 0,
        bottom: 48,
        width: 200,
        height: 48,
        x: left,
        y: 0,
        toJSON() {},
      };
    },
  );
  rerender(navigation("MODEL_29"));
  expect(move).toHaveBeenLastCalledWith({ left: 350, behavior: "auto" });
  expect(
    move.mock.instances.every((el) => el.classList.contains("model-tabs")),
  ).toBe(true);
  delete (HTMLElement.prototype as unknown as { scrollBy?: unknown }).scrollBy;
});
