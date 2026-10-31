import { useState } from "react";
import { Check, ChevronRight, ShieldCheck } from "lucide-react";
import type { HubReport } from "./contracts/hub";
import { label } from "./model";
import type { ProjectStatus, SetupFailure } from "./contracts/setup";
import TemplateImport from "./TemplateImport";
import { Status } from "./Status";
import {
  CheckButton,
  CheckedAt,
  CheckResult,
  Dependencies,
} from "./CheckResult";

export default function Checklist({
  report,
  selected,
  busy,
  busyComponent,
  onCheck,
  onOpen,
  projects,
  projectsFailure,
  onProjects,
}: {
  report: HubReport;
  selected: string | null;
  busy: boolean;
  busyComponent: string | null;
  onCheck: (id: string) => void;
  onOpen: (id: string) => void;
  projects: ProjectStatus | null;
  projectsFailure: SetupFailure | null;
  onProjects: () => Promise<void>;
}) {
  const components = report.components.filter((c) => c.lifecycle !== "paused");
  const [current, setCurrent] = useState(
    selected ??
      components.find((c) => c.id === "spm")?.id ??
      components[0]?.id ??
      "",
  );
  const component = components.find((c) => c.id === current) ?? components[0];
  function focusStep(id: string) {
    const step = document.getElementById(`setup-${component?.id}-${id}`);
    step?.focus();
    step?.scrollIntoView?.({ block: "center" });
  }
  return (
    <div className="checklist-layout">
      <aside className="component-list" aria-label="选择组件">
        {components.map((c) => (
          <button
            key={c.id}
            aria-pressed={component?.id === c.id}
            onClick={() => setCurrent(c.id)}
          >
            <span>
              {c.name}
              <small>{label(c.state)}</small>
            </span>
            <ChevronRight size={15} />
          </button>
        ))}
      </aside>
      <section className="setup-detail" aria-label="组件设置">
        {component ? (
          <>
            <div className="section-heading">
              <h2>{component.name}</h2>
              <Status state={component.state} />
            </div>
            <p className="muted">{component.summary}</p>
            <div className="check-toolbar">
              <CheckedAt component={component} />
              <CheckButton
                component={component}
                busy={busy}
                busyComponent={busyComponent}
                onCheck={onCheck}
              />
            </div>
            <CheckResult observation={component.status} />
            <Dependencies
              ids={component.depends_on}
              report={report}
              onOpen={onOpen}
            />
            {component.issues.map((issue, i) => (
              <p role="status" className="issue" key={i}>
                {issue}
              </p>
            ))}
            {component.setup.length ? (
              component.setup.map((step, i) => (
                <article
                  className="setup-step"
                  key={step.id}
                  id={`setup-${component.id}-${step.id}`}
                  tabIndex={-1}
                  aria-label={step.title}
                >
                  <span
                    className={`step-number ${step.state === "done" ? "complete" : ""}`}
                  >
                    {step.state === "done" ? <Check size={15} /> : i + 1}
                  </span>
                  <div>
                    <div className="section-heading">
                      <h3>{step.title}</h3>
                      <Status state={step.state} />
                    </div>
                    <p>{step.why}</p>
                    <CheckResult observation={step.observation} />
                    {step.needs.length ? (
                      <div className="prerequisites">
                        <small>前置步骤</small>
                        {step.needs.map((id) => {
                          const local = component.setup.find(
                            (s) => s.id === id,
                          );
                          const dependency = report.components.find(
                            (c) => c.id === id,
                          );
                          return local ? (
                            <button
                              key={id}
                              className="dependency-link"
                              onClick={() => focusStep(id)}
                            >
                              {local.title}
                              <Status state={local.state} />
                              <ChevronRight size={13} />
                            </button>
                          ) : dependency ? (
                            <button
                              key={id}
                              className="dependency-link"
                              aria-label={`配置${dependency.name}`}
                              onClick={() => onOpen(id)}
                            >
                              {dependency.name}
                              <Status state={dependency.state} />
                              <ChevronRight size={13} />
                            </button>
                          ) : (
                            <small key={id}>{id} · 暂未找到配置说明</small>
                          );
                        })}
                      </div>
                    ) : null}
                    {component.id === "spm" && step.id === "template" ? (
                      <TemplateImport
                        status={projects}
                        statusFailure={projectsFailure}
                        onRefresh={onProjects}
                      />
                    ) : step.state === "planned" ? (
                      <small>配置入口将在后续版本提供</small>
                    ) : null}
                  </div>
                </article>
              ))
            ) : (
              <div className="empty-small">
                <ShieldCheck size={28} />
                <h3>暂无额外配置步骤</h3>
                <p>
                  {component.checkable
                    ? "可检查此组件，确认当前状态。"
                    : "此组件尚未声明自动检查项。"}
                </p>
              </div>
            )}
          </>
        ) : (
          <p>尚未发现组件。请先安装开发工具。</p>
        )}
      </section>
    </div>
  );
}
