import { useEffect, useRef, useState } from "react";
import { Progress } from "@/components/ui/progress";
import {
  ArrowRight,
  Blocks,
  ChevronRight,
  Home,
  LayoutDashboard,
  ListChecks,
  RefreshCw,
  ShieldCheck,
  Activity,
  Layers,
  Clock3,
  GitBranch,
  UserRound,
} from "lucide-react";
import { loadHub, navigation, type Page } from "./api";
import type { Component, HubReport, LoadFailure } from "./contracts/hub";
import { attention, groups, mergeReport, progress } from "./model";
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
    <button className="component-card" onClick={onOpen}>
      <div className="card-top">
        <span className="component-icon">
          <Blocks size={18} />
        </span>
        <Status state={component.state} />
      </div>
      <h3>{component.name}</h3>
      <p>{component.summary}</p>
      <span className="card-bottom">
        查看配置 <ArrowRight size={14} />
      </span>
    </button>
  );
}
export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [report, setReport] = useState<HubReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [busyComponent, setBusyComponent] = useState<string | null>(null);
  const [error, setError] = useState<LoadFailure | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const inFlight = useRef(false);
  const lastRequest = useRef<{ check: boolean; component?: string }>({
    check: false,
  });
  const mounted = useRef(false);
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
        setReport(mergeReport(report, data));
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
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">
            <Layers size={22} />
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
            <button
              aria-current={page === item.id ? "page" : undefined}
              key={item.id}
              onClick={() => {
                setSelected(null);
                setPage(item.id);
              }}
            >
              <item.icon size={18} />
              {item.title}
            </button>
          ))}
          <span className="nav-caption">交付空间</span>
          {[
            { title: "看板", icon: LayoutDashboard },
            { title: "时间线", icon: Clock3 },
            { title: "问题与 CI/CD", icon: GitBranch },
            { title: "账户与来源", icon: UserRound },
          ].map((item) => (
            <span className="nav-planned" key={item.title}>
              <item.icon size={18} />
              {item.title}
              <small>待支持</small>
            </span>
          ))}
        </nav>
        <div className="sidebar-footer">
          <ShieldCheck size={15} />
          <span>工具各自运行，工作在此汇聚</span>
        </div>
      </aside>
      <main id="main">
        <header className="topbar">
          <span>
            工作台 <ChevronRight size={13} />{" "}
            {page === "home"
              ? "首页"
              : page === "setup"
                ? "设置清单"
                : "运行状况"}
          </span>
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
            <button
              className="button secondary"
              disabled={busy}
              onClick={() => void refresh(true)}
            >
              <RefreshCw size={15} className={busy ? "spin" : ""} />
              {busy
                ? lastRequest.current.check
                  ? "正在检查…"
                  : "正在读取…"
                : "检查状态"}
            </button>
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
              正在发现本机组件…
            </div>
          ) : null}
          {report ? (
            <>
              {page === "home" ? (
                <>
                  <section className="welcome-panel">
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
                      <button
                        className="button primary"
                        onClick={() => openSetup(next?.id)}
                      >
                        查看设置清单 <ArrowRight size={15} />
                      </button>
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
                          totals.total ? (totals.done / totals.total) * 100 : 0
                        }
                        aria-label="配置完成进度"
                      />
                      <small>
                        {checkedCount
                          ? `${checkedCount} 个组件已检查`
                          : "状态尚未检查"}
                      </small>
                    </div>
                  </section>
                  <section className="projects">
                    <div className="section-heading">
                      <h2>你的项目</h2>
                      <span className="quiet-label">SPM</span>
                    </div>
                    <div className="project-empty">
                      <span className="empty-icon">
                        <LayoutDashboard size={25} />
                      </span>
                      <div>
                        <h3>从项目配置开始</h3>
                        <p>查看 SPM 设置要求，准备项目模板和来源连接。</p>
                      </div>
                      <button
                        className="button secondary"
                        onClick={() => openSetup("spm")}
                      >
                        查看项目配置 <ArrowRight size={14} />
                      </button>
                    </div>
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
                    <details className="paused">
                      <summary>已暂停的组件</summary>
                      {report.components
                        .filter((c) => c.lifecycle === "paused")
                        .map((c) => (
                          <p key={c.id}>
                            {c.name} · {c.summary}
                          </p>
                        ))}
                    </details>
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
  );
}
