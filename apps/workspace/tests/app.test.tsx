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
      checkable: false,
      depends_on: [],
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

describe("component diagnosis and retry", () => {
  const connected: HubReport = {
    ...report,
    components: [
      report.components[0],
      {
        id: "jenkins-bridge",
        name: "Jenkins",
        group: "delivery",
        summary: "Build status",
        host: "wsl",
        lifecycle: "active",
        installed: true,
        checkable: true,
        state: "action_needed",
        depends_on: ["cdp-forwarder"],
        issues: [],
        checked_at: "2026-10-31T00:01:00Z",
        status: {
          state: "action_needed",
          code: "proxy_unavailable",
          exit_code: 1,
        },
        setup: [
          {
            id: "route",
            title: "访问构建服务",
            why: "需要网页连接",
            needs: ["cdp-forwarder"],
            state: "action_needed",
          },
        ],
      },
      {
        id: "cdp-forwarder",
        name: "网页数据转发",
        group: "delivery",
        summary: "Browser access",
        host: "windows",
        lifecycle: "active",
        installed: true,
        checkable: false,
        depends_on: [],
        state: "setup_pending",
        setup: [],
        issues: [],
      },
    ],
  };
  it("checks only the chosen component and preserves its retry context", async () => {
    mocks.loadHub.mockResolvedValueOnce(connected).mockResolvedValueOnce({
      ...connected,
      scope: "jenkins-bridge",
      checked: true,
      components: [
        {
          ...connected.components[1],
          state: "ready",
          checked_at: "2026-10-31T00:02:00Z",
          status: { state: "done", exit_code: 0 },
        },
      ],
    });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /查看项目配置/ });
    await user.click(screen.getByRole("button", { name: "运行状况" }));
    expect(screen.getByText(/网页转发服务无法连接/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "检查Jenkins" }));
    await screen.findByText("检查已通过。");
    expect(mocks.loadHub.mock.calls).toEqual([
      [false],
      [true, "jenkins-bridge"],
    ]);
    expect(screen.getByRole("heading", { name: "SPM" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "网页数据转发" })).toBeTruthy();
  });
  it("shows a failed setup check even when the component has no status command", async () => {
    const tool = {
      ...connected.components[1],
      status: undefined,
      setup: [
        {
          id: "connection",
          title: "确认连接",
          why: "Read access",
          needs: [],
          state: "action_needed",
          observation: {
            state: "action_needed",
            code: "proxy_unavailable",
            exit_code: 1,
          },
        },
      ],
    };
    mocks.loadHub.mockResolvedValue({ ...connected, components: [tool] });
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /查看项目配置/ });
    await user.click(screen.getByRole("button", { name: "运行状况" }));
    expect(screen.getByText("确认连接")).toBeTruthy();
    expect(screen.getByText(/网页转发服务无法连接/)).toBeTruthy();
  });
  it("navigates to the prerequisite component without executing setup", async () => {
    mocks.loadHub.mockResolvedValue(connected);
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /查看项目配置/ });
    await user.click(screen.getByRole("button", { name: "运行状况" }));
    await user.click(screen.getByRole("button", { name: "配置网页数据转发" }));
    expect(
      screen.getByRole("region", { name: "组件设置" }).textContent,
    ).toContain("网页数据转发");
    expect(mocks.loadHub.mock.calls).toEqual([[false]]);
  });
  it("keeps the last failure visible when an individual retry cannot run", async () => {
    mocks.loadHub
      .mockResolvedValueOnce(connected)
      .mockRejectedValueOnce(new Error("hub_timeout"));
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /查看项目配置/ });
    await user.click(screen.getByRole("button", { name: "运行状况" }));
    await user.click(screen.getByRole("button", { name: "检查Jenkins" }));
    await screen.findByRole("alert");
    expect(screen.getByText(/网页转发服务无法连接/)).toBeTruthy();
    mocks.loadHub.mockResolvedValueOnce({
      ...connected,
      scope: "jenkins-bridge",
      checked: true,
      components: [connected.components[1]],
    });
    await user.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() =>
      expect(mocks.loadHub.mock.calls).toEqual([
        [false],
        [true, "jenkins-bridge"],
        [true, "jenkins-bridge"],
      ]),
    );
  });
});

describe("first-launch prerequisite recovery", () => {
  it.each([
    ["wsl_missing", "尚未安装 WSL"],
    ["wsl_distribution_missing", "未找到可启动的 Linux 发行版"],
    ["wsl_distribution_unavailable", "默认 Linux 发行版无法启动"],
    ["cli_missing", "尚未找到可运行的 dev-cli"],
    ["hub_update_required", "需要更新 dev-cli"],
    ["hub_invalid_response", "工具清单返回的数据无效"],
    ["hub_timeout", "读取工具状态超时"],
  ])(
    "explains %s and recovers to inventory without checks",
    async (code, title) => {
      mocks.loadHub.mockRejectedValueOnce({
        code,
        observed_at_ms: 1793440000000,
      });
      const user = userEvent.setup();
      render(<App />);
      await screen.findByRole("heading", { name: title });
      let finish: (value: HubReport) => void = () => {};
      mocks.loadHub.mockImplementationOnce(
        () =>
          new Promise<HubReport>((resolve) => {
            finish = resolve;
          }),
      );
      await user.click(screen.getByRole("button", { name: "重试" }));
      expect(screen.getByRole("heading", { name: title })).toBeTruthy();
      expect(
        screen
          .getByRole("button", { name: "正在重试…" })
          .hasAttribute("disabled"),
      ).toBe(true);
      finish(report);
      await screen.findByRole("button", { name: /查看项目配置/ });
      expect(screen.queryByRole("alert")).toBeNull();
      expect(mocks.loadHub.mock.calls).toEqual([[false], [false]]);
    },
  );
  it("does not render raw process errors", async () => {
    mocks.loadHub.mockRejectedValueOnce({
      code: "private /home/user/token=secret",
      observed_at_ms: Infinity,
    });
    render(<App />);
    await screen.findByRole("heading", { name: "无法读取本机工具状态" });
    expect(document.body.textContent).not.toContain("token=secret");
  });
});
