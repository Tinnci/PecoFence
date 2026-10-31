import { useEffect, useRef, useState } from "react";
import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Card, CardContent } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemFooter,
  ItemHeader,
  ItemTitle,
} from "@/components/ui/item";
import { Spinner } from "@/components/ui/spinner";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { BrandMark } from "@/components/brand-mark";
import {
  ArrowRight,
  Blocks,
  ChevronDown,
  Home,
  LayoutDashboard,
  ListChecks,
  RefreshCw,
  ShieldCheck,
  Activity,
  Clock3,
  GitBranch,
  UserRound,
} from "lucide-react";
import { desktop, loadHub, navigation, projectStatus, type Page } from "./api";
import type { ProjectStatus, SetupFailure } from "./contracts/setup";
import { failureOf } from "./TemplateImport";
import type { Component, HubReport, LoadFailure } from "./contracts/hub";
import {
  attention,
  groups,
  mergeReport,
  progress,
  withProjects,
} from "./model";
import { Status } from "./Status";
import Checklist from "./Checklist";
import Health from "./Health";
import LoadFailurePanel, { loadFailure } from "./LoadFailure";

function ComponentCard({
  component,
  onOpen,
}: {
  component: Component;
  onOpen: () => void;
}) {
  return (
    <Item
      variant="outline"
      className="component-card"
      render={<button type="button" onClick={onOpen} />}
    >
      <ItemHeader>
        <span className="component-icon">
          <Blocks size={18} />
        </span>
        <Status state={component.state} />
      </ItemHeader>
      <ItemContent>
        <ItemTitle>{component.name}</ItemTitle>
        <ItemDescription>{component.summary}</ItemDescription>
      </ItemContent>
      <ItemFooter className="card-bottom">
        查看配置 <ArrowRight size={14} />
      </ItemFooter>
    </Item>
  );
}
export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [hub, setHub] = useState<HubReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [busyComponent, setBusyComponent] = useState<string | null>(null);
  const [error, setError] = useState<LoadFailure | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const inFlight = useRef(false);
  const lastRequest = useRef<{ check: boolean; component?: string }>({
    check: false,
  });
  const mounted = useRef(false);
  const [projects, setProjects] = useState<ProjectStatus | null>(null);
  const [projectsFailure, setProjectsFailure] = useState<SetupFailure | null>(
    null,
  );
  async function loadProjects() {
    try {
      const status = await projectStatus();
      if (mounted.current) {
        setProjects(status);
        setProjectsFailure(null);
      }
    } catch (failure) {
      if (mounted.current) setProjectsFailure(failureOf(failure));
    }
  }
  async function refresh(check: boolean, component?: string) {
    if (inFlight.current) return;
    inFlight.current = true;
    lastRequest.current = { check, component };
    setBusy(true);
    setBusyComponent(component ?? null);
    try {
      const data = component
        ? await loadHub(check, component)
        : await loadHub(check);
      if (component && data.scope !== component)
        throw new Error("hub_update_required");
      if (mounted.current) {
        setHub(mergeReport(hub, data));
        setError(null);
      }
    } catch (failure) {
      if (mounted.current) setError(loadFailure(failure));
    } finally {
      inFlight.current = false;
      if (mounted.current) {
        setBusy(false);
        setBusyComponent(null);
      }
    }
  }
  useEffect(() => {
    mounted.current = true;
    void refresh(false);
    if (desktop()) void loadProjects();
    let cleanup: (() => void) | undefined;
    void navigation((next) => {
      if (mounted.current) setPage(next);
    })
      .then((fn) => {
        if (mounted.current) cleanup = fn;
        else fn();
      })
      .catch(() => {});
    return () => {
      mounted.current = false;
      cleanup?.();
    };
  }, []);
  function openSetup(id?: string) {
    setSelected(id ?? null);
    setPage("setup");
  }
  const report = hub ? withProjects(hub, projects) : null;
  const totals = report ? progress(report) : { done: 0, total: 0 };
  const active =
    report?.components.filter((c) => c.lifecycle !== "paused") ?? [];
  const next =
    active.find((c) => c.id === "spm" && attention(c)) ??
    active.find((c) => attention(c) && c.setup.length);
  const checkedCount = active.filter((c) => c.checked_at).length;
  const headings: Record<Page, [string, string]> = {
    home: ["让工作，从这里开始。", "项目、工具与连接，都有各自的位置。"],
    setup: ["设置清单", "按组件查看配置要求和当前进度。"],
    health: ["运行状况", "检查本机工具与连接的当前状态。"],
  };
  return (
    <TooltipProvider>
      <div className="app-shell">
        <aside className="sidebar">
          <div className="brand">
            <span className="brand-mark">
              <BrandMark />
            </span>
            <div>
              工作台<small>TINNCI WORKSPACE</small>
            </div>
          </div>
          <nav aria-label="主导航">
            {(
              [
                { id: "home", title: "首页", icon: Home },
                { id: "setup", title: "设置清单", icon: ListChecks },
                { id: "health", title: "运行状况", icon: Activity },
              ] as const
            ).map((item) => (
              <Button
                variant="ghost"
                className="nav-item"
                aria-current={page === item.id ? "page" : undefined}
                key={item.id}
                onClick={() => {
                  setSelected(null);
                  setPage(item.id);
                }}
              >
                <item.icon />
                {item.title}
              </Button>
            ))}
            <span className="nav-caption">交付空间</span>
            {[
              { title: "看板", icon: LayoutDashboard },
              { title: "时间线", icon: Clock3 },
              { title: "问题与 CI/CD", icon: GitBranch },
              { title: "账户与来源", icon: UserRound },
            ].map((item) => (
              <Tooltip key={item.title}>
                <TooltipTrigger
                  render={<span className="nav-planned" tabIndex={0} />}
                >
                  <item.icon size={18} />
                  {item.title}
                  <Badge variant="secondary" className="nav-badge">
                    待支持
                  </Badge>
                </TooltipTrigger>
                <TooltipContent side="right">将在后续版本提供</TooltipContent>
              </Tooltip>
            ))}
          </nav>
          <div className="sidebar-footer">
            <ShieldCheck size={15} />
            <span>工具各自运行，工作在此汇聚</span>
          </div>
        </aside>
        <main id="main">
          <header className="topbar">
            <Breadcrumb>
              <BreadcrumbList>
                <BreadcrumbItem>工作台</BreadcrumbItem>
                <BreadcrumbSeparator />
                <BreadcrumbItem>
                  <BreadcrumbPage>
                    {page === "home"
                      ? "首页"
                      : page === "setup"
                        ? "设置清单"
                        : "运行状况"}
                  </BreadcrumbPage>
                </BreadcrumbItem>
              </BreadcrumbList>
            </Breadcrumb>
            <span className="local-indicator">
              <i />
              本机工作空间
            </span>
          </header>
          <div className="page-content">
            <div className="page-heading">
              <div>
                <span className="eyebrow">YOUR WORK, CONNECTED</span>
                <h1>{headings[page][0]}</h1>
                <p>{headings[page][1]}</p>
              </div>
              <Button
                variant="outline"
                size="lg"
                disabled={busy}
                onClick={() => void refresh(true)}
              >
                {busy ? <Spinner aria-hidden="true" /> : <RefreshCw />}
                {busy
                  ? lastRequest.current.check
                    ? "正在检查…"
                    : "正在读取…"
                  : "检查状态"}
              </Button>
            </div>
            <div aria-live="polite">
              {error ? (
                <LoadFailurePanel
                  failure={error}
                  busy={busy}
                  retained={!!report}
                  onRetry={() =>
                    void refresh(
                      lastRequest.current.check,
                      lastRequest.current.component,
                    )
                  }
                />
              ) : null}
            </div>
            {!report && !error ? (
              <div className="loading" role="status">
                <Spinner aria-hidden="true" /> 正在发现本机组件…
              </div>
            ) : null}
            {report ? (
              <>
                {page === "home" ? (
                  <>
                    <Card className="welcome-panel">
                      <CardContent className="welcome-body">
                        <div className="welcome-main">
                          <span className="eyebrow">准备好你的工作空间</span>
                          <h2>
                            {totals.total > 0 && totals.done === totals.total
                              ? "配置已完成"
                              : "连接工具，开始协作"}
                          </h2>
                          <p>
                            {next
                              ? `下一步：${next.name} · ${next.setup.find((s) => s.state !== "done")?.title ?? "查看配置"}`
                              : "查看设置清单，了解本机工具的配置要求。"}
                          </p>
                          <Button size="lg" onClick={() => openSetup(next?.id)}>
                            查看设置清单 <ArrowRight />
                          </Button>
                        </div>
                        <div className="progress-box">
                          <strong>
                            {totals.done}
                            <span> / {totals.total}</span>
                          </strong>
                          <p>配置步骤已完成</p>
                          <Progress
                            className="setup-progress"
                            value={
                              totals.total
                                ? (totals.done / totals.total) * 100
                                : 0
                            }
                            aria-label="配置完成进度"
                          />
                          <small>
                            {checkedCount
                              ? `${checkedCount} 个组件已检查`
                              : "状态尚未检查"}
                          </small>
                        </div>
                      </CardContent>
                    </Card>
                    <section className="projects">
                      <div className="section-heading">
                        <h2>你的项目</h2>
                        <span className="quiet-label">SPM</span>
                      </div>
                      <Empty className="project-empty">
                        <EmptyHeader>
                          <EmptyMedia variant="icon">
                            <LayoutDashboard />
                          </EmptyMedia>
                          <EmptyTitle>从项目配置开始</EmptyTitle>
                          <EmptyDescription>
                            导入 SPM 负责人提供的项目模板，再连接来源。
                          </EmptyDescription>
                        </EmptyHeader>
                        <EmptyContent>
                          <Button
                            variant="outline"
                            onClick={() => openSetup("spm")}
                          >
                            查看项目配置 <ArrowRight />
                          </Button>
                        </EmptyContent>
                      </Empty>
                    </section>
                    {groups.map(([group, title]) => {
                      const rows = active
                        .filter((c) => c.group === group)
                        .sort(
                          (a, b) =>
                            Number(b.id === "spm") - Number(a.id === "spm"),
                        );
                      return rows.length ? (
                        <section className="tool-group" key={group}>
                          <div className="section-heading">
                            <h2>{title}</h2>
                            <span className="count">{rows.length}</span>
                          </div>
                          <div className="component-grid">
                            {rows.map((c) => (
                              <ComponentCard
                                key={c.id}
                                component={c}
                                onOpen={() => openSetup(c.id)}
                              />
                            ))}
                          </div>
                        </section>
                      ) : null;
                    })}
                    {report.components.some((c) => c.lifecycle === "paused") ? (
                      <Collapsible className="paused">
                        <CollapsibleTrigger className="details-trigger">
                          已暂停的组件 <ChevronDown size={12} />
                        </CollapsibleTrigger>
                        <CollapsibleContent>
                          {report.components
                            .filter((c) => c.lifecycle === "paused")
                            .map((c) => (
                              <p key={c.id}>
                                {c.name} · {c.summary}
                              </p>
                            ))}
                        </CollapsibleContent>
                      </Collapsible>
                    ) : null}
                  </>
                ) : page === "setup" ? (
                  <Checklist
                    key={selected}
                    report={report}
                    selected={selected}
                    busy={busy}
                    busyComponent={busyComponent}
                    onCheck={(id) => void refresh(true, id)}
                    onOpen={openSetup}
                    projects={projects}
                    projectsFailure={projectsFailure}
                    onProjects={loadProjects}
                  />
                ) : (
                  <Health
                    report={report}
                    busy={busy}
                    busyComponent={busyComponent}
                    onCheck={(id) => void refresh(true, id)}
                    onOpen={openSetup}
                  />
                )}
                <footer className="page-footer">
                  <span>
                    {report.checked ? "最近全量检查" : "最近读取"} ·{" "}
                    {new Date(report.observed_at).toLocaleString("zh-CN")}
                  </span>
                  {report.unmanifested.length ? (
                    <span>
                      {report.unmanifested.length} 个组件尚未提供配置说明
                    </span>
                  ) : null}
                </footer>
              </>
            ) : null}
          </div>
        </main>
      </div>
    </TooltipProvider>
  );
}
