import { useTranslation } from "react-i18next";
import { t, locale } from "./i18n";
import type { Component, HubReport, Observation } from "./contracts/hub";
import { checkExplanation } from "./model";
import { Status } from "./Status";
import { ChevronDown, ChevronRight, RefreshCw } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";

export function CheckResult({ observation }: { observation?: Observation }) {
  useTranslation();
  if (!observation || observation.state === "unchecked") return null;
  const attention = ["action_needed", "unknown"].includes(observation.state);
  return (
    <Alert
      role="status"
      className={`check-result ${attention ? "alert-warning" : ""}`}
    >
      <AlertDescription>
        <p>{checkExplanation(observation)}</p>
        {observation.code || observation.exit_code != null ? (
          <Collapsible>
            <CollapsibleTrigger className="details-trigger">
              {t("checkResult.checkDetails")}
              <ChevronDown size={12} />
            </CollapsibleTrigger>
            <CollapsibleContent className="details-content">
              {observation.code ? (
                <span>
                  {t("checkResult.checkCode", { v0: observation.code })}
                </span>
              ) : null}
              {observation.exit_code != null ? (
                <span>
                  {t("checkResult.exitCode", { v0: observation.exit_code })}
                </span>
              ) : null}
            </CollapsibleContent>
          </Collapsible>
        ) : null}
      </AlertDescription>
    </Alert>
  );
}
export function CheckedAt({ component }: { component: Component }) {
  useTranslation();
  return component.checked_at ? (
    <small className="checked-at">
      {t("checkResult.lastCheck")}
      <time dateTime={component.checked_at}>
        {new Date(component.checked_at).toLocaleString(locale())}
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
  useTranslation();
  if (!component.checkable) return null;
  const checking = busy && busyComponent === component.id;
  return (
    <Button
      variant="outline"
      aria-label={t("checkResult.check", { v0: component.name })}
      disabled={busy}
      onClick={() => onCheck(component.id)}
    >
      {checking ? <Spinner aria-hidden="true" /> : <RefreshCw />}
      {checking ? t("app.checking") : t("checkResult.checkThisComponent")}
    </Button>
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
  useTranslation();
  if (!ids.length) return null;
  return (
    <div className="dependencies">
      <span className="dependency-caption">
        {t("checkResult.dependencies")}
      </span>
      {ids.map((id) => {
        const dependency = report.components.find((c) => c.id === id);
        return dependency ? (
          <Button
            key={id}
            variant="secondary"
            size="sm"
            onClick={() => onOpen(id)}
            aria-label={t("checklist.configure", { v0: dependency.name })}
          >
            {dependency.name}
            <Status state={dependency.state} />
            <ChevronRight />
          </Button>
        ) : (
          <span key={id} className="muted">
            {t("checkResult.componentNotFound", { v0: id })}
          </span>
        );
      })}
    </div>
  );
}
