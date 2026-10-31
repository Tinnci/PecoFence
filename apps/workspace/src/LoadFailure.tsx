import { CircleHelp, RefreshCw } from "lucide-react";
import type { LoadFailure } from "./contracts/hub";

const remedies: Record<
  string,
  { title: string; steps: string[]; command?: string }
> = {
  wsl_missing: {
    title: "尚未安装 WSL",
    steps: [
      "以管理员身份打开 Windows 终端并运行安装命令，按提示重启并完成 Linux 用户初始化。",
      "准备好后回到工作台重试。",
    ],
    command: "wsl --install",
  },
  wsl_unavailable: {
    title: "WSL 暂时无法使用",
    steps: [
      "在 Windows 终端运行 wsl --status，按提示完成 WSL 的安装或更新。",
      "确认可以打开 Linux 终端后重试。",
    ],
  },
  wsl_distribution_missing: {
    title: "未找到可启动的 Linux 发行版",
    steps: ["在 Windows 终端安装发行版，并首次打开它完成用户初始化。"],
    command: "wsl --install -d Debian",
  },
  wsl_distribution_unavailable: {
    title: "默认 Linux 发行版无法启动",
    steps: [
      "在 Windows 终端运行 wsl，完成默认发行版的初始化。",
      "如需更换默认发行版，使用 wsl --set-default <发行版名称>，然后重试。",
    ],
  },
  wsl_timeout: {
    title: "等待 WSL 启动超时",
    steps: ["先打开一次 Linux 终端，等待它启动完成，再回到这里重试。"],
  },
  cli_missing: {
    title: "尚未找到可运行的 dev-cli",
    steps: [
      "在默认 WSL 发行版中获取 github.com/Tinnci/dev-cli-tools 项目，在项目目录运行安装命令。",
      "若使用其他安装位置，请将 Windows 用户变量 TINNCI_DEV_CLI_WSL 设为 Linux 可执行文件的绝对路径，并重新打开工作台。",
    ],
    command: "python3 agent-cli/install.py",
  },
  cli_unavailable: {
    title: "dev-cli 暂时无法运行",
    steps: [
      "在默认 WSL 终端运行 python3 --version，确认 Python 可用。",
      "运行 dev-cli hub --help，修复启动错误后重试。",
    ],
  },
  hub_update_required: {
    title: "需要更新 dev-cli",
    steps: [
      "在 WSL 中将 dev-cli-tools 更新到最新 main，并重新运行安装命令。",
      "确认 dev-cli hub --help 包含 --component 后重试。",
    ],
    command: "python3 agent-cli/install.py",
  },
  hub_incompatible_version: {
    title: "工具清单版本不兼容",
    steps: ["将工作台和 dev-cli-tools 更新到配套版本，然后重试。"],
  },
  hub_invalid_install_path: {
    title: "dev-cli 安装位置无效",
    steps: [
      "检查 Windows 用户变量 TINNCI_DEV_CLI_WSL：它应是 Linux 中可执行文件的绝对路径。",
      "默认位置是 Linux 用户目录中的 .local/bin/dev-cli；修改变量后请重新打开工作台。",
    ],
  },
  hub_invalid_response: {
    title: "工具清单返回的数据无效",
    steps: [
      "在 WSL 中运行 dev-cli hub --json，确认能返回完整 JSON。",
      "更新或修复 dev-cli 后重试。",
    ],
  },
  hub_invalid_scope: {
    title: "组件检查返回范围不匹配",
    steps: ["更新 dev-cli-tools 后重试此组件。"],
  },
  hub_timeout: {
    title: "读取工具状态超时",
    steps: [
      "确认 WSL 终端可用后重试。若只需检查某个工具，可在运行状况中单独检查。",
    ],
  },
  hub_output_limit: {
    title: "工具清单超出读取上限",
    steps: ["更新 dev-cli-tools，并检查其输出是否包含异常重复内容，然后重试。"],
  },
  hub_busy: { title: "已有检查正在进行", steps: ["等待当前检查结束后重试。"] },
  hub_unavailable: {
    title: "无法读取本机工具状态",
    steps: ["在默认 WSL 终端运行 dev-cli hub --json，确认工具可运行后重试。"],
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
  const remedy = remedies[failure.code] ?? remedies.hub_unavailable;
  return (
    <section className="bootstrap-panel" role="alert" aria-busy={busy}>
      <div className="bootstrap-heading">
        <CircleHelp size={22} />
        <h2>{remedy.title}</h2>
      </div>
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
            上次诊断：
            <time dateTime={new Date(failure.observed_at_ms).toISOString()}>
              {new Date(failure.observed_at_ms).toLocaleString("zh-CN")}
            </time>
          </small>
          {retained ? <small>下方保留上次读取结果</small> : null}
        </div>
        <button className="button secondary" disabled={busy} onClick={onRetry}>
          <RefreshCw size={15} className={busy ? "spin" : ""} />
          {busy ? "正在重试…" : "重试"}
        </button>
      </div>
      <details>
        <summary>诊断代码</summary>
        <code>{failure.code}</code>
      </details>
    </section>
  );
}
