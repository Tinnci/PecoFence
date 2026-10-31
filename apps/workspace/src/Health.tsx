import type { Component, HubReport } from "./contracts/hub";
import { Status } from "./Status";
import {
  CheckButton,
  CheckedAt,
  CheckResult,
  Dependencies,
} from "./CheckResult";
function StepFailures({ component }: { component: Component }) {
  const status = component.status;
  return component.setup
    .filter((step) => {
      const observation = step.observation;
      if (
        !observation ||
        !["unknown", "action_needed"].includes(observation.state)
      )
        return false;
      return (
        !status ||
        observation.state !== status.state ||
        observation.code !== status.code ||
        observation.reason !== status.reason ||
        observation.exit_code !== status.exit_code
      );
    })
    .map((step) => (
      <div key={step.id}>
        <small className="muted">{step.title}</small>
        <CheckResult observation={step.observation} />
      </div>
    ));
}
export default function Health({
  report,
  busy,
  busyComponent,
  onCheck,
  onOpen,
}: {
  report: HubReport;
  busy: boolean;
  busyComponent: string | null;
  onCheck: (id: string) => void;
  onOpen: (id: string) => void;
}) {
  return (
    <section className="health-list" aria-label="组件运行状况">
      {report.components.map((c) => (
        <article className="health-row" key={c.id} aria-label={c.name}>
          <div className="health-content">
            <div className="section-heading">
              <h3>{c.name}</h3>
              <Status state={c.state} />
            </div>
            <p>{c.summary}</p>
            <CheckedAt component={c} />
            <CheckResult observation={c.status} />
            <StepFailures component={c} />
            <Dependencies ids={c.depends_on} report={report} onOpen={onOpen} />
            {c.issues.map((issue, i) => (
              <p className="issue" key={i}>
                {issue}
              </p>
            ))}
            {c.lifecycle !== "paused" && !c.checkable ? (
              <small className="muted">暂无自动检查项</small>
            ) : null}
          </div>
          <div className="health-actions">
            <CheckButton
              component={c}
              busy={busy}
              busyComponent={busyComponent}
              onCheck={onCheck}
            />
            <button
              className="button secondary"
              onClick={() => onOpen(c.id)}
              disabled={c.lifecycle === "paused"}
            >
              查看配置
            </button>
          </div>
        </article>
      ))}
    </section>
  );
}
