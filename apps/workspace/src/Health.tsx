import { ContextTarget } from "./components/context-menu";
import { copyContextAction } from "./context-actions";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import type { Component, HubReport } from "./contracts/hub";
import { Status } from "./Status";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  CheckButton,
  CheckedAt,
  CheckResult,
  Dependencies,
} from "./CheckResult";
function StepFailures({ component }: { component: Component }) {
  useTranslation();
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
  useTranslation();
  return (
    <section className="health-list" aria-label={t("health.componentHealth")}>
      {report.components.map((c) => (
        <ContextTarget
          key={c.id}
          title={c.name}
          actions={[
            {
              id: "configure",
              label: t("app.viewConfiguration2"),
              disabled: c.lifecycle === "paused",
              run: () => onOpen(c.id),
            },
            ...(c.checkable
              ? [
                  {
                    id: "recheck",
                    label: t("checkResult.checkThisComponent"),
                    disabled: busy,
                    run: () => onCheck(c.id),
                  },
                ]
              : []),
            copyContextAction(
              "copy-component",
              t("context.copyComponent"),
              c.id,
            ),
            copyContextAction(
              "copy-diagnostic",
              t("context.copyDiagnostic"),
              [c.name, c.state, c.summary, ...c.issues].join("\n"),
            ),
          ]}
        >
          <Card className="health-row" role="article" aria-label={c.name}>
            <CardContent className="health-body">
              <div className="health-content">
                <div className="section-heading">
                  <h3>{c.name}</h3>
                  <Status state={c.state} />
                </div>
                <p>{c.summary}</p>
                <CheckedAt component={c} />
                <CheckResult
                  observation={c.status}
                  onCheck={c.checkable ? () => onCheck(c.id) : undefined}
                  busy={busy}
                  onConfigure={
                    c.lifecycle !== "paused" ? () => onOpen(c.id) : undefined
                  }
                />
                <StepFailures component={c} />
                <Dependencies
                  ids={c.depends_on}
                  report={report}
                  onOpen={onOpen}
                />
                {c.issues.map((issue, i) => (
                  <p className="issue" key={i}>
                    {issue}
                  </p>
                ))}
                {c.lifecycle !== "paused" && !c.checkable ? (
                  <small className="muted">
                    {t("health.noAutomaticChecksYet")}
                  </small>
                ) : null}
              </div>
              <div className="health-actions">
                <CheckButton
                  component={c}
                  busy={busy}
                  busyComponent={busyComponent}
                  onCheck={onCheck}
                />
                <Button
                  variant="ghost"
                  onClick={() => onOpen(c.id)}
                  disabled={c.lifecycle === "paused"}
                >
                  {t("app.viewConfiguration2")}
                </Button>
              </div>
            </CardContent>
          </Card>
        </ContextTarget>
      ))}
    </section>
  );
}
