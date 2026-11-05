import { ComponentIcon } from "./components/action-icon";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import { useState } from "react";
import { Check, ChevronRight, ShieldCheck } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from "@/components/ui/item";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { HubReport } from "./contracts/hub";
import { label } from "./model";
import type {
  ConnectionReport,
  ProjectStatus,
  SetupFailure,
} from "./contracts/setup";
import TemplateImport from "./TemplateImport";
import {
  ProjectWorkbenches,
  type OpenProjectWorkbench,
} from "./ProjectWorkbenchButton";
import ConnectSources from "./ConnectSources";
import { ProjectStepSummary, type ProjectSetup } from "./ProjectSetup";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ChevronDown } from "lucide-react";
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
  connections,
  connectionsFailure,
  onConnections,
  onConnectionReport,
  projectSetup,
  onOpenProject,
  onOpenWorkbench,
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
  connections: ConnectionReport | null;
  connectionsFailure: SetupFailure | null;
  onConnections: () => Promise<void>;
  onConnectionReport: (report: ConnectionReport) => void;
  projectSetup?: ProjectSetup;
  onOpenProject?: () => void;
  onOpenWorkbench?: OpenProjectWorkbench;
}) {
  useTranslation();
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
      <aside
        className="component-list"
        aria-label={t("checklist.chooseComponent")}
      >
        <ScrollArea className="component-scroll">
          <ItemGroup>
            {components.map((c) => (
              <Item
                key={c.id}
                size="sm"
                variant={component?.id === c.id ? "muted" : "default"}
                aria-pressed={component?.id === c.id}
                render={
                  <button type="button" onClick={() => setCurrent(c.id)} />
                }
              >
                <ItemContent>
                  <ItemTitle>
                    <ComponentIcon id={c.id} />
                    {c.name}
                  </ItemTitle>
                  <ItemDescription>{label(c.state)}</ItemDescription>
                </ItemContent>
                <ItemActions>
                  <ChevronRight size={15} />
                </ItemActions>
              </Item>
            ))}
          </ItemGroup>
        </ScrollArea>
      </aside>
      <Card
        className="setup-detail"
        aria-label={t("checklist.componentSetup")}
        role="region"
      >
        <CardContent>
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
              <CheckResult
                observation={component.status}
                onCheck={
                  component.checkable ? () => onCheck(component.id) : undefined
                }
                busy={busy}
              />
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
                          <Badge variant="ghost">
                            {t("checklist.prerequisites")}
                          </Badge>
                          {step.needs.map((id) => {
                            const local = component.setup.find(
                              (s) => s.id === id,
                            );
                            const dependency = report.components.find(
                              (c) => c.id === id,
                            );
                            return local ? (
                              <Button
                                key={id}
                                variant="secondary"
                                size="sm"
                                onClick={() => focusStep(id)}
                              >
                                {local.title}
                                <Status state={local.state} />
                                <ChevronRight />
                              </Button>
                            ) : dependency ? (
                              <Button
                                key={id}
                                variant="secondary"
                                size="sm"
                                aria-label={t("checklist.configure", {
                                  v0: dependency.name,
                                })}
                                onClick={() => onOpen(id)}
                              >
                                {dependency.name}
                                <Status state={dependency.state} />
                                <ChevronRight />
                              </Button>
                            ) : (
                              <small key={id}>
                                {t("checklist.noSetupInstructionsFoundYet", {
                                  v0: id,
                                })}
                              </small>
                            );
                          })}
                        </div>
                      ) : null}
                      {component.id === "spm" && step.id === "connect" ? (
                        <ConnectSources
                          report={connections}
                          failure={connectionsFailure}
                          onRefresh={onConnections}
                          onReport={onConnectionReport}
                          onOpen={onOpen}
                        />
                      ) : null}
                      {component.id === "spm" &&
                      projectSetup &&
                      onOpenProject &&
                      (step.id === "project" ||
                        step.id === "gates" ||
                        step.id === "confirm" ||
                        step.id === "start") ? (
                        <ProjectStepSummary
                          setup={projectSetup}
                          step={step.id}
                          onOpen={onOpenProject}
                        />
                      ) : null}
                      {component.id === "spm" &&
                      step.id === "project" &&
                      onOpenWorkbench ? (
                        <ProjectWorkbenches
                          status={projects}
                          onOpen={onOpenWorkbench}
                          onChanged={onProjects}
                        />
                      ) : null}
                      {component.id === "spm" && step.id === "project" ? (
                        <Collapsible className="advanced-import">
                          <CollapsibleTrigger className="details-trigger">
                            {t("checklist.advancedImportConfigurationFile")}
                            <ChevronDown size={12} />
                          </CollapsibleTrigger>
                          <CollapsibleContent>
                            <TemplateImport
                              status={projects}
                              statusFailure={projectsFailure}
                              onRefresh={onProjects}
                            />
                          </CollapsibleContent>
                        </Collapsible>
                      ) : null}
                      {step.state === "planned" ? (
                        <small>
                          {t(
                            "checklist.configurationWillBeAvailableInAFutureVersion",
                          )}
                        </small>
                      ) : null}
                    </div>
                  </article>
                ))
              ) : (
                <Empty>
                  <EmptyHeader>
                    <EmptyMedia variant="icon">
                      <ShieldCheck />
                    </EmptyMedia>
                    <EmptyTitle>
                      {t("checklist.noAdditionalSetupSteps")}
                    </EmptyTitle>
                    <EmptyDescription>
                      {component.checkable
                        ? t(
                            "checklist.checkThisComponentToConfirmItsCurrentStatus",
                          )
                        : t("checklist.thisComponentHasNoAutomaticChecksYet")}
                    </EmptyDescription>
                  </EmptyHeader>
                </Empty>
              )}
            </>
          ) : (
            <p>
              {t("checklist.noComponentsFoundInstallTheDevelopmentToolsFirst")}
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
