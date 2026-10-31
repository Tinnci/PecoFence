import type { Component, HubReport } from "./contracts/hub";

export const groups = [
  ["delivery", "交付与构建"],
  ["people", "沟通与人员"],
  ["output", "报告与产出"],
  ["desktop", "桌面与工具"],
] as const;
export const stateLabels: Record<string, string> = {
  ready: "已就绪",
  installed: "已安装",
  not_installed: "未安装",
  setup_pending: "待配置",
  unchecked: "未检查",
  unknown: "状态未知",
  action_needed: "需要处理",
  invalid: "配置有误",
  paused: "已暂停",
  planned: "待支持",
  manual: "需手动配置",
  done: "已完成",
};
export function label(state: string) {
  return stateLabels[state] ?? "状态未知";
}
export function progress(report: HubReport) {
  const steps = report.components
    .filter((c) => c.lifecycle !== "paused")
    .flatMap((c) => c.setup);
  return {
    done: steps.filter((s) => s.state === "done").length,
    total: steps.length,
  };
}
export function attention(component: Component) {
  return [
    "action_needed",
    "invalid",
    "not_installed",
    "setup_pending",
  ].includes(component.state);
}

export function mergeReport(
  current: HubReport | null,
  incoming: HubReport,
): HubReport {
  if (!incoming.scope) return incoming;
  if (
    !current ||
    incoming.components.length !== 1 ||
    incoming.components[0].id !== incoming.scope ||
    !current.components.some((c) => c.id === incoming.scope)
  ) {
    throw new Error("hub_invalid_scope");
  }
  return {
    ...current,
    components: current.components.map((c) =>
      c.id === incoming.scope ? incoming.components[0] : c,
    ),
  };
}

export function checkExplanation(
  observation: import("./contracts/hub").Observation,
): string {
  if (observation.reason === "check_timeout")
    return "检查超时，暂时无法确认状态。稍后可重试此组件。";
  if (observation.reason === "check_unavailable")
    return "检查程序无法运行。请确认组件已安装，再重试。";
  if (observation.reason === "inconsistent_result")
    return "检查返回了不一致的结果。请更新组件后重试。";
  const code = observation.code?.toLowerCase();
  if (code === "proxy_unavailable")
    return "网页转发服务无法连接。先检查网页数据转发，再重试。";
  if (code === "transport_error")
    return "工具报告连接失败。请检查本机客户端及其连接。";
  if (
    code &&
    /^(auth|unauthorized|forbidden|invalid_token|credentials)/.test(code)
  )
    return "请确认登录、凭据和访问权限，再查看此组件的配置要求。";
  if (observation.state === "done") return "检查已通过。";
  if (observation.state === "action_needed")
    return "组件检查未通过，请查看配置清单和依赖组件。";
  return "尚未确认组件状态，可重试检查。";
}

/**
 * The hub checks SPM from WSL, which reads another configuration root and cannot
 * read files Windows encrypts; the desktop's own `spm setup status` decides the
 * template step instead.
 */
export function withProjects(
  report: HubReport,
  status: import("./contracts/setup").ProjectStatus | null,
): HubReport {
  if (!status) return report;
  const imported = status.projects.some((p) => p.check.valid);
  return {
    ...report,
    components: report.components.map((c) =>
      c.id !== "spm"
        ? c
        : {
            ...c,
            setup: c.setup.map((s) =>
              s.id === "template"
                ? {
                    ...s,
                    state: imported ? "done" : "action_needed",
                    observation: undefined,
                  }
                : s,
            ),
          },
    ),
  };
}
