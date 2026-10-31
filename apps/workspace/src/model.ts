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
