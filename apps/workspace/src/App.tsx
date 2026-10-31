import { useEffect, useRef, useState } from "react";
import * as Progress from "@radix-ui/react-progress";
import {
  ArrowRight,
  Blocks,
  Check,
  ChevronRight,
  CircleHelp,
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
import type { Component, HubReport } from "./contracts/hub";
import { attention, groups, label, progress } from "./model";
import { Status } from "./Status";

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
function Checklist({
  report,
  selected,
}: {
  report: HubReport;
  selected: string | null;
}) {
  const components = report.components.filter((c) => c.lifecycle !== "paused");
  const [current, setCurrent] = useState(selected ?? components[0]?.id ?? "");
  const component = components.find((c) => c.id === current) ?? components[0];
  return (
    <div className="checklist-layout">
      <aside className="component-list" aria-label="选择组件">
        {components.map((c) => (
          <button
            key={c.id}
            aria-pressed={component?.id === c.id}
            onClick={() => setCurrent(c.id)}
          >
            <span>
              {c.name}
              <small>{label(c.state)}</small>
            </span>
            <ChevronRight size={15} />
          </button>
        ))}
      </aside>
      <section className="setup-detail" aria-label="组件设置">
        {component ? (
          <>
            <div className="section-heading">
              <h2>{component.name}</h2>
              <Status state={component.state} />
            </div>
            <p className="muted">{component.summary}</p>
            {component.issues.map((issue, i) => (
              <p role="status" className="issue" key={i}>
                {issue}
              </p>
            ))}
            {component.setup.length ? (
              component.setup.map((step, i) => (
                <article className="setup-step" key={step.id}>
                  <span
                    className={`step-number ${step.state === "done" ? "complete" : ""}`}
                  >
                    {step.state === "done" ? <Check size={15} /> : i + 1}
                  </span>
                  <div>
                    <div className="section-heading">
                      <h3>{step.title}</h3>
                      <Status state={step.state} />
                    </div>
                    <p>{step.why}</p>
                    {step.needs.length ? (
                      <small>
                        前置步骤：
                        {step.needs
                          .map(
                            (id) =>
                              component.setup.find((s) => s.id === id)?.title ??
                              id,
                          )
                          .join("、")}
                      </small>
                    ) : null}
                    {step.state === "planned" ? (
                      <small>配置入口将在后续版本提供</small>
                    ) : null}
                  </div>
                </article>
              ))
            ) : (
              <div className="empty-small">
                <ShieldCheck size={28} />
                <h3>暂无额外配置步骤</h3>
                <p>可通过“检查状态”获取组件当前状态。</p>
              </div>
            )}
          </>
        ) : (
          <p>尚未发现组件。请先安装开发工具。</p>
        )}
      </section>
    </div>
  );
}
export default function App() {
  const [page, setPage] = useState<Page>("home");
  const [report, setReport] = useState<HubReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const inFlight = useRef(false);
  const mounted = useRef(false);
  async function refresh(check: boolean) {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      const data = await loadHub(check);
      if (mounted.current) setReport(data);
    } catch {
      if (mounted.current)
        setError(
          "无法读取本机工具状态。请确认 WSL 和 dev-cli 已安装，再重试。",
        );
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(false);
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
              {busy ? "正在读取…" : "检查状态"}
            </button>
          </div>
          <div aria-live="polite">
            {error ? (
              <div className="error-panel" role="alert">
                <CircleHelp size={20} />
                <div>
                  {error}
                  {report ? <small>下方保留上次读取结果</small> : null}
                </div>
                <button
                  className="button secondary"
                  disabled={busy}
                  onClick={() => void refresh(false)}
                >
                  重试
                </button>
              </div>
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
                      <Progress.Root
                        className="progress-track"
                        value={
                          totals.total ? (totals.done / totals.total) * 100 : 0
                        }
                        aria-label="配置完成进度"
                      >
                        <Progress.Indicator
                          className="progress-value"
                          style={{
                            transform: `translateX(-${100 - (totals.total ? (totals.done / totals.total) * 100 : 0)}%)`,
                          }}
                        />
                      </Progress.Root>
                      <small>
                        {report.checked ? "已检查组件状态" : "状态尚未检查"}
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
                <Checklist key={selected} report={report} selected={selected} />
              ) : (
                <section className="health-list">
                  {report.components.map((c) => (
                    <div className="health-row" key={c.id}>
                      <div>
                        <h3>{c.name}</h3>
                        <p>{c.summary}</p>
                        {c.issues.map((issue, i) => (
                          <p className="issue" key={i}>
                            {issue}
                          </p>
                        ))}
                      </div>
                      <Status state={c.state} />
                    </div>
                  ))}
                </section>
              )}
              <footer className="page-footer">
                <span>
                  {report.checked ? "最近检查" : "最近读取"} ·{" "}
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
