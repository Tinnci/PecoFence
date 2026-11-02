import { t } from "./i18n";
import type { Component, HubReport } from "./contracts/hub";

export const groups = () =>
  [
    ["delivery", t("model.deliveryAndBuilds")],
    ["people", t("model.communicationAndPeople")],
    ["output", t("model.reportsAndOutputs")],
    ["desktop", t("model.desktopAndTools")],
  ] as const;
export const stateLabels: Record<string, string> = {
  get ready() {
    return t("model.ready");
  },
  get installed() {
    return t("model.installed");
  },
  get not_installed() {
    return t("model.notInstalled");
  },
  get setup_pending() {
    return t("model.setupNeeded");
  },
  get unchecked() {
    return t("model.notChecked");
  },
  get unknown() {
    return t("model.unknownStatus");
  },
  get action_needed() {
    return t("model.actionNeeded");
  },
  get invalid() {
    return t("model.invalidConfiguration");
  },
  get paused() {
    return t("model.paused");
  },
  get planned() {
    return t("model.planned");
  },
  get manual() {
    return t("model.manualConfiguration");
  },
  get done() {
    return t("model.done");
  },
  get connected() {
    return t("model.connected");
  },
  get signed_out() {
    return t("model.signedOut");
  },
  get not_configured() {
    return t("model.notConfigured");
  },
  get unreachable() {
    return t("model.unreachable");
  },
  get error() {
    return t("model.checkFailed");
  },
};
export function label(state: string) {
  return stateLabels[state] ?? t("model.unknownStatus");
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
    return t("model.theCheckTimedOutTryCheckingThisComponentAgain");
  if (observation.reason === "check_unavailable")
    return t("model.theCheckCouldNotRunConfirmTheComponentIs");
  if (observation.reason === "inconsistent_result")
    return t("model.theCheckReturnedInconsistentResultsUpdateTheComponentAnd");
  const code = observation.code?.toLowerCase();
  if (code === "proxy_unavailable")
    return t("model.theBrowserForwarderIsUnreachableCheckItThenTry");
  if (code === "transport_error")
    return t("model.theToolReportedAConnectionFailureCheckTheLocal");
  if (
    code &&
    /^(auth|unauthorized|forbidden|invalid_token|credentials)/.test(code)
  )
    return t("model.checkYourSignInCredentialsAndAccessPermissionsThen");
  if (observation.state === "done") return t("model.checkPassed");
  if (observation.state === "action_needed")
    return t("model.theComponentCheckFailedReviewItsSetupChecklistAnd");
  return t("model.componentStatusHasNotBeenConfirmedTryCheckingAgain");
}

/**
 * The hub checks SPM from WSL, which reads another configuration root and cannot
 * read files Windows encrypts; the desktop's own `spm setup status` decides the
 * project step instead (a project imported from a file counts as chosen).
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
              s.id === "project"
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

/** At least one issue source must be connected; Gerrit and Jenkins are optional. */
export const issueSources = ["meegle", "jira"];
export function withConnections(
  report: HubReport,
  connections: import("./contracts/setup").ConnectionReport | null,
): HubReport {
  if (!connections) return report;
  const ready = connections.connections.some(
    (c) => issueSources.includes(c.source) && c.state === "connected",
  );
  return {
    ...report,
    components: report.components.map((c) =>
      c.id !== "spm"
        ? c
        : {
            ...c,
            setup: c.setup.map((s) =>
              s.id === "connect"
                ? {
                    ...s,
                    state: ready ? "done" : "action_needed",
                    observation: undefined,
                  }
                : s,
            ),
          },
    ),
  };
}

/** 选择项目, 设置门槛, 确认 and 开始采集 progress from the flow or an existing project. */
export function withDraft(
  report: HubReport,
  progress: {
    scoped: boolean;
    gated: boolean;
    created: boolean;
    started: boolean;
  },
): HubReport {
  const done: Record<string, boolean> = {
    project: progress.scoped,
    gates: progress.gated,
    confirm: progress.created,
    start: progress.started,
  };
  return {
    ...report,
    components: report.components.map((c) =>
      c.id !== "spm"
        ? c
        : {
            ...c,
            setup: c.setup.map((s) =>
              !(s.id in done)
                ? s
                : done[s.id]
                  ? { ...s, state: "done", observation: undefined }
                  : s.id === "project" && s.state === "done"
                    ? s
                    : {
                        ...s,
                        state:
                          s.state === "planned" ? s.state : "action_needed",
                        observation: undefined,
                      },
            ),
          },
    ),
  };
}
