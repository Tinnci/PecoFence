import { useState } from "react";
import { FileJson, RefreshCw } from "lucide-react";
import type {
  ImportPlan,
  ProjectStatus,
  SetupFailure,
} from "./contracts/setup";
import { applyTemplate, planTemplate } from "./api";

const failures: Record<string, string> = {
  desktop_required: "导入需要在工作台桌面版中进行。",
  spm_missing: "未找到 Windows 版 spm。请先安装 SPM。",
  spm_update_required: "已安装的 spm 不支持项目导入，请更新 SPM。",
  spm_timeout: "spm 未在 30 秒内响应，请重试。",
  spm_unavailable: "spm 无法运行，请确认安装完整后重试。",
  spm_invalid_response: "spm 返回了无法识别的结果，请更新 SPM 后重试。",
  setup_busy: "另一个项目操作正在进行，请稍候。",
  template_unreadable: "无法读取所选文件。",
  template_too_large: "文件超过 1 MiB，不是项目模板。",
  template_stale: "导入计划已失效，请重新选择模板。",
  template_invalid: "模板无效",
  import_conflict: "已有文件与模板不同，可以备份后替换。",
  import_io: "写入项目配置失败",
};
const roles: Record<string, string> = {
  deployment: "部署配置",
  live_config: "采集配置",
  template_lock: "模板锁定",
};
const actions: Record<string, string> = {
  create: "新建",
  unchanged: "不变",
  replace: "备份后替换",
  conflict: "与模板不同",
};
export function failureOf(error: unknown): SetupFailure {
  return typeof error === "object" && error && "code" in error
    ? (error as SetupFailure)
    : { code: "spm_unavailable" };
}
function FailureNote({ failure }: { failure: SetupFailure }) {
  const text = failures[failure.code] ?? failures.spm_unavailable;
  return (
    <div className="check-result check-result-action_needed" role="alert">
      <p>{failure.message ? `${text}：${failure.message}` : text}</p>
    </div>
  );
}
function Missing({ names }: { names: string[] }) {
  return names.length ? (
    <p className="import-missing">
      采集前还需设置环境变量：<code>{names.join("、")}</code>
    </p>
  ) : null;
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
        <ul className="import-projects" aria-label="已导入的项目">
          {status.projects.map((p) => (
            <li key={p.directory}>
              <strong>
                {p.project ?? "?"} / {p.scope ?? "?"}
              </strong>
              <span>
                {!p.check.valid
                  ? "配置无法加载"
                  : p.check.ready
                    ? "可以采集"
                    : "已导入"}
              </span>
              <Missing names={p.check.missing_environment} />
            </li>
          ))}
        </ul>
      ) : null}
      {(failure ?? statusFailure) ? (
        <FailureNote failure={(failure ?? statusFailure)!} />
      ) : null}
      {result ? (
        <div className="check-result" role="status">
          <p>
            已导入 {result.project} / {result.scope}
            {result.backups.length
              ? `，原文件已备份（${result.backups.length} 个）`
              : ""}
            。
          </p>
          <Missing names={result.check?.missing_environment ?? []} />
        </div>
      ) : null}
      {plan ? (
        <section className="import-plan" aria-label="导入计划">
          <h4>
            {plan.template.title}
            <small>
              {plan.template.id} · {plan.template.version}
            </small>
          </h4>
          <p>
            项目 <strong>{plan.project}</strong> · 范围{" "}
            <strong>{plan.scope}</strong> · 实时采集
          </p>
          <ul>
            {plan.files.map((f) => (
              <li key={f.role} className={`plan-${f.action}`}>
                <span>{roles[f.role] ?? f.role}</span>
                <code title={f.path}>{f.path}</code>
                <em>{actions[f.action] ?? f.action}</em>
              </li>
            ))}
          </ul>
          {conflict ? <p>{failures.import_conflict}</p> : null}
          <div className="import-actions">
            {unchanged ? (
              <span className="muted">配置已与模板一致</span>
            ) : (
              <button
                className="button primary"
                disabled={busy}
                onClick={() => void apply(!!conflict)}
              >
                {conflict ? "备份并替换" : "导入"}
              </button>
            )}
            <button
              className="button secondary"
              disabled={busy}
              onClick={() => setPlan(null)}
            >
              {unchanged ? "完成" : "取消"}
            </button>
          </div>
        </section>
      ) : (
        <div className="import-actions">
          <button
            className="button primary"
            disabled={busy}
            onClick={() => void choose()}
          >
            <FileJson size={14} />
            选择项目模板…
          </button>
          <button
            className="button secondary"
            disabled={busy}
            onClick={() => void refreshStatus()}
          >
            <RefreshCw size={14} />
            重新检查
          </button>
        </div>
      )}
    </div>
  );
}
