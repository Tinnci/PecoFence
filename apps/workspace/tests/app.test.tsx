// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import type { HubReport } from "../src/contracts/hub";
const mocks = vi.hoisted(() => ({
  loadHub: vi.fn(),
  navigation: vi.fn(),
  desktop: vi.fn(),
  projectStatus: vi.fn(),
  planTemplate: vi.fn(),
  applyTemplate: vi.fn(),
  loadConnections: vi.fn(),
  signIn: vi.fn(),
}));
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
          id: "connect",
          title: "连接来源",
          why: "账号由各来源工具登录和保存",
          state: "unchecked",
          needs: [],
        },
        {
          id: "project",
          title: "选择项目",
          why: "从已连接账号能看到的范围中选择",
          state: "planned",
          needs: ["connect"],
        },
      ],
    },
  ],
};
beforeEach(() => {
  mocks.loadHub.mockReset().mockResolvedValue(report);
  mocks.navigation.mockReset().mockResolvedValue(() => {});
  mocks.desktop.mockReset().mockReturnValue(false);
  for (const fn of [
    mocks.projectStatus,
    mocks.planTemplate,
    mocks.applyTemplate,
    mocks.signIn,
  ])
    fn.mockReset();
});
beforeEach(() => {
  mocks.loadConnections.mockReset().mockResolvedValue({
    checked_at: "2026-11-01T00:00:00Z",
    connections: [],
  });
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
      await screen.findByRole("heading", { name: "连接来源" }),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: "选择项目" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /从配置文件导入/ })).toBeTruthy();
    expect(mocks.loadConnections).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "执行" })).toBeNull();
  });
  it("shows setup progress with the shadcn Base UI progress bar", async () => {
    const step = report.components[0].setup[0];
    mocks.loadHub.mockReset().mockResolvedValue({
      ...report,
      components: [
        {
          ...report.components[0],
          setup: [step, { ...step, id: "mode", state: "done" }],
        },
      ],
    });
    render(<App />);
    const bar = await screen.findByRole("progressbar", {
      name: "配置完成进度",
    });
    expect(bar.getAttribute("aria-valuenow")).toBe("50");
    const indicator = bar.querySelector<HTMLElement>(
      '[data-slot="progress-indicator"]',
    );
    expect(indicator?.style.width).toBe("50%");
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

describe("SPM template import", () => {
  const plan = {
    template: {
      id: "scope-a",
      version: "1.0",
      title: "Scope A",
      sha256: "ab12",
    },
    project: "PROJ",
    scope: "SCOPE",
    mode: "live",
    directory: "C:\\cfg\\PROJ\\SCOPE",
    files: [
      {
        role: "deployment",
        path: "C:\\cfg\\PROJ\\SCOPE\\deployment.json",
        action: "create",
      },
      {
        role: "live_config",
        path: "C:\\cfg\\PROJ\\SCOPE\\live.json",
        action: "conflict",
      },
    ],
    applicable: false,
    applied: false,
    backups: [],
  };
  const imported = {
    config_root: "C:\\cfg",
    projects: [
      {
        project: "PROJ",
        scope: "SCOPE",
        directory: "C:\\cfg\\PROJ\\SCOPE",
        check: { valid: true, ready: false, missing_environment: ["JIRA_PAT"] },
      },
    ],
  };
  async function openTemplateStep() {
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: /查看项目配置/ }),
    );
    await screen.findByRole("heading", { name: "选择项目" });
    await user.click(screen.getByRole("button", { name: /从配置文件导入/ }));
    return user;
  }
  it("plans, confirms a backup-and-replace, and takes the step state from the desktop status", async () => {
    mocks.desktop.mockReturnValue(true);
    mocks.projectStatus
      .mockResolvedValueOnce({ config_root: "C:\\cfg", projects: [] })
      .mockResolvedValueOnce(imported);
    mocks.planTemplate.mockResolvedValue(plan);
    mocks.applyTemplate.mockResolvedValue({
      ...plan,
      applied: true,
      backups: ["live.json.bak-1"],
      check: imported.projects[0].check,
    });
    const user = await openTemplateStep();
    const step = screen.getByRole("article", { name: "选择项目" });
    await waitFor(() => expect(step.textContent).toContain("需要处理"));
    await user.click(screen.getByRole("button", { name: /选择项目模板/ }));
    const planned = await screen.findByRole("region", { name: "导入计划" });
    expect(planned.textContent).toContain("PROJ");
    expect(planned.textContent).toContain("与模板不同");
    expect(mocks.applyTemplate).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "备份并替换" }));
    expect(mocks.applyTemplate).toHaveBeenCalledWith("ab12", true);
    await waitFor(() => expect(step.textContent).toContain("已完成"));
    expect(step.textContent).toContain("原文件已备份（1 个）");
    expect(step.textContent).toContain("JIRA_PAT");
    expect(screen.queryByRole("region", { name: "导入计划" })).toBeNull();
  });
  it("shows owner validation errors and stays quiet when the dialog is cancelled", async () => {
    mocks.desktop.mockReturnValue(true);
    mocks.projectStatus.mockResolvedValue({
      config_root: "C:\\cfg",
      projects: [],
    });
    mocks.planTemplate
      .mockRejectedValueOnce({ code: "template_cancelled" })
      .mockRejectedValueOnce({
        code: "template_invalid",
        message: "unknown field `x`",
      });
    const user = await openTemplateStep();
    await user.click(screen.getByRole("button", { name: /选择项目模板/ }));
    await waitFor(() => expect(mocks.planTemplate).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("alert")).toBeNull();
    await user.click(screen.getByRole("button", { name: /选择项目模板/ }));
    expect((await screen.findByRole("alert")).textContent).toBe(
      "模板无效：unknown field `x`",
    );
    expect(mocks.applyTemplate).not.toHaveBeenCalled();
  });
});

describe("connect sources", () => {
  const row = (name: string) => screen.getByRole("listitem", { name });
  async function openChecklist() {
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: /查看项目配置/ }),
    );
    await screen.findByRole("heading", { name: "连接来源" });
    return user;
  }
  it("shows each tool's login, signs in to Meegle through its own flow and completes the step", async () => {
    mocks.desktop.mockReturnValue(true);
    mocks.projectStatus.mockResolvedValue({
      config_root: "C:\\cfg",
      projects: [],
    });
    mocks.loadConnections.mockResolvedValue({
      checked_at: "2026-11-01T00:00:00Z",
      connections: [
        {
          source: "meegle",
          tool: "meegle",
          state: "signed_out",
          host: "project.feishu.cn",
          sign_in: true,
        },
        {
          source: "jira",
          tool: "jira-cli",
          state: "not_configured",
          sign_in: false,
        },
        {
          source: "gerrit",
          tool: "grr",
          state: "connected",
          account: "shitianchi",
          host: "gerrit.example",
          sign_in: false,
        },
        {
          source: "jenkins",
          tool: "jenkins-bridge",
          state: "unreachable",
          account: "work",
          code: "transport_error",
          sign_in: false,
        },
      ],
    });
    mocks.signIn.mockResolvedValue({
      checked_at: "2026-11-01T00:01:00Z",
      connections: [
        {
          source: "meegle",
          tool: "meegle",
          state: "connected",
          host: "project.feishu.cn",
          sign_in: true,
        },
      ],
    });
    const user = await openChecklist();
    const step = screen.getByRole("article", { name: "连接来源" });
    await waitFor(() => expect(step.textContent).toContain("需要处理"));
    expect(row("Jira").textContent).toContain("个人访问令牌文件");
    expect(row("Gerrit").textContent).toContain("shitianchi · gerrit.example");
    expect(row("Jenkins").textContent).toContain("网页数据转发");
    await user.click(screen.getByRole("button", { name: "登录" }));
    expect(mocks.signIn).toHaveBeenCalledWith("meegle");
    await waitFor(() =>
      expect(row("飞书项目").textContent).toContain("已连接"),
    );
    expect(step.textContent).toContain("已完成");
    expect(screen.queryByRole("button", { name: "登录" })).toBeNull();
  });
  it("explains a sign-in that needs the site chosen in a terminal", async () => {
    mocks.desktop.mockReturnValue(true);
    mocks.projectStatus.mockResolvedValue({
      config_root: "C:\\cfg",
      projects: [],
    });
    mocks.loadConnections.mockResolvedValue({
      checked_at: "2026-11-01T00:00:00Z",
      connections: [
        {
          source: "meegle",
          tool: "meegle",
          state: "signed_out",
          host: "project.feishu.cn",
          sign_in: true,
        },
      ],
    });
    mocks.signIn.mockRejectedValue({
      code: "sign_in_host_unknown",
      message: "run meegle auth login",
    });
    const user = await openChecklist();
    await user.click(await screen.findByRole("button", { name: "登录" }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "尚未选择飞书项目站点",
    );
  });
});
