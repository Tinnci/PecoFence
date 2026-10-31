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
    <Alert className="alert-warning">
      <CircleAlert />
      <AlertDescription>
        {failure.message ? `${text}：${failure.message}` : text}
      </AlertDescription>
    </Alert>
  );
}
function Missing({ names }: { names: string[] }) {
  return names.length ? (
    <div className="import-missing">
      <span>采集前还需设置环境变量</span>
      {names.map((name) => (
        <Badge key={name} variant="secondary">
          <code>{name}</code>
        </Badge>
      ))}
    </div>
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
        <ItemGroup aria-label="已导入的项目">
          {status.projects.map((p) => (
            <Item key={p.directory} variant="outline" size="sm">
              <ItemContent>
                <ItemTitle>
                  {p.project ?? "?"} / {p.scope ?? "?"}
                  <Badge variant={p.check.valid ? "secondary" : "destructive"}>
                    {!p.check.valid
                      ? "配置无法加载"
                      : p.check.ready
                        ? "可以采集"
                        : "已导入"}
                  </Badge>
                </ItemTitle>
                <Missing names={p.check.missing_environment} />
              </ItemContent>
            </Item>
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
            已导入 {result.project} / {result.scope}
            {result.backups.length
              ? `，原文件已备份（${result.backups.length} 个）`
              : ""}
            。
          </AlertTitle>
          <AlertDescription>
            <Missing names={result.check?.missing_environment ?? []} />
          </AlertDescription>
        </Alert>
      ) : null}
      {plan ? (
        <Card className="import-plan" role="region" aria-label="导入计划">
          <CardHeader>
            <CardTitle>{plan.template.title}</CardTitle>
            <CardDescription>
              项目 <strong>{plan.project}</strong> · 范围{" "}
              <strong>{plan.scope}</strong> · 实时采集
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
              <span className="muted">配置已与模板一致</span>
            ) : (
              <Button disabled={busy} onClick={() => void apply(!!conflict)}>
                {busy ? <Spinner aria-hidden="true" /> : null}
                {conflict ? "备份并替换" : "导入"}
              </Button>
            )}
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => setPlan(null)}
            >
              {unchanged ? "完成" : "取消"}
            </Button>
          </CardFooter>
        </Card>
      ) : (
        <div className="import-actions">
          <Button disabled={busy} onClick={() => void choose()}>
            {busy ? <Spinner aria-hidden="true" /> : <FileJson />}
            选择项目模板…
          </Button>
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => void refreshStatus()}
          >
            <RefreshCw />
            重新检查
          </Button>
        </div>
      )}
    </div>
  );
}
