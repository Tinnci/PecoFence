// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import type { HubReport } from "../src/contracts/hub";
const mocks = vi.hoisted(() => ({ loadHub: vi.fn(), navigation: vi.fn() }));
vi.mock("../src/api", () => mocks);
const report: HubReport = {
  schema_version: 1,
  observed_at: "2026-10-31T00:00:00Z",
  checked: false,
  unmanifested: [],
  components: [
    {
      id: "spm",
      name: "SPM",
      group: "delivery",
      summary: "Delivery",
      host: "both",
      lifecycle: "active",
      installed: true,
      issues: [],
      state: "setup_pending",
      setup: [
        {
          id: "template",
          title: "导入项目模板",
          why: "由项目负责人提供",
          state: "planned",
          needs: [],
        },
      ],
    },
  ],
};
beforeEach(() => {
  mocks.loadHub.mockReset().mockResolvedValue(report);
  mocks.navigation.mockReset().mockResolvedValue(() => {});
});
afterEach(cleanup);
describe("welcome to setup flow", () => {
  it("loads inventory without running checks and opens the real SPM checklist", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /查看项目配置/ });
    expect(mocks.loadHub.mock.calls).toEqual([[false]]);
    await user.click(screen.getByRole("button", { name: /查看项目配置/ }));
    expect(
      await screen.findByRole("heading", { name: "导入项目模板" }),
    ).toBeTruthy();
    expect(screen.getByText("配置入口将在后续版本提供")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "执行" })).toBeNull();
  });
  it("runs checks only on request and retains prior results when they fail", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /查看项目配置/ });
    mocks.loadHub.mockRejectedValueOnce(new Error("hub_timeout"));
    await user.click(screen.getByRole("button", { name: "检查状态" }));
    await screen.findByRole("alert");
    expect(mocks.loadHub.mock.calls).toEqual([[false], [true]]);
    expect(screen.getByText("下方保留上次读取结果")).toBeTruthy();
    expect(screen.getByRole("button", { name: /查看项目配置/ })).toBeTruthy();
    await waitFor(() =>
      expect(
        screen
          .getByRole("button", { name: "检查状态" })
          .hasAttribute("disabled"),
      ).toBe(false),
    );
  });
});
