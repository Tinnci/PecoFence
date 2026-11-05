import { ModelNavigation } from "./ModelNavigation";
import { ActionIcon, ComponentIcon } from "./components/action-icon";
import { ContextMenuProvider } from "./components/context-menu";
import { modelContextActions } from "./context-actions";
import { useTranslation } from "react-i18next";
import {
  languagePreference,
  setLanguage,
  type LanguagePreference,
} from "./i18n";
import { t } from "./i18n";
import { setTheme, themePreference, type Theme } from "./theme";
import { useModelPreference } from "./model-navigation";
import { prefetchBuilds, queryClient, focusKey } from "./focus-cache";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Progress } from "@/components/ui/progress";
import { Button } from "@/components/ui/button";
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
import { TooltipProvider } from "@/components/ui/tooltip";
import { BrandMark } from "@/components/brand-mark";
import {
  ArrowRight,
  ChevronDown,
  LayoutDashboard,
  Menu,
  Palette,
  Plus,
  RefreshCw,
  Pencil,
  Download,
  Trash2,
  Upload,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuGroup,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { FocusHome, FollowPage, shortfall, useFocus } from "./Focus";
import { ModelPage } from "./Model";
import { LogsPage } from "./Logs";
import { WindowControls } from "@/components/window-controls";
import { isTauri } from "@tauri-apps/api/core";
import {
  desktop,
  focus,
  loadConnections as fetchConnections,
  loadHub,
  navigation,
  projectStatus,
  type ModelTab,
  type Page,
} from "./api";
import type {
  ConnectionReport,
  FocusModel,
  ProjectStatus,
  SetupFailure,
} from "./contracts/setup";
import { failureOf } from "./TemplateImport";
import { projectWorkbench } from "./project-workbench";
import { DataBackup } from "./DataBackup";
import { RemoveModelDialog } from "./RemoveModelDialog";
import { RenameModelDialog } from "./RenameModelDialog";
import { SearchDialog } from "./SearchDialog";
import { getVersion } from "@tauri-apps/api/app";
import type { Component, HubReport, LoadFailure } from "./contracts/hub";
import {
  attention,
  groups,
  mergeReport,
  progress,
  issueSources,
  withConnections,
  withDraft,
  withProjects,
} from "./model";
import {
  ProjectWizard,
  useProjectSetup,
  validGates,
  type Source,
} from "./ProjectSetup";
import { Status } from "./Status";
import Checklist from "./Checklist";
import ActivityPage from "./Activity";
import Health from "./Health";
import LoadFailurePanel, { loadFailure } from "./LoadFailure";

function ComponentCard({
  component,
  onOpen,
}: {
  component: Component;
  onOpen: () => void;
}) {
  useTranslation();
  return (
    <Item
      variant="outline"
      className="component-card"
      render={<button type="button" onClick={onOpen} />}
    >
      <ItemHeader>
        <span className="component-icon">
          <ComponentIcon id={component.id} responsive={false} />
        </span>
        <Status state={component.state} />
      </ItemHeader>
      <ItemContent>
        <ItemTitle>{component.name}</ItemTitle>
        <ItemDescription>{component.summary}</ItemDescription>
      </ItemContent>
      <ItemFooter className="card-bottom">
        {t("app.viewConfiguration")}
        <ArrowRight size={14} />
      </ItemFooter>
    </Item>
  );
}
function Workspace() {
  useTranslation();
  const [page, setPage] = useState<Page>("home");
  const [modelCode, setModelCode] = useState<string | null>(null);
  const [followCode, setFollowCode] = useState<string | undefined>(undefined);
  const focusState = useFocus(desktop());
  const [renamingCode, setRenamingCode] = useState<string | null>(null);
  const [currentTheme, setCurrentTheme] = useState(themePreference);
  const [backup, setBackup] = useState<"export" | "restore" | null>(null);
  const [version, setVersion] = useState("");
  useEffect(() => {
    if (isTauri()) void getVersion().then(setVersion);
  }, []);
  const removingModel = focusState.models?.find(
    (model) => model.code === focusState.removing,
  );
  const renaming = focusState.models?.find(
    (model) => model.code === renamingCode,
  );
  const [modelTab, setModelTab] = useModelPreference<ModelTab>(
    modelCode ?? "",
    "tab",
    "overview",
  );
  const prefetchTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const cancelPrefetch = () => clearTimeout(prefetchTimer.current);
  function warmModel(code: string) {
    cancelPrefetch();
    prefetchTimer.current = setTimeout(() => void prefetchBuilds(code), 150);
  }
  useEffect(() => cancelPrefetch, []);
  const scroll = useRef<HTMLDivElement>(null);
  const positions = useRef(new Map<string, number>());
  const viewKey = `${page}:${modelCode ?? ""}:${modelTab}`;
  useLayoutEffect(() => {
    const element = scroll.current;
    if (!element) return;
    element.scrollTop = positions.current.get(viewKey) ?? 0;
    return () => {
      positions.current.set(viewKey, element.scrollTop);
    };
  }, [viewKey]);
  function openModel(code: string) {
    void focusState.ensure(code);
    setModelCode(code);
    setPage("model");
  }
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
  const [connections, setConnections] = useState<ConnectionReport | null>(null);
  const [connectionsFailure, setConnectionsFailure] =
    useState<SetupFailure | null>(null);
  async function loadConnections() {
    try {
      const report = await fetchConnections();
      if (mounted.current) {
        setConnections(report);
        setConnectionsFailure(null);
      }
    } catch (failure) {
      if (mounted.current) setConnectionsFailure(failureOf(failure));
    }
  }
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
    if (desktop()) {
      void loadProjects();
      void loadConnections();
    }
    let cleanup: (() => void) | undefined;
    void navigation((next, code, tab) => {
      if (!mounted.current) return;
      if (code) setModelCode(code);
      if (tab && code)
        queryClient.setQueryData([...focusKey, "view", code, "tab"], tab);
      setPage(next);
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
  const connectedSources = (connections?.connections ?? [])
    .filter((c) => issueSources.includes(c.source) && c.state === "connected")
    .map((c) => c.source as Source);
  const projectSetup = useProjectSetup(connectedSources, loadProjects);
  async function openProjectWorkbench(seed: FocusModel) {
    const existing = (await focus("list")).models?.find(
      (model) => model.code === seed.code,
    );
    await focusState.save(projectWorkbench(seed, existing), false);
    queryClient.setQueryData([...focusKey, "view", seed.code, "tab"], "issues");
    openModel(seed.code);
  }

  const hasProject = projects?.projects.some((p) => p.check.valid) ?? false;
  function openSetup(id?: string) {
    setSelected(id ?? null);
    setPage("setup");
  }
  const report = hub
    ? withDraft(withConnections(withProjects(hub, projects), connections), {
        scoped: projectSetup.preview !== null,
        gated:
          (projectSetup.preview !== null && validGates(projectSetup.gates)) ||
          hasProject,
        created: projectSetup.created !== null || hasProject,
        started:
          projectSetup.service?.mode === "live" &&
          !!projectSetup.service.project &&
          !projectSetup.service.unmapped.some(
            (u) => u.field === "stage" && u.count > 0,
          ),
      })
    : null;
  const totals = report ? progress(report) : { done: 0, total: 0 };
  const active =
    report?.components.filter((c) => c.lifecycle !== "paused") ?? [];
  const next =
    active.find((c) => c.id === "spm" && attention(c)) ??
    active.find((c) => attention(c) && c.setup.length);
  const checkedCount = active.filter((c) => c.checked_at).length;
  const headings: Record<Page, [string, string]> = {
    home: [
      t("app.startYourWorkHere"),
      t("app.projectsToolsAndConnectionsAllInOnePlace"),
    ],
    setup: [
      t("app.setupChecklist"),
      t("app.configurationRequirementsAndProgressByComponent"),
    ],
    health: [t("app.health"), t("app.checkLocalToolsAndConnections")],
    project: [
      t("app.newSPMProject"),
      t("app.fromChoosingAScopeToYourFirstData"),
    ],
    activity: [t("app.codeActivity"), t("app.gerritChangesDayByDay")],
    logs: [t("logs.title"), ""],
    model: [t("app.model"), ""],
    follow: [t("app.followAModel"), ""],
  };
  const followed = focusState.summary?.models.length
    ? focusState.ordered
    : null;
  const tabs = (focusState.models ?? []).map((m) => {
    const summary = focusState.summary?.models.find((x) => x.code === m.code);
    return {
      code: m.code,
      name: m.name ?? m.code,
      short: summary ? shortfall(summary) > 0 : null,
    };
  });
  const more = [
    { id: "setup", title: t("app.setupChecklist") },
    { id: "health", title: t("app.health") },
    { id: "activity", title: t("app.codeActivity") },
    { id: "logs", title: t("logs.title") },
    { id: "project", title: t("app.newSPMProject") },
  ] as const;
  return (
    <TooltipProvider>
      <div className="app-shell">
        {renaming ? (
          <RenameModelDialog
            key={renaming.code}
            model={renaming}
            onSave={(name) => focusState.rename(renaming.code, name)}
            onClose={() => setRenamingCode(null)}
          />
        ) : null}
        {backup ? (
          <DataBackup
            key={backup}
            mode={backup}
            onRestored={async () => {
              await queryClient.invalidateQueries({ queryKey: focusKey });
              await focusState.refresh();
            }}
            onClose={() => setBackup(null)}
          />
        ) : null}
        {removingModel ? (
          <RemoveModelDialog
            key={removingModel.code}
            name={removingModel.name ?? removingModel.code}
            onRemove={async () => {
              await focusState.remove(removingModel.code);
              if (modelCode === removingModel.code) setPage("home");
            }}
            onClose={() => focusState.requestRemove(null)}
          />
        ) : null}
        <header className="appbar" data-tauri-drag-region>
          <button
            type="button"
            className="wordmark"
            aria-label={t("app.workspace")}
            onClick={() => {
              setSelected(null);
              setPage("home");
            }}
          >
            <BrandMark busy={focusState.refreshing} />
            <span className="wordmark-label">{t("app.workspace")}</span>
          </button>
          <ModelNavigation
            models={tabs}
            activeCode={page === "model" ? modelCode : null}
            homeActive={page === "home"}
            onHome={() => setPage("home")}
            onModel={openModel}
            onWarm={warmModel}
            onCancelWarm={cancelPrefetch}
            actions={(code) =>
              modelContextActions(
                code,
                openModel,
                focusState.refresh,
                setRenamingCode,
                focusState.requestRemove,
              )
            }
          />
          <SearchDialog onModel={openModel} />
          <Button
            variant="outline"
            className="follow-button"
            onClick={() => {
              setFollowCode(undefined);
              setPage("follow");
            }}
          >
            <Plus /> {t("app.followAModel2")}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button variant="ghost" aria-label={t("app.more")}>
                  <Menu />
                </Button>
              }
            />
            <DropdownMenuContent align="end" className="workspace-menu">
              {page === "model" &&
              modelCode &&
              focusState.models?.some((model) => model.code === modelCode) ? (
                <DropdownMenuGroup>
                  <DropdownMenuItem onClick={() => setRenamingCode(modelCode)}>
                    <Pencil />
                    {t("model.rename")}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    variant="destructive"
                    onClick={() => focusState.requestRemove(modelCode)}
                  >
                    <Trash2 />
                    {t("model.unfollow")}
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              ) : null}

              <DropdownMenuGroup>
                <DropdownMenuLabel>
                  <ActionIcon name="language" responsive={false} />
                  {t("language.label")}
                </DropdownMenuLabel>
                <DropdownMenuRadioGroup
                  value={languagePreference()}
                  onValueChange={(value) =>
                    void setLanguage(value as LanguagePreference)
                  }
                >
                  <DropdownMenuRadioItem closeOnClick value="system">
                    {t("language.system")}
                  </DropdownMenuRadioItem>
                  <DropdownMenuRadioItem closeOnClick value="zh-CN">
                    简体中文
                  </DropdownMenuRadioItem>
                  <DropdownMenuRadioItem closeOnClick value="en">
                    English
                  </DropdownMenuRadioItem>
                </DropdownMenuRadioGroup>
              </DropdownMenuGroup>
              <DropdownMenuGroup>
                <DropdownMenuLabel>
                  <Palette aria-hidden="true" />
                  {t("theme.label")}
                </DropdownMenuLabel>
                <DropdownMenuRadioGroup
                  value={currentTheme}
                  onValueChange={(value) => {
                    setTheme(value as Theme);
                    setCurrentTheme(value as Theme);
                  }}
                >
                  <DropdownMenuRadioItem closeOnClick value="light">
                    {t("theme.light")}
                  </DropdownMenuRadioItem>
                  <DropdownMenuRadioItem closeOnClick value="dark">
                    {t("theme.dark")}
                  </DropdownMenuRadioItem>
                </DropdownMenuRadioGroup>
              </DropdownMenuGroup>
              <DropdownMenuSeparator />
              {more.map((item) => (
                <DropdownMenuItem
                  key={item.id}
                  onClick={() => {
                    setSelected(null);
                    setPage(item.id);
                  }}
                >
                  <ActionIcon name={item.id} responsive={false} />
                  {item.title}
                </DropdownMenuItem>
              ))}
              <DropdownMenuItem onClick={() => setBackup("export")}>
                <Download />
                {t("backup.export")}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setBackup("restore")}>
                <Upload />
                {t("backup.restore")}
              </DropdownMenuItem>
              {version ? (
                <DropdownMenuLabel className="menu-version">
                  {version}
                </DropdownMenuLabel>
              ) : null}
            </DropdownMenuContent>
          </DropdownMenu>
          {isTauri() ? <WindowControls /> : null}
        </header>
        {focusState.removed ? (
          <div className="undo-bar" role="status">
            <span>
              {t("model.unfollowed", {
                v0: focusState.removed.name ?? focusState.removed.code,
              })}
            </span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void focusState.undoRemove()}
            >
              {t("model.undo")}
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t("model.cancel")}
              onClick={focusState.dismissRemoved}
            >
              <ActionIcon name="close" />
            </Button>
          </div>
        ) : null}
        <div className="window-scroll" ref={scroll}>
          <main id="main" className="page-content">
            {page === "model" && modelCode ? (
              <ModelPage
                key={modelCode}
                state={focusState}
                code={modelCode}
                tab={modelTab}
                onTab={setModelTab}
                onEdit={() => {
                  setFollowCode(modelCode);
                  setPage("follow");
                }}
              />
            ) : page === "follow" ? (
              <FollowPage
                key={followCode ?? "new"}
                state={focusState}
                initial={followCode}
                onDone={openModel}
                onCancel={() => setPage(followCode ? "model" : "home")}
              />
            ) : page === "home" && followed ? (
              <FocusHome state={focusState} onModel={openModel} />
            ) : (
              <>
                <div className="page-heading" hidden={page === "project"}>
                  <h1>{headings[page][0]}</h1>
                  <p>{headings[page][1]}</p>
                  <Button
                    variant="outline"
                    hidden={page === "activity"}
                    disabled={busy}
                    onClick={() => void refresh(true)}
                  >
                    {busy ? <Spinner aria-hidden="true" /> : <RefreshCw />}
                    {busy
                      ? lastRequest.current.check
                        ? t("app.checking")
                        : t("app.loading")
                      : t("app.checkStatus")}
                  </Button>
                </div>
                {page === "home" ? (
                  <section className="follow-invite">
                    <div>
                      <h2>{t("app.startByFollowingAModel")}</h2>
                      <p>
                        {t("app.chooseAModelYouOwnTheWorkspaceBringsTogether")}
                      </p>
                    </div>
                    <Button
                      onClick={() => {
                        setFollowCode(undefined);
                        setPage("follow");
                      }}
                    >
                      {t("app.followAModel3")}
                      <ArrowRight />
                    </Button>
                  </section>
                ) : null}
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
                    <Spinner aria-hidden="true" />{" "}
                    {t("app.discoveringLocalComponents")}
                  </div>
                ) : null}
                {report ? (
                  <>
                    {page === "home" ? (
                      <HomeOverview
                        report={report}
                        totals={totals}
                        next={next}
                        checkedCount={checkedCount}
                        active={active}
                        openSetup={openSetup}
                      />
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
                        onOpenWorkbench={openProjectWorkbench}
                        connections={connections}
                        connectionsFailure={connectionsFailure}
                        onConnections={loadConnections}
                        onConnectionReport={setConnections}
                        projectSetup={projectSetup}
                        onOpenProject={() => setPage("project")}
                      />
                    ) : page === "activity" ? (
                      <ActivityPage />
                    ) : page === "logs" ? (
                      <LogsPage />
                    ) : page === "project" ? (
                      <ProjectWizard
                        setup={projectSetup}
                        projects={projects}
                        onOpenWorkbench={openProjectWorkbench}
                        onExit={() => openSetup("spm")}
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
                    <footer
                      className="page-footer"
                      hidden={page === "activity"}
                    >
                      <span>
                        {report.checked
                          ? t("app.lastFullCheck")
                          : t("app.lastLoaded")}{" "}
                        · {new Date(report.observed_at).toLocaleString("zh-CN")}
                      </span>
                      {report.unmanifested.length ? (
                        <span>
                          {t("app.componentsHaveNoSetupInstructionsYet", {
                            v0: report.unmanifested.length,
                          })}
                        </span>
                      ) : null}
                    </footer>
                  </>
                ) : null}
              </>
            )}
          </main>
        </div>
      </div>
    </TooltipProvider>
  );
}

/** The setup overview: progress, the next step and every component by group. */
function HomeOverview({
  report,
  totals,
  next,
  checkedCount,
  active,
  openSetup,
}: {
  report: HubReport;
  totals: { done: number; total: number };
  next: Component | undefined;
  checkedCount: number;
  active: Component[];
  openSetup: (id?: string) => void;
}) {
  useTranslation();
  return (
    <>
      <Card className="welcome-panel">
        <CardContent className="welcome-body">
          <div className="welcome-main">
            <span className="eyebrow">{t("app.prepareYourWorkspace")}</span>
            <h2>
              {totals.total > 0 && totals.done === totals.total
                ? t("app.setupComplete")
                : t("app.connectYourToolsAndStartCollaborating")}
            </h2>
            <p>
              {next
                ? t("app.next", {
                    v0: next.name,
                    v1:
                      next.setup.find((s) => s.state !== "done")?.title ??
                      t("app.viewConfiguration2"),
                  })
                : t("app.openTheSetupChecklistToSeeLocalToolRequirements")}
            </p>
            <Button size="lg" onClick={() => openSetup(next?.id)}>
              {t("app.viewSetupChecklist")}
              <ArrowRight />
            </Button>
          </div>
          <div className="progress-box">
            <strong>
              {totals.done}
              <span> / {totals.total}</span>
            </strong>
            <p>{t("app.setupStepsCompleted")}</p>
            <Progress
              className="setup-progress"
              value={totals.total ? (totals.done / totals.total) * 100 : 0}
              aria-label={t("app.setupProgress")}
            />
            <small>
              {checkedCount
                ? t("app.componentsChecked", { count: checkedCount })
                : t("app.statusHasNotBeenChecked")}
            </small>
          </div>
        </CardContent>
      </Card>
      <section className="projects">
        <div className="section-heading">
          <h2>{t("app.yourProjects")}</h2>
          <span className="quiet-label">SPM</span>
        </div>
        <Empty className="project-empty">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <LayoutDashboard />
            </EmptyMedia>
            <EmptyTitle>{t("app.startWithProjectConfiguration")}</EmptyTitle>
            <EmptyDescription>
              {t("app.importTheProjectTemplateProvidedByYourSPMOwner")}
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button variant="outline" onClick={() => openSetup("spm")}>
              {t("app.viewProjectConfiguration")}
              <ArrowRight />
            </Button>
          </EmptyContent>
        </Empty>
      </section>
      {groups().map(([group, title]) => {
        const rows = active
          .filter((c) => c.group === group)
          .sort((a, b) => Number(b.id === "spm") - Number(a.id === "spm"));
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
            {t("app.pausedComponents")}
            <ChevronDown size={12} />
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
  );
}

export default function App() {
  return (
    <ContextMenuProvider>
      <Workspace />
    </ContextMenuProvider>
  );
}
