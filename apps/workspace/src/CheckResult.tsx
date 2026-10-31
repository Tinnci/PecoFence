import type { Component, HubReport, Observation } from "./contracts/hub";
import { checkExplanation } from "./model";
import { Status } from "./Status";
import { ChevronRight, RefreshCw } from "lucide-react";

export function CheckResult({ observation }: { observation?: Observation }) {
  if (!observation || observation.state === "unchecked") return null;
  return (
    <div
      className={`check-result check-result-${observation.state}`}
      role="status"
    >
      <p>{checkExplanation(observation)}</p>
      {observation.code || observation.exit_code != null ? (
        <details>
          <summary>检查详情</summary>
          {observation.code ? <span>检查代码：{observation.code}</span> : null}
          {observation.exit_code != null ? (
            <span>退出码：{observation.exit_code}</span>
          ) : null}
        </details>
      ) : null}
    </div>
  );
}
export function CheckedAt({ component }: { component: Component }) {
  return component.checked_at ? (
    <small className="checked-at">
      最近检查 ·{" "}
      <time dateTime={component.checked_at}>
        {new Date(component.checked_at).toLocaleString("zh-CN")}
      </time>
    </small>
  ) : null;
}
export function CheckButton({
  component,
  busy,
  busyComponent,
  onCheck,
}: {
  component: Component;
  busy: boolean;
  busyComponent: string | null;
  onCheck: (id: string) => void;
}) {
  if (!component.checkable) return null;
  const checking = busy && busyComponent === component.id;
  return (
    <button
      className="button secondary"
      aria-label={`检查${component.name}`}
      disabled={busy}
      onClick={() => onCheck(component.id)}
    >
      <RefreshCw size={14} className={checking ? "spin" : ""} />
      {checking ? "正在检查…" : "检查此组件"}
    </button>
  );
}
export function Dependencies({
  ids,
  report,
  onOpen,
}: {
  ids: string[];
  report: HubReport;
  onOpen: (id: string) => void;
}) {
  if (!ids.length) return null;
  return (
    <div className="dependencies">
      <span className="dependency-caption">依赖组件</span>
      {ids.map((id) => {
        const dependency = report.components.find((c) => c.id === id);
        return dependency ? (
          <button
            key={id}
            className="dependency-link"
            onClick={() => onOpen(id)}
            aria-label={`配置${dependency.name}`}
          >
            {dependency.name}
            <Status state={dependency.state} />
            <ChevronRight size={13} />
          </button>
        ) : (
          <span key={id} className="muted">
            {id} · 未发现组件
          </span>
        );
      })}
    </div>
  );
}
