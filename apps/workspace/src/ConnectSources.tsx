import { ContextTarget } from "./components/context-menu";
import { ExternalLink } from "./components/external-link";
import { copyContextAction, linkContextActions } from "./context-actions";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import { useState } from "react";
import { ArrowRight, LogIn, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from "@/components/ui/item";
import { Spinner } from "@/components/ui/spinner";
import type {
  Connection,
  ConnectionReport,
  SetupFailure,
} from "./contracts/setup";
import { signIn } from "./api";
import { issueSources } from "./model";
import { Status } from "./Status";
import { FailureNote, failureOf } from "./TemplateImport";

const names: Record<string, string> = {
  get meegle() {
    return t("projectSetup.meegle");
  },
  jira: "Jira",
  gerrit: "Gerrit",
  jenkins: "Jenkins",
};
/** What fixes each source's state; tools own their logins (ADR-0004). */
const remedies: Record<string, Record<string, string>> = {
  get meegle() {
    return {
      not_configured: t("connectSources.runMeegleAuthLoginInATerminalChooseA"),
      not_installed: t(
        "connectSources.installTheMeegleCommandLineToolNpmInstallG",
      ),
    };
  },
  get jira() {
    return {
      not_configured: t(
        "connectSources.configureAPersonalAccessTokenFileForJiraCli",
      ),
      signed_out: t("connectSources.jiraRejectedTheTokenReplaceItWithANew"),
      not_installed: t("connectSources.installJiraCliInWSL"),
    };
  },
  get gerrit() {
    return {
      not_configured: t(
        "connectSources.runGrrConfigInitToConfigureYourGerritAccount",
      ),
      signed_out: t(
        "connectSources.gerritRejectedSSHAuthenticationCheckTheKeyConfiguredIn",
      ),
      not_installed: t("connectSources.installTheGerritCommandLineToolGrr"),
    };
  },
  get jenkins() {
    return {
      not_configured: t(
        "connectSources.addJenkinsServerConfigurationForJenkinsBridgeInWSL",
      ),
      unreachable: t(
        "connectSources.jenkinsIsUnreachableThroughTheBrowserForwarderStartThe",
      ),
      not_installed: t("connectSources.installJenkinsBridgeInWSL"),
    };
  },
};
function remedy(c: Connection) {
  if (c.state === "connected") return null;
  return (
    remedies[c.source]?.[c.state] ??
    (c.state === "unreachable"
      ? t("connectSources.temporarilyUnableToConnectCheckTheNetworkAndTry")
      : t("connectSources.theCheckDidNotFinishTryAgain"))
  );
}

function SourceRow({
  connection,
  signingIn,
  busy,
  onSignIn,
  onRecheck,
  onOpen,
}: {
  connection: Connection;
  signingIn: boolean;
  busy: boolean;
  onSignIn: () => void;
  onRecheck: () => Promise<void>;
  onOpen: (id: string) => void;
}) {
  useTranslation();
  const fix = remedy(connection);
  const identity = [connection.account, connection.host]
    .filter(Boolean)
    .join(" · ");
  return (
    <ContextTarget
      title={names[connection.source] ?? connection.source}
      actions={[
        ...linkContextActions(connection.host),
        ...(connection.host
          ? [
              copyContextAction(
                "copy-host",
                t("context.copyServer"),
                connection.host,
              ),
            ]
          : []),
        ...(connection.account
          ? [
              copyContextAction(
                "copy-account",
                t("context.copyAccount"),
                connection.account,
              ),
            ]
          : []),
        {
          id: "recheck",
          label: t("templateImport.checkAgain"),
          disabled: busy,
          run: onRecheck,
        },
        copyContextAction(
          "copy-diagnostic",
          t("context.copyDiagnostic"),
          [connection.source, connection.state, connection.code]
            .filter(Boolean)
            .join(" · "),
        ),
      ]}
    >
      <Item
        variant="outline"
        size="sm"
        role="listitem"
        aria-label={names[connection.source]}
      >
        <ItemContent>
          <ItemTitle>
            <ExternalLink href={connection.host} contextMenu={false}>
              {names[connection.source] ?? connection.source}
            </ExternalLink>
            <Status state={connection.state} />
          </ItemTitle>
          <ItemDescription>
            {signingIn
              ? t("connectSources.finishSigningInInYourBrowser")
              : (fix ?? identity)}
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          {connection.sign_in && connection.state !== "connected" ? (
            <Button size="sm" disabled={busy} onClick={onSignIn}>
              {signingIn ? <Spinner aria-hidden="true" /> : <LogIn />}
              {t("connectSources.signIn")}
            </Button>
          ) : null}
          {connection.source === "jenkins" &&
          connection.state === "unreachable" ? (
            <Button
              size="sm"
              variant="outline"
              onClick={() => onOpen("cdp-forwarder")}
            >
              {t("connectSources.browserForwarder")}
              <ArrowRight />
            </Button>
          ) : null}
        </ItemActions>
      </Item>
    </ContextTarget>
  );
}

/** 连接来源: observes each tool's login and offers the fix for what is missing. */
export default function ConnectSources({
  report,
  failure,
  onRefresh,
  onReport,
  onOpen,
}: {
  report: ConnectionReport | null;
  failure: SetupFailure | null;
  onRefresh: () => Promise<void>;
  onReport: (report: ConnectionReport) => void;
  onOpen: (id: string) => void;
}) {
  useTranslation();
  const [busy, setBusy] = useState(false);
  const [signingIn, setSigningIn] = useState<string | null>(null);
  const [signInFailure, setSignInFailure] = useState<SetupFailure | null>(null);
  async function recheck() {
    setBusy(true);
    try {
      await onRefresh();
    } finally {
      setBusy(false);
    }
  }
  async function login(source: string) {
    setBusy(true);
    setSigningIn(source);
    setSignInFailure(null);
    try {
      const result = await signIn(source);
      if (report)
        onReport({
          checked_at: result.checked_at,
          connections: report.connections.map(
            (c) => result.connections.find((r) => r.source === c.source) ?? c,
          ),
        });
    } catch (error) {
      setSignInFailure(failureOf(error));
    } finally {
      setSigningIn(null);
      setBusy(false);
    }
  }
  const rows = (sources: string[]) =>
    report?.connections
      .filter((c) => sources.includes(c.source))
      .map((c) => (
        <SourceRow
          key={c.source}
          connection={c}
          busy={busy}
          signingIn={signingIn === c.source}
          onSignIn={() => void login(c.source)}
          onRecheck={recheck}
          onOpen={onOpen}
        />
      ));
  return (
    <div className="connect-sources">
      {(signInFailure ?? failure) ? (
        <FailureNote failure={(signInFailure ?? failure)!} />
      ) : null}
      {report ? (
        <>
          <h4 className="source-group">{t("projectSetup.issueSources")}</h4>
          <ItemGroup aria-label={t("projectSetup.issueSources")}>
            {rows(issueSources)}
          </ItemGroup>
          <h4 className="source-group">{t("connectSources.codeAndBuilds")}</h4>
          <ItemGroup aria-label={t("connectSources.codeAndBuilds")}>
            {rows(["gerrit", "jenkins"])}
          </ItemGroup>
        </>
      ) : !failure ? (
        <p className="loading-inline" role="status">
          <Spinner aria-hidden="true" />{" "}
          {t("connectSources.checkingSignInStatusForEachSource")}
        </p>
      ) : null}
      <div className="import-actions">
        <Button
          variant="outline"
          disabled={busy || (!report && !failure)}
          onClick={() => void recheck()}
        >
          {busy && !signingIn ? <Spinner aria-hidden="true" /> : <RefreshCw />}
          {t("templateImport.checkAgain")}
        </Button>
      </div>
    </div>
  );
}
