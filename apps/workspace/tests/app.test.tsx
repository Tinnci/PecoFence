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
  discover: vi.fn(),
  draft: vi.fn(),
  activate: vi.fn(),
  serviceStatus: vi.fn(),
  remap: vi.fn(),
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
    mocks.discover,
    mocks.draft,
    mocks.activate,
    mocks.remap,
  ])
    fn.mockReset();
});
beforeEach(() => {
  mocks.loadConnections.mockReset().mockResolvedValue({
    checked_at: "2026-11-01T00:00:00Z",
    connections: [],
  });
});
beforeEach(() => {
  mocks.serviceStatus
    .mockReset()
    .mockResolvedValue({
      mode: "historical",
      state: "historical",
      revision: 0,
      gates: [],
      unmapped: [],
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
    await user.click(screen.getByRole("button", { name: "系统" }));
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
    await user.click(screen.getByRole("button", { name: "系统" }));
    expect(screen.getByText("确认连接")).toBeTruthy();
    expect(screen.getByText(/网页转发服务无法连接/)).toBeTruthy();
  });
  it("navigates to the prerequisite component without executing setup", async () => {
    mocks.loadHub.mockResolvedValue(connected);
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: /查看项目配置/ });
    await user.click(screen.getByRole("button", { name: "系统" }));
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
    await user.click(screen.getByRole("button", { name: "系统" }));
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
        check: { valid: true, ready: true },
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
    expect(step.textContent).toContain("可以采集");
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

describe("new SPM project page", () => {
  const steps = [
    {
      id: "gates",
      title: "设置门槛",
      why: "门槛",
      state: "manual",
      needs: ["project"],
    },
    {
      id: "confirm",
      title: "确认并创建项目",
      why: "确认",
      state: "manual",
      needs: ["gates"],
    },
    {
      id: "start",
      title: "开始采集",
      why: "采集",
      state: "manual",
      needs: ["confirm"],
    },
  ];
  const statuses = [
    { value: "关闭", count: 242, mapped: "Closed" },
    { value: "OPEN", count: 32, mapped: "Open" },
    { value: "回归测试", count: 20, mapped: "Regression" },
  ];
  const draftOf = (
    count: number,
    questions: { field: string; value: string; count: number }[],
    extra: { value: string; count: number; mapped?: string }[] = [],
  ) => ({
    template: {
      id: "E6767SM-main",
      version: "2026-11-01",
      title: "E6767SM main",
      sha256: "f".repeat(64),
    },
    project: "E6767SM",
    scope: "main",
    directory: "C:\\cfg\\E6767SM\\main",
    files: [],
    applicable: true,
    applied: false,
    backups: [],
    summary: "",
    count,
    questions,
    mapping: {
      severity_from: "优先级",
      severities: {},
      stages: {},
      dispositions: {},
    },
    breakdown: {
      severities: [
        { value: "一般", count: 237, mapped: "Normal" },
        { value: "严重", count: 100, mapped: "Severe" },
        { value: "提示", count: 5, mapped: "Hint" },
      ],
      statuses: [...statuses, ...extra],
      resolutions: [],
      measures: {
        severe_total: 100,
        severe_resolved: 96,
        overall_total: 342,
        overall_resolved: 281,
        fatal_open: 0,
      },
    },
  });
  beforeEach(() => {
    mocks.desktop.mockReturnValue(true);
    mocks.loadHub.mockReset().mockResolvedValue({
      ...report,
      components: [
        {
          ...report.components[0],
          setup: [...report.components[0].setup, ...steps],
        },
      ],
    });
    mocks.projectStatus.mockResolvedValue({
      config_root: "C:\\cfg",
      projects: [],
    });
    mocks.loadConnections.mockResolvedValue({
      checked_at: "2026-11-01T00:00:00Z",
      connections: [
        { source: "meegle", tool: "meegle", state: "connected", sign_in: true },
      ],
    });
    mocks.discover.mockImplementation(
      async (list: string, args: { count_type?: string }) => {
        if (list === "spaces")
          return {
            spaces: [
              { project_key: "693f", simple_name: "efzuhc", name: "IPD" },
            ],
          };
        if (list === "types")
          return {
            types: [
              {
                type_key: "6942",
                api_name: "wentiguanli",
                name: "问题管理",
                issue_like: true,
              },
            ],
          };
        if (list === "groups")
          return {
            groups: [
              {
                field: "field_b86050",
                name: "关联项目",
                target_type: "693ff1c8",
                target_name: "项目管理",
              },
            ],
          };
        if (args.count_type)
          return {
            items: [
              { id: "7055355773", name: "E6765SM", count: 386 },
              { id: "7059897737", name: "E6767SM", count: 342 },
            ],
            without_issues: 60,
          };
        return { items: [{ id: "7141954099", name: "E676KLV" }] };
      },
    );
  });
  async function openPage() {
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: /查看项目配置/ }),
    );
    await user.click(await screen.findByRole("button", { name: /新建项目/ }));
    return user;
  }
  it("picks a counted project, judges gates on its issues and starts collection", async () => {
    mocks.draft
      .mockResolvedValueOnce(
        draftOf(342, [{ field: "stage", value: "Communication", count: 0 }]),
      )
      .mockResolvedValueOnce(
        draftOf(342, [{ field: "stage", value: "Communication", count: 0 }]),
      )
      .mockResolvedValueOnce({
        ...draftOf(342, []),
        applied: true,
        check: { valid: true, ready: true },
      });
    const health = {
      mode: "live",
      state: "running",
      project: "E6767SM",
      scope: "main",
      revision: 1,
      last_commit: "2026-11-01T10:00:00Z",
      gates: [
        { gate_id: "severe", state: "satisfied", n: 100, v: 96, gap: 0 },
        { gate_id: "overall", state: "satisfied", n: 342, v: 281, gap: 0 },
        { gate_id: "fatal", state: "satisfied", n: 0, v: 0, gap: 0 },
      ],
      unmapped: [],
    };
    mocks.activate.mockResolvedValue({ health, waited_secs: 20 });
    const user = await openPage();
    const list = await screen.findByRole("listbox", { name: "项目" });
    expect(list.textContent).toContain("E6765SM386");
    expect(mocks.discover).toHaveBeenCalledWith("items", {
      space: "efzuhc",
      kind: "693ff1c8",
      count_type: "6942",
      field: "field_b86050",
    });
    expect(screen.getByText(/2 个项目有问题，按问题数排列/)).toBeTruthy();
    await user.click(
      screen.getByRole("button", { name: "显示没有问题的 60 个项目" }),
    );
    expect(await screen.findByRole("option", { name: /E676KLV/ })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "只看有问题的" }));
    await user.click(await screen.findByRole("option", { name: /E6767SM/ }));
    const preview = screen.getByRole("complementary", { name: "范围预览" });
    await waitFor(() => expect(preview.textContent).toContain("342个问题"));
    expect(preview.textContent).toContain("严重100");
    expect(preview.textContent).toContain("已解决281");
    expect(mocks.draft.mock.calls[0][0].scope.filter).toEqual({
      field: "field_b86050",
      field_name: "关联项目",
      values: [{ id: "7059897737", label: "E6767SM" }],
    });

    await user.click(screen.getByRole("button", { name: /设置门槛/ }));
    const severe = screen.getByRole("article", { name: "严重问题解决率" });
    expect(severe.textContent).toContain("现在 96%（96 / 100）");
    expect(severe.textContent).toContain("达标");
    const overall = screen.getByRole("article", { name: "整体解决率" });
    expect(overall.textContent).toContain("还差 10 个");
    for (let i = 0; i < 3; i++)
      await user.click(screen.getByRole("button", { name: "降低整体解决率" }));
    expect(overall.textContent).toContain("达标");
    await user.click(screen.getByRole("button", { name: "MP" }));
    expect(screen.getByText(/阶段 MP。$/)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /确认/ }));
    expect(
      screen.getByText(
        /E6767SM · main 跟踪飞书项目「问题管理」中关联项目为 E6767SM 的 342 个问题；严重问题解决率不低于 95%，整体不低于 82%/,
      ),
    ).toBeTruthy();
    const table = screen.getByRole("region", { name: "状态对应" });
    expect(table.textContent).toContain("关闭242已关闭");
    expect(table.textContent).toContain("Communication 没有自动对应");
    await user.click(screen.getByRole("button", { name: "创建并开始采集" }));
    await screen.findByRole("heading", { name: "第一批数据已到" });
    const [, planned, applied] = mocks.draft.mock.calls;
    expect(planned[0]).toMatchObject({
      overall_target_pct: 82,
      phase: "MP",
      scope_complete: true,
    });
    expect(planned[1]).toBeUndefined();
    expect(applied[1]).toBe("f".repeat(64));
    expect(mocks.activate).toHaveBeenCalledWith("E6767SM", "main");
    expect(
      screen.getByRole("article", { name: "整体解决率" }).textContent,
    ).toContain("82.2%");
    expect(mocks.projectStatus).toHaveBeenCalledTimes(2);
  });
  it("merges projects picked with Ctrl into one scope named by their common prefix", async () => {
    mocks.draft.mockResolvedValue(draftOf(728, []));
    const user = await openPage();
    await user.click(await screen.findByRole("option", { name: /E6767SM/ }));
    await user.keyboard("{Control>}");
    await user.click(screen.getByRole("option", { name: /E6765SM/ }));
    await user.keyboard("{/Control}");
    await screen.findByText("已选 2 个，合并为一个范围");
    await waitFor(() => expect(mocks.draft).toHaveBeenCalledTimes(2));
    const merged = mocks.draft.mock.calls[1][0];
    expect(
      merged.scope.filter.values.map((v: { label: string }) => v.label),
    ).toEqual(["E6767SM", "E6765SM"]);
    expect(merged.project).toBe("E676");
    const listbox = screen.getByRole("listbox", { name: "项目" });
    expect(listbox.getAttribute("aria-multiselectable")).toBe("true");
    await user.click(screen.getByRole("option", { name: /E6765SM/ }));
    await waitFor(() =>
      expect(
        mocks.draft.mock.calls.at(-1)?.[0].scope.filter.values,
      ).toHaveLength(1),
    );
  });
  it("asks for statuses that appeared after creation and restarts collection", async () => {
    const running = {
      mode: "live",
      state: "running",
      project: "E6751SM",
      scope: "main",
      revision: 4,
      gates: [],
      unmapped: [
        { field: "stage", value: "Communication", count: 3 },
        { field: "stage", value: "To Be Confirmed", count: 0 },
      ],
    };
    mocks.serviceStatus.mockResolvedValue(running);
    mocks.remap.mockResolvedValue({});
    mocks.activate.mockResolvedValue({ health: { ...running, unmapped: [] } });
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: /查看项目配置/ }),
    );
    const start = await screen.findByRole("article", { name: "开始采集" });
    await waitFor(() =>
      expect(start.textContent).toContain(
        "E6751SM 有 3 个问题处于没有对应的状态",
      ),
    );
    expect(start.textContent).toContain("采集中：E6751SM · main");
    expect(start.textContent).not.toContain("To Be Confirmed");
    const save = screen.getByRole("button", { name: "保存并重新开始采集" });
    expect(save.hasAttribute("disabled")).toBe(true);
    await user.click(
      screen.getByRole("combobox", { name: "Communication 对应" }),
    );
    await user.click(await screen.findByRole("option", { name: "等待信息" }));
    await user.click(save);
    await waitFor(() =>
      expect(mocks.activate).toHaveBeenCalledWith("E6751SM", "main"),
    );
    expect(mocks.remap).toHaveBeenCalledWith("E6751SM", "main", {
      Communication: "WaitInfo",
    });
    await waitFor(() => expect(start.textContent).toContain("已完成"));
  });
  it("requires an answer only for unmapped statuses that issues hold, and asks again when the scope moved", async () => {
    const question = [{ field: "stage", value: "复核", count: 3 }];
    mocks.draft
      .mockResolvedValueOnce(
        draftOf(342, question, [{ value: "复核", count: 3 }]),
      )
      .mockResolvedValueOnce(draftOf(345, [], []));
    const user = await openPage();
    await user.click(await screen.findByRole("option", { name: /E6767SM/ }));
    await user.click(await screen.findByRole("button", { name: /设置门槛/ }));
    await user.click(screen.getByRole("button", { name: /确认/ }));
    const create = screen.getByRole("button", { name: "创建并开始采集" });
    expect(create.hasAttribute("disabled")).toBe(true);
    await user.click(screen.getByRole("combobox", { name: "复核 对应" }));
    await user.click(await screen.findByRole("option", { name: "审核中" }));
    expect(create.hasAttribute("disabled")).toBe(false);
    await user.click(create);
    await screen.findByText("范围已更新为 345 个问题，请再次确认。");
    expect(mocks.draft.mock.calls[1][0].answers.stages).toEqual({
      复核: "Reviewing",
    });
    expect(mocks.activate).not.toHaveBeenCalled();
  });
});
