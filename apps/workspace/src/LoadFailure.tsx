import { useTranslation } from "react-i18next";
import { t, locale } from "./i18n";
import { ChevronDown, CircleHelp, RefreshCw } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Spinner } from "@/components/ui/spinner";
import type { LoadFailure } from "./contracts/hub";

const remedies: Record<
  string,
  { title: string; steps: string[]; command?: string }
> = {
  get wsl_missing() {
    return {
      title: t("loadFailure.wslIsNotInstalled"),
      steps: [
        t(
          "loadFailure.openWindowsTerminalAsAdministratorAndRunTheInstallation",
        ),
        t("loadFailure.returnToTheWorkspaceAndTryAgainWhenReady"),
      ],
      command: "wsl --install",
    };
  },
  get wsl_unavailable() {
    return {
      title: t("loadFailure.wslIsTemporarilyUnavailable"),
      steps: [
        t("loadFailure.runWslStatusInWindowsTerminalAndFollowThe"),
        t("loadFailure.tryAgainAfterConfirmingThatALinuxTerminalOpens"),
      ],
    };
  },
  get wsl_distribution_missing() {
    return {
      title: t("loadFailure.noBootableLinuxDistributionFound"),
      steps: [t("loadFailure.installADistributionInWindowsTerminalAndOpenIt")],
      command: "wsl --install -d Debian",
    };
  },
  get wsl_distribution_unavailable() {
    return {
      title: t("loadFailure.theDefaultLinuxDistributionCouldNotStart"),
      steps: [
        t("loadFailure.runWslInWindowsTerminalToFinishSettingUp"),
        t("loadFailure.toChangeTheDefaultDistributionRunWslSetDefault"),
      ],
    };
  },
  get wsl_timeout() {
    return {
      title: t("loadFailure.timedOutWaitingForWSL"),
      steps: [t("loadFailure.openALinuxTerminalWaitForItToStart")],
    };
  },
  get cli_missing() {
    return {
      title: t("loadFailure.noRunnableDevCliFound"),
      steps: [
        t("loadFailure.getGithubComTinnciDevCliToolsInThe"),
        t("loadFailure.forACustomInstallationSetTheWindowsUserVariable"),
      ],
      command: "python3 agent-cli/install.py",
    };
  },
  get cli_unavailable() {
    return {
      title: t("loadFailure.devCliIsTemporarilyUnavailable"),
      steps: [
        t("loadFailure.runPython3VersionInTheDefaultWSLTerminalTo"),
        t("loadFailure.runDevCliHubHelpFixAnyStartupErrors"),
      ],
    };
  },
  get hub_update_required() {
    return {
      title: t("loadFailure.updateDevCli"),
      steps: [
        t("loadFailure.updateDevCliToolsToTheLatestMainIn"),
        t("loadFailure.confirmThatDevCliHubHelpIncludesComponentThen"),
      ],
      command: "python3 agent-cli/install.py",
    };
  },
  get hub_incompatible_version() {
    return {
      title: t("loadFailure.incompatibleToolManifestVersion"),
      steps: [t("loadFailure.updateTheWorkspaceAndDevCliToolsToCompatible")],
    };
  },
  get hub_invalid_install_path() {
    return {
      title: t("loadFailure.invalidDevCliInstallationPath"),
      steps: [
        t("loadFailure.checkTheWindowsUserVariableTINNCIDEVCLIWSL"),
        t("loadFailure.theDefaultPathIsLocalBinDevCliIn"),
      ],
    };
  },
  get hub_invalid_response() {
    return {
      title: t("loadFailure.theToolManifestReturnedInvalidData"),
      steps: [
        t("loadFailure.runDevCliHubJsonInWSLAndConfirm"),
        t("loadFailure.updateOrRepairDevCliThenTryAgain"),
      ],
    };
  },
  get hub_invalid_scope() {
    return {
      title: t("loadFailure.componentCheckReturnedTheWrongScope"),
      steps: [t("loadFailure.updateDevCliToolsThenCheckThisComponentAgain")],
    };
  },
  get hub_timeout() {
    return {
      title: t("loadFailure.timedOutReadingToolStatus"),
      steps: [t("loadFailure.confirmTheWSLTerminalWorksThenTryAgainTo")],
    };
  },
  get hub_output_limit() {
    return {
      title: t("loadFailure.theToolManifestExceededTheOutputLimit"),
      steps: [t("loadFailure.updateDevCliToolsAndCheckItsOutputFor")],
    };
  },
  get hub_busy() {
    return {
      title: t("loadFailure.aCheckIsAlreadyRunning"),
      steps: [t("loadFailure.waitForTheCurrentCheckToFinishThenTry")],
    };
  },
  get hub_unavailable() {
    return {
      title: t("loadFailure.couldNotReadLocalToolStatus"),
      steps: [t("loadFailure.runDevCliHubJsonInTheDefaultWSL")],
    };
  },
};

export function loadFailure(value: unknown): LoadFailure {
  const raw =
    value instanceof Error
      ? value.message
      : typeof value === "string"
        ? value
        : typeof value === "object" && value !== null && "code" in value
          ? value.code
          : null;
  const code =
    typeof raw === "string" && Object.hasOwn(remedies, raw)
      ? raw
      : "hub_unavailable";
  const time =
    typeof value === "object" && value !== null && "observed_at_ms" in value
      ? value.observed_at_ms
      : null;
  return {
    code,
    observed_at_ms:
      typeof time === "number" &&
      Number.isFinite(time) &&
      time > 0 &&
      time <= 8640000000000000
        ? time
        : Date.now(),
  };
}

export default function LoadFailurePanel({
  failure,
  busy,
  retained,
  onRetry,
}: {
  failure: LoadFailure;
  busy: boolean;
  retained: boolean;
  onRetry: () => void;
}) {
  useTranslation();
  const remedy = remedies[failure.code] ?? remedies.hub_unavailable;
  return (
    <Alert className="bootstrap-panel alert-warning" aria-busy={busy}>
      <CircleHelp />
      <AlertTitle className="bootstrap-heading">
        <h2>{remedy.title}</h2>
      </AlertTitle>
      <AlertDescription>
        <ol>
          {remedy.steps.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
        {remedy.command ? (
          <pre>
            <code>{remedy.command}</code>
          </pre>
        ) : null}
        <div className="bootstrap-footer">
          <div>
            <small>
              {t("loadFailure.lastDiagnostic")}
              <time dateTime={new Date(failure.observed_at_ms).toISOString()}>
                {new Date(failure.observed_at_ms).toLocaleString(locale())}
              </time>
            </small>
            {retained ? (
              <small>{t("loadFailure.previousResultsAreShownBelow")}</small>
            ) : null}
          </div>
          <Button variant="outline" disabled={busy} onClick={onRetry}>
            {busy ? <Spinner aria-hidden="true" /> : <RefreshCw />}
            {busy ? t("loadFailure.retrying") : t("loadFailure.retry")}
          </Button>
        </div>
        <Collapsible>
          <CollapsibleTrigger className="details-trigger">
            {t("loadFailure.diagnosticCode")}
            <ChevronDown size={12} />
          </CollapsibleTrigger>
          <CollapsibleContent className="details-content">
            <code>{failure.code}</code>
          </CollapsibleContent>
        </Collapsible>
      </AlertDescription>
    </Alert>
  );
}
