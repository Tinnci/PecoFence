import { ActionIcon } from "./components/action-icon";
import { ContextTarget } from "./components/context-menu";
import { copyContextAction, revealContextAction } from "./context-actions";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import { useState } from "react";
import { CircleAlert, CircleCheck, FileJson, RefreshCw } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Item, ItemContent, ItemGroup, ItemTitle } from "@/components/ui/item";
import { Spinner } from "@/components/ui/spinner";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type {
  ImportPlan,
  ProjectStatus,
  SetupFailure,
} from "./contracts/setup";
import { applyTemplate, planTemplate } from "./api";

const failures: Record<string, string> = {
  get log_settings_io() {
    return t("logging.ioFailed");
  },
  get export_failed() {
    return t("views.exportFailed");
  },
  get desktop_required() {
    return t("templateImport.importRequiresTheDesktopWorkspaceApp");
  },
  get spm_missing() {
    return t("templateImport.windowsSpmWasNotFoundInstallSPMFirst");
  },
  get spm_update_required() {
    return t("templateImport.thisVersionOfSpmDoesNotSupportProjectImport");
  },
  get spm_timeout() {
    return t("templateImport.spmDidNotRespondInTimeTryAgain");
  },
  get spm_unavailable() {
    return t("templateImport.spmCouldNotRunCheckTheInstallationAndTry");
  },
  get spm_invalid_response() {
    return t("templateImport.spmReturnedAnUnrecognizedResultUpdateSPMAndTry");
  },
  get setup_busy() {
    return t("templateImport.anotherProjectOperationIsRunningPleaseWait");
  },
  get template_unreadable() {
    return t("templateImport.couldNotReadTheSelectedFile");
  },
  get template_too_large() {
    return t("templateImport.theFileExceeds1MiBAndIsNotA");
  },
  get template_stale() {
    return t("templateImport.theImportPlanHasExpiredSelectTheTemplateAgain");
  },
  get template_invalid() {
    return t("templateImport.invalidTemplate");
  },
  get import_conflict() {
    return t("templateImport.existingFilesDifferFromTheTemplateYouCanBack");
  },
  get import_io() {
    return t("templateImport.couldNotWriteProjectConfiguration");
  },
  get sign_in_failed() {
    return t("templateImport.signInDidNotFinishTryAgain");
  },
  get sign_in_host_unknown() {
    return t("templateImport.noMeegleSiteSelectedRunMeegleAuthLoginIn");
  },
  get sign_in_unsupported() {
    return t("templateImport.configureThisSourceUsingItsOwnTool");
  },
  get invalid_input() {
    return t("templateImport.invalidSelection");
  },
  get tool_missing() {
    return t("templateImport.theSourceCommandLineToolIsNotInstalled");
  },
  get signed_out() {
    return t("templateImport.thisSourceIsSignedOutSignInUnderConnect");
  },
  get source_failed() {
    return t("templateImport.theSourceReturnedAnError");
  },
  get scope_empty() {
    return t("templateImport.thereAreNoIssuesInThisScope");
  },
  get scope_too_large() {
    return t("templateImport.theScopeIsTooLargeNarrowYourSelection");
  },
  get scope_incomplete() {
    return t("templateImport.theSourceReturnedAnIncompleteScopeTryAgain");
  },
  get mapping_undetected() {
    return t("templateImport.couldNotIdentifyFields");
  },
  get questions_open() {
    return t("templateImport.someStatusesStillNeedMapping");
  },
  get draft_changed() {
    return t("templateImport.theScopeChangedAfterConfirmationConfirmItAgain");
  },
};
export const roles: Record<string, string> = {
  get deployment() {
    return t("templateImport.deploymentConfiguration");
  },
  get live_config() {
    return t("templateImport.collectionConfiguration");
  },
  get template_lock() {
    return t("templateImport.templateLock");
  },
  get scope_query() {
    return t("templateImport.scopeQuery");
  },
};
export const actions: Record<string, string> = {
  get create() {
    return t("templateImport.create");
  },
  get unchanged() {
    return t("templateImport.unchanged");
  },
  get replace() {
    return t("templateImport.backUpAndReplace");
  },
  get conflict() {
    return t("templateImport.differsFromTemplate");
  },
};
export function failureOf(error: unknown): SetupFailure {
  return typeof error === "object" && error && "code" in error
    ? (error as SetupFailure)
    : { code: "spm_unavailable" };
}
export function FailureNote({ failure }: { failure: SetupFailure }) {
  useTranslation();
  return (
    <Alert className="alert-warning">
      <CircleAlert />
      <AlertDescription>{failureText(failure)}</AlertDescription>
    </Alert>
  );
}
export function failureText(error: unknown) {
  const failure = failureOf(error);
  const text = failures[failure.code] ?? failures.spm_unavailable;
  return failure.message ? `${text}：${failure.message}` : text;
}
/** Plan, confirm and apply one template; project status is owned by the app. */
export default function TemplateImport({
  status,
  statusFailure,
  onRefresh,
}: {
  status: ProjectStatus | null;
  statusFailure: SetupFailure | null;
  onRefresh: () => Promise<void>;
}) {
  useTranslation();
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [result, setResult] = useState<ImportPlan | null>(null);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [busy, setBusy] = useState(false);
  async function run<T>(work: () => Promise<T>): Promise<T | null> {
    setBusy(true);
    setFailure(null);
    try {
      return await work();
    } catch (error) {
      const next = failureOf(error);
      if (next.code !== "template_cancelled") setFailure(next);
      return null;
    } finally {
      setBusy(false);
    }
  }
  async function refreshStatus() {
    setBusy(true);
    try {
      await onRefresh();
    } finally {
      setBusy(false);
    }
  }
  async function choose() {
    setResult(null);
    setPlan(await run(planTemplate));
  }
  async function apply(replace: boolean) {
    if (!plan) return;
    const applied = await run(() =>
      applyTemplate(plan.template.sha256, replace),
    );
    if (!applied) return;
    setPlan(null);
    setResult(applied);
    await refreshStatus();
  }
  const conflict = plan?.files.some((f) => f.action === "conflict");
  const unchanged = plan?.files.every((f) => f.action === "unchanged");
  return (
    <div className="template-import">
      {status?.projects.length ? (
        <ItemGroup aria-label={t("templateImport.importedProjects")}>
          {status.projects.map((p) => (
            <ContextTarget
              key={p.directory}
              title={p.project ?? p.directory}
              actions={[
                ...[revealContextAction(p.directory)].filter(
                  (action) => action !== null,
                ),
                copyContextAction(
                  "copy-path",
                  t("context.copyPath"),
                  p.directory,
                ),
                ...(p.project
                  ? [
                      copyContextAction(
                        "copy-project",
                        t("context.copyProject"),
                        p.project,
                      ),
                    ]
                  : []),
                ...(p.scope
                  ? [
                      copyContextAction(
                        "copy-scope",
                        t("context.copyScope"),
                        p.scope,
                      ),
                    ]
                  : []),
              ]}
            >
              <Item variant="outline" size="sm">
                <ItemContent>
                  <ItemTitle>
                    {p.project ?? "?"} / {p.scope ?? "?"}
                    <Badge
                      variant={p.check.valid ? "secondary" : "destructive"}
                    >
                      {!p.check.valid
                        ? t("templateImport.couldNotLoadConfiguration")
                        : p.check.ready
                          ? t("templateImport.readyToCollect")
                          : t("templateImport.imported")}
                    </Badge>
                  </ItemTitle>
                </ItemContent>
              </Item>
            </ContextTarget>
          ))}
        </ItemGroup>
      ) : null}
      {(failure ?? statusFailure) ? (
        <FailureNote failure={(failure ?? statusFailure)!} />
      ) : null}
      {result ? (
        <Alert role="status">
          <CircleCheck />
          <AlertTitle>
            {t("templateImport.imported2", {
              v0: result.project,
              v1: result.scope,
              v2: result.backups.length
                ? t("templateImport.originalFilesBackedUp", {
                    v0: result.backups.length,
                  })
                : "",
            })}
          </AlertTitle>
          <AlertDescription>
            {t("templateImport.signInUsingEachSourceToolSeeConnectSources")}
          </AlertDescription>
        </Alert>
      ) : null}
      {plan ? (
        <ContextTarget
          title={plan.project}
          actions={[
            ...[revealContextAction(plan.directory)].filter(
              (action) => action !== null,
            ),
            copyContextAction(
              "copy-path",
              t("context.copyPath"),
              plan.directory,
            ),
            copyContextAction(
              "copy-project",
              t("context.copyProject"),
              plan.project,
            ),
            copyContextAction("copy-scope", t("context.copyScope"), plan.scope),
          ]}
        >
          <Card
            className="import-plan"
            role="region"
            aria-label={t("templateImport.importPlan")}
          >
            <CardHeader>
              <CardTitle>{plan.template.title}</CardTitle>
              <CardDescription>
                {t("templateImport.project")}
                <strong>{plan.project}</strong> {t("templateImport.scope")}
                <strong>{plan.scope}</strong>{" "}
                {t("templateImport.liveCollection")}
              </CardDescription>
              <CardAction>
                <Badge variant="outline">
                  {plan.template.id} · {plan.template.version}
                </Badge>
              </CardAction>
            </CardHeader>
            <CardContent>
              <ul>
                {plan.files.map((f) => (
                  <li key={f.role} className={`plan-${f.action}`}>
                    <span>{roles[f.role] ?? f.role}</span>
                    <Tooltip>
                      <TooltipTrigger render={<code tabIndex={0} />}>
                        {f.path}
                      </TooltipTrigger>
                      <TooltipContent>{f.path}</TooltipContent>
                    </Tooltip>
                    <Badge
                      variant={
                        f.action === "conflict"
                          ? "destructive"
                          : f.action === "unchanged"
                            ? "ghost"
                            : "secondary"
                      }
                    >
                      {actions[f.action] ?? f.action}
                    </Badge>
                  </li>
                ))}
              </ul>
              {conflict ? (
                <p className="plan-note">{failures.import_conflict}</p>
              ) : null}
            </CardContent>
            <CardFooter className="import-actions">
              {unchanged ? (
                <span className="muted">
                  {t("templateImport.configurationAlreadyMatchesTheTemplate")}
                </span>
              ) : (
                <Button disabled={busy} onClick={() => void apply(!!conflict)}>
                  {busy ? (
                    <Spinner aria-hidden="true" />
                  ) : (
                    <ActionIcon name="import" />
                  )}
                  {conflict
                    ? t("templateImport.backUpAndReplace2")
                    : t("templateImport.import")}
                </Button>
              )}
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => setPlan(null)}
              >
                <ActionIcon name={unchanged ? "check" : "close"} />
                {unchanged ? t("templateImport.done") : t("model.cancel")}
              </Button>
            </CardFooter>
          </Card>
        </ContextTarget>
      ) : (
        <div className="import-actions">
          <Button disabled={busy} onClick={() => void choose()}>
            {busy ? <Spinner aria-hidden="true" /> : <FileJson />}
            {t("templateImport.chooseProjectTemplate")}
          </Button>
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => void refreshStatus()}
          >
            <RefreshCw />
            {t("templateImport.checkAgain")}
          </Button>
        </div>
      )}
    </div>
  );
}
