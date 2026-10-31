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
  meegle: "飞书项目",
  jira: "Jira",
  gerrit: "Gerrit",
  jenkins: "Jenkins",
};
/** What fixes each source's state; tools own their logins (ADR-0004). */
const remedies: Record<string, Record<string, string>> = {
  meegle: {
    not_configured: "在终端运行 meegle auth login，选择飞书项目站点并登录。",
    not_installed:
      "安装飞书项目命令行工具：npm install -g @lark-project/meegle",
  },
  jira: {
    not_configured: "在 WSL 中为 jira-cli 配置个人访问令牌文件。",
    signed_out:
      "Jira 拒绝了令牌，请在 jira-cli 的令牌文件中换成新的个人访问令牌。",
    not_installed: "在 WSL 中安装 jira-cli。",
  },
  gerrit: {
    not_configured: "运行 grr config init，配置 Gerrit 账号和 SSH 密钥。",
    signed_out: "Gerrit 拒绝了 SSH 认证，请检查 grr 配置的密钥。",
    not_installed: "安装 Gerrit 命令行工具 grr。",
  },
  jenkins: {
    not_configured: "在 WSL 中为 jenkins-bridge 添加 Jenkins 服务器配置。",
    unreachable: "无法通过网页数据转发访问 Jenkins，请先启动网页数据转发。",
    not_installed: "在 WSL 中安装 jenkins-bridge。",
  },
};
function remedy(c: Connection) {
  if (c.state === "connected") return null;
  return (
    remedies[c.source]?.[c.state] ??
    (c.state === "unreachable"
      ? "暂时无法连接，请检查网络后重新检查。"
      : "检查未完成，请重新检查。")
  );
}

function SourceRow({
  connection,
  signingIn,
  busy,
  onSignIn,
  onOpen,
}: {
  connection: Connection;
  signingIn: boolean;
  busy: boolean;
  onSignIn: () => void;
  onOpen: (id: string) => void;
}) {
  const fix = remedy(connection);
  const identity = [connection.account, connection.host]
    .filter(Boolean)
    .join(" · ");
  return (
    <Item
      variant="outline"
      size="sm"
      role="listitem"
      aria-label={names[connection.source]}
    >
      <ItemContent>
        <ItemTitle>
          {names[connection.source] ?? connection.source}
          <Status state={connection.state} />
        </ItemTitle>
        <ItemDescription>
          {signingIn ? "请在浏览器中完成登录…" : (fix ?? identity)}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        {connection.sign_in && connection.state !== "connected" ? (
          <Button size="sm" disabled={busy} onClick={onSignIn}>
            {signingIn ? <Spinner aria-hidden="true" /> : <LogIn />}
            登录
          </Button>
        ) : null}
        {connection.source === "jenkins" &&
        connection.state === "unreachable" ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => onOpen("cdp-forwarder")}
          >
            网页数据转发 <ArrowRight />
          </Button>
        ) : null}
      </ItemActions>
    </Item>
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
          <h4 className="source-group">问题来源</h4>
          <ItemGroup aria-label="问题来源">{rows(issueSources)}</ItemGroup>
          <h4 className="source-group">代码与构建</h4>
          <ItemGroup aria-label="代码与构建">
            {rows(["gerrit", "jenkins"])}
          </ItemGroup>
        </>
      ) : !failure ? (
        <p className="loading-inline" role="status">
          <Spinner aria-hidden="true" /> 正在检查各来源的登录状态…
        </p>
      ) : null}
      <div className="import-actions">
        <Button
          variant="outline"
          disabled={busy || (!report && !failure)}
          onClick={() => void recheck()}
        >
          {busy && !signingIn ? <Spinner aria-hidden="true" /> : <RefreshCw />}
          重新检查
        </Button>
      </div>
    </div>
  );
}
