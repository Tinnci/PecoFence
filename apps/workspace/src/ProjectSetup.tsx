import { ActionIcon } from "./components/action-icon";
import { useTranslation } from "react-i18next";
import { t, locale } from "./i18n";
import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  Minus,
  Plus,
  Search,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type {
  Activation,
  ProjectStatus,
  GateSummary,
  ServiceHealth,
  Draft,
  DraftChoices,
  Group,
  Item,
  JiraGroup,
  JiraProject,
  Measures,
  ScopeQuery,
  SetupFailure,
  Space,
  WorkItemType,
} from "./contracts/setup";
import { activate, discover, draft, remap, serviceStatus } from "./api";
import { FailureNote, failureOf } from "./TemplateImport";
import {
  ProjectWorkbenchButton,
  type OpenProjectWorkbench,
} from "./ProjectWorkbenchButton";
import { matchesSearch, normalizeSearch, searchTerms } from "./work-search";

export type Source = "meegle" | "jira";
export type Step = "project" | "gates" | "confirm" | "collect";
export const sourceNames: Record<Source, string> = {
  get meegle() {
    return t("projectSetup.meegle");
  },
  jira: "Jira",
};
export const phases = [
  "TR1",
  "TR2",
  "TR3",
  "TR4",
  "TR5",
  "TR6",
  "EVT",
  "DVT",
  "PVT",
  "MP",
];
export type Gates = {
  phase: string;
  severe: number;
  overall: number;
  fatal: number;
  milestone: string;
};
export const defaultGates: Gates = {
  phase: "TR4",
  severe: 95,
  overall: 85,
  fatal: 0,
  milestone: "",
};
export const stageNames: Record<string, string> = {
  get Open() {
    return t("projectSetup.open");
  },
  get Implementing() {
    return t("projectSetup.inProgress");
  },
  get Reviewing() {
    return t("projectSetup.inReview");
  },
  get Regression() {
    return t("projectSetup.regressionTesting");
  },
  get Closed() {
    return t("projectSetup.closed");
  },
  get WaitInfo() {
    return t("projectSetup.waitingForInformation");
  },
  get Track() {
    return t("focus.track");
  },
  get Ccb() {
    return t("projectSetup.ccbReview");
  },
};
const severityNames: Record<string, string> = {
  get Fatal() {
    return t("projectSetup.fatal");
  },
  get Severe() {
    return t("projectSetup.severe");
  },
  get Normal() {
    return t("projectSetup.normal");
  },
  get Hint() {
    return t("projectSetup.hint");
  },
};
const severityOrder = ["Fatal", "Severe", "Normal", "Hint"];
const severityColors: Record<string, string> = {
  Fatal: "#9a4a33",
  Severe: "#b8743f",
  Normal: "#8fa889",
  Hint: "#d5dccd",
};
/** Jira grouping: the whole project, an affected version, a starred filter or JQL. */
export type JiraMode = "all" | "versions" | "filters" | "jql";

/** A directory-safe identifier, as SPM requires for project and scope. */
export function identifier(text: string) {
  return (
    text
      .trim()
      .replace(/[^A-Za-z0-9._-]+/g, "-")
      .replace(/^[^A-Za-z0-9]+/, "")
      .slice(0, 64) || "project"
  );
}
/** The labels of a Meegle filter, joined for reading. */
export function filterLabel(scope: ScopeQuery) {
  return scope.source === "meegle"
    ? (scope.filter?.values.map((v) => v.label).join("、") ?? "")
    : "";
}
/** One name for several picked groups: their common prefix (E6751SM-A, E6751SM-B → E6751SM). */
export function commonName(names: string[]) {
  if (names.length < 2) return names[0] ?? "";
  let prefix = names[0];
  for (const n of names)
    while (!n.startsWith(prefix)) prefix = prefix.slice(0, -1);
  prefix = prefix.replace(/[-_ ·.]+$/, "");
  return prefix.length >= 3 ? prefix : names.join("+");
}
export function describe(scope: ScopeQuery) {
  if (scope.source === "meegle")
    return scope.filter
      ? t("projectSetup.meegleWhereIs", {
          v0: scope.type_name,
          v1: scope.filter.field_name,
          v2: filterLabel(scope),
        })
      : t("projectSetup.allOfMeegle", { v0: scope.type_name });
  if (scope.label)
    return t("projectSetup.jira", { v0: scope.project, v1: scope.label });
  return scope.jql
    ? t("projectSetup.jiraMatching", { v0: scope.project, v1: scope.jql })
    : t("projectSetup.allOfJira", { v0: scope.project });
}
export function gateSentence(gates: Gates) {
  const parts = [
    t("projectSetup.severeIssueResolutionAtLeast", { v0: gates.severe }),
    t("projectSetup.overallAtLeast", { v0: gates.overall }),
    t("projectSetup.atMostFatalIssues", { v0: gates.fatal }),
    t("projectSetup.phase", { v0: gates.phase }),
  ];
  if (gates.milestone)
    parts.push(t("projectSetup.milestone", { v0: gates.milestone }));
  return parts.join(t("format.listSeparator"));
}
export function validGates(gates: Gates) {
  const pct = (n: number) => Number.isInteger(n) && n >= 0 && n <= 100;
  return (
    pct(gates.severe) &&
    pct(gates.overall) &&
    Number.isInteger(gates.fatal) &&
    gates.fatal >= 0 &&
    phases.includes(gates.phase)
  );
}
const percent = (v: number, n: number) =>
  n ? Math.round((v / n) * 1000) / 10 : 0;

/** Each gate judged on the scope's measures; targets only move the threshold. */
export function judge(measures: Measures, gates: Gates) {
  const rate = (target: number, v: number, n: number) => {
    const gap = Math.max(0, Math.ceil((target / 100) * n) - v);
    return { now: percent(v, n), v, n, gap, ok: gap === 0 };
  };
  return {
    severe: rate(gates.severe, measures.severe_resolved, measures.severe_total),
    overall: rate(
      gates.overall,
      measures.overall_resolved,
      measures.overall_total,
    ),
    fatal: {
      open: measures.fatal_open,
      ok: measures.fatal_open <= gates.fatal,
    },
  };
}
export function daysUntil(date: string, today = new Date()) {
  if (!date) return null;
  const target = new Date(`${date}T00:00:00`);
  const start = new Date(today.toDateString());
  return Math.round((target.getTime() - start.getTime()) / 86400000);
}

type Lists = {
  spaces: Space[] | null;
  types: WorkItemType[] | null;
  groups: Group[] | null;
  items: Item[] | null;
  withoutIssues: number | null;
  empty: Item[] | null;
  projects: JiraProject[] | null;
  jiraGroups: JiraGroup[] | null;
};

/** The new-project flow's state; lives in App so it survives page changes. */
export function useProjectSetup(
  connected: Source[],
  onCreated: () => Promise<void>,
) {
  const [step, setStep] = useState<Step>("project");
  const [source, setSource] = useState<Source | null>(null);
  const [lists, setLists] = useState<Lists>({
    spaces: null,
    types: null,
    groups: null,
    items: null,
    withoutIssues: null,
    empty: null,
    projects: null,
    jiraGroups: null,
  });
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [space, setSpace] = useState<Space | null>(null);
  const [type, setType] = useState<WorkItemType | null>(null);
  const [group, setGroup] = useState<Group | null>(null);
  const [search, setSearch] = useState("");
  const [showEmpty, setShowEmpty] = useState(false);
  const [picked, setPicked] = useState<Item[]>([]);
  /** A click picks one group; Ctrl/⌘-click adds or removes it, for one scope of several. */
  const pick = (row: Item, add: boolean) =>
    setPicked((current) =>
      !add
        ? [row]
        : current.some((i) => i.id === row.id)
          ? current.filter((i) => i.id !== row.id)
          : [...current, row],
    );
  const [project, setProject] = useState<JiraProject | null>(null);
  const [jiraMode, setJiraMode] = useState<JiraMode>("versions");
  const [jiraGroup, setJiraGroup] = useState<JiraGroup | null>(null);
  const [jql, setJql] = useState("");
  const [preview, setPreview] = useState<Draft | null>(null);
  const [previewAt, setPreviewAt] = useState<Date | null>(null);
  const [previewFailure, setPreviewFailure] = useState<SetupFailure | null>(
    null,
  );
  const [gates, setGates] = useState<Gates>(defaultGates);
  const [ids, setIds] = useState({ project: "", scope: "main" });
  const [scopeComplete, setScopeComplete] = useState(true);
  const [stages, setStages] = useState<Record<string, string>>({});
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<Draft | null>(null);
  const [createFailure, setCreateFailure] = useState<SetupFailure | null>(null);
  const [changed, setChanged] = useState<Draft | null>(null);
  const [activating, setActivating] = useState(false);
  const [activation, setActivation] = useState<Activation | null>(null);
  const [service, setService] = useState<ServiceHealth | null>(null);
  const [remapping, setRemapping] = useState(false);
  const [remapFailure, setRemapFailure] = useState<SetupFailure | null>(null);
  async function refreshService() {
    try {
      setService(await serviceStatus());
    } catch {
      /* A busy or stopped service keeps the last reading. */
    }
  }
  useEffect(() => {
    void refreshService();
  }, []);
  const [activateFailure, setActivateFailure] = useState<SetupFailure | null>(
    null,
  );
  const generation = useRef<Record<string, number>>({});

  async function load<K extends keyof Lists>(
    key: K,
    work: () => Promise<Lists[K]>,
  ) {
    const n = (generation.current[key] = (generation.current[key] ?? 0) + 1);
    setFailure(null);
    try {
      const value = await work();
      if (generation.current[key] === n)
        setLists((l) => ({ ...l, [key]: value }));
      return value;
    } catch (error) {
      if (generation.current[key] === n) setFailure(failureOf(error));
      return null;
    }
  }

  const connectedKey = connected.join();
  useEffect(() => {
    if (!source || !connected.includes(source)) setSource(connected[0] ?? null);
  }, [connectedKey]);
  useEffect(() => {
    if (source === "meegle" && !lists.spaces)
      void load(
        "spaces",
        async () => (await discover("spaces")).spaces ?? [],
      ).then((spaces) => setSpace((s) => s ?? spaces?.[0] ?? null));
    if (source === "jira" && !lists.projects)
      void load(
        "projects",
        async () => (await discover("jira_projects")).projects ?? [],
      );
  }, [source]);
  useEffect(() => {
    setType(null);
    if (!space) return;
    void load(
      "types",
      async () =>
        (await discover("types", { space: space.simple_name })).types ?? [],
    ).then((types) => setType(types?.[0] ?? null));
  }, [space?.project_key]);
  useEffect(() => {
    setGroup(null);
    if (!space || !type) return;
    void load(
      "groups",
      async () =>
        (
          await discover("groups", {
            space: space.simple_name,
            kind: type.api_name,
          })
        ).groups ?? [],
    ).then((groups) => setGroup(groups?.[0] ?? null));
  }, [type?.type_key]);
  // Groups that hold issues, counted, largest first.
  useEffect(() => {
    setShowEmpty(false);
    setPicked([]);
    setLists((l) => ({ ...l, items: null, empty: null, withoutIssues: null }));
    if (!space || !type || !group) return;
    void load("items", async () => {
      const found = await discover("items", {
        space: space.simple_name,
        kind: group.target_type,
        count_type: type.type_key,
        field: group.field,
      });
      setLists((l) => ({ ...l, withoutIssues: found.without_issues ?? null }));
      return found.items ?? [];
    });
  }, [group?.field, type?.type_key]);
  // Groups without issues, listed on request and narrowed by the search.
  useEffect(() => {
    if (!showEmpty || !space || !group) return;
    setLists((l) => ({ ...l, empty: null }));
    const timer = setTimeout(
      () =>
        void load("empty", async () => {
          const all =
            (
              await discover("items", {
                space: space.simple_name,
                kind: group.target_type,
                search: search.trim() || undefined,
              })
            ).items ?? [];
          const counted = new Set((lists.items ?? []).map((i) => i.id));
          return all.filter((i) => !counted.has(i.id));
        }),
      search ? 350 : 0,
    );
    return () => clearTimeout(timer);
  }, [showEmpty, group?.field, search]);
  useEffect(() => {
    setJiraGroup(null);
    setLists((l) => ({ ...l, jiraGroups: null }));
    if (!project || (jiraMode !== "versions" && jiraMode !== "filters")) return;
    void load(
      "jiraGroups",
      async () =>
        (
          await discover(
            jiraMode === "versions" ? "jira_versions" : "jira_filters",
            { project: project.key },
          )
        ).jira_groups ?? [],
    );
  }, [project?.key, jiraMode]);

  const scope: ScopeQuery | null =
    source === "meegle" && space && type && group && picked.length
      ? {
          source: "meegle",
          space: space.simple_name,
          type_key: type.type_key,
          work_item_type: type.api_name,
          type_name: type.name,
          filter: {
            field: group.field,
            field_name: group.name,
            values: picked.map((i) => ({ id: i.id, label: i.name })),
          },
        }
      : source === "jira" && project
        ? jiraMode === "all"
          ? { source: "jira", project: project.key }
          : jiraMode === "jql"
            ? jql.trim()
              ? { source: "jira", project: project.key, jql: jql.trim() }
              : null
            : jiraGroup
              ? {
                  source: "jira",
                  project: project.key,
                  jql: jiraGroup.jql,
                  label:
                    jiraMode === "versions"
                      ? t("projectSetup.affectedVersion", {
                          v0: jiraGroup.name,
                        })
                      : t("projectSetup.inFilter", { v0: jiraGroup.name }),
                }
              : null
        : null;
  const scopeKey = scope ? JSON.stringify(scope) : "";
  const label =
    scope?.source === "meegle"
      ? commonName(picked.map((i) => i.name))
      : (project?.name ?? project?.key ?? "");

  function choices(answers: Record<string, string>): DraftChoices | null {
    if (!scope) return null;
    return {
      project: identifier(ids.project || label),
      scope_id: identifier(ids.scope || "main"),
      scope,
      phase: gates.phase,
      ...(gates.milestone ? { milestone_date: gates.milestone } : {}),
      severe_target_pct: gates.severe,
      overall_target_pct: gates.overall,
      fatal_max: gates.fatal,
      scope_complete: scopeComplete,
      answers: { severities: {}, stages: answers },
    };
  }

  // The count, breakdown and mapping questions come from a draft of the scope.
  useEffect(() => {
    setPreview(null);
    setPreviewFailure(null);
    setCreated(null);
    setChanged(null);
    setActivation(null);
    setActivateFailure(null);
    setStages({});
    if (!scope) return;
    setIds((current) => ({ ...current, project: identifier(label) }));
    const n = (generation.current.preview =
      (generation.current.preview ?? 0) + 1);
    const request = choices({});
    if (!request) return;
    draft({ ...request, project: identifier(label) })
      .then((d) => {
        if (generation.current.preview !== n) return;
        setPreview(d);
        setPreviewAt(new Date());
      })
      .catch((error) => {
        if (generation.current.preview === n)
          setPreviewFailure(failureOf(error));
      });
  }, [scopeKey]);

  const questions = preview?.questions ?? [];
  const blocking = questions.filter((q) => q.count > 0);
  const answered = blocking.every((q) => stages[q.value]);
  async function start(project: string, scope: string) {
    setActivating(true);
    setActivateFailure(null);
    // The daemon's own health shows the switch and the first read as they happen.
    const poll = setInterval(() => void refreshService(), 2500);
    try {
      const result = await activate(project, scope);
      setActivation(result);
      if (result.health) setService(result.health);
    } catch (error) {
      setActivateFailure(failureOf(error));
    } finally {
      clearInterval(poll);
      setActivating(false);
    }
  }
  /** Maps values that appeared after creation, then restarts collection with them. */
  async function remapAndRestart(answers: Record<string, string>) {
    if (!service?.project || !service.scope) return;
    setRemapping(true);
    setRemapFailure(null);
    try {
      await remap(service.project, service.scope, answers);
      await start(service.project, service.scope);
    } catch (error) {
      setRemapFailure(failureOf(error));
    } finally {
      setRemapping(false);
    }
  }
  async function create() {
    const request = choices(stages);
    if (!request || !preview) return;
    setCreating(true);
    setCreateFailure(null);
    setChanged(null);
    try {
      const planned = await draft(request);
      if (
        planned.count !== preview.count ||
        planned.questions.some((q) => q.count > 0 && !stages[q.value])
      ) {
        setChanged(planned);
        setPreview(planned);
        return;
      }
      const done = await draft(request, planned.template.sha256);
      setCreated(done);
      setStep("collect");
      await onCreated();
      void start(done.project, done.scope);
    } catch (error) {
      setCreateFailure(failureOf(error));
    } finally {
      setCreating(false);
    }
  }

  return {
    service,
    refreshService,
    remapping,
    remapFailure,
    remapAndRestart,
    connected,
    step,
    setStep,
    source,
    setSource,
    lists,
    failure,
    space,
    group,
    setGroup,
    search,
    setSearch,
    showEmpty,
    setShowEmpty,
    picked,
    pick,
    setPicked,
    project,
    setProject,
    jiraMode,
    setJiraMode,
    jiraGroup,
    setJiraGroup,
    jql,
    setJql,
    scope,
    preview,
    previewAt,
    previewFailure,
    gates,
    setGates,
    ids,
    setIds,
    scopeComplete,
    setScopeComplete,
    stages,
    setStages,
    questions,
    blocking,
    answered,
    creating,
    created,
    createFailure,
    changed,
    create,
    activating,
    activation,
    activateFailure,
    start,
  };
}
export type ProjectSetup = ReturnType<typeof useProjectSetup>;

function Stepper({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const order: Step[] = ["project", "gates", "confirm", "collect"];
  const at = order.indexOf(setup.step);
  const preview = setup.preview;
  const name =
    setup.scope?.source === "meegle"
      ? filterLabel(setup.scope)
      : (setup.jiraGroup?.name ?? setup.project?.key);
  const steps: { id: Step; title: string; done?: string }[] = [
    {
      id: "project",
      title: t("projectSetup.chooseProject"),
      done: preview ? `${name} · ${preview.count}` : undefined,
    },
    {
      id: "gates",
      title: t("projectSetup.setTargets"),
      done: `${setup.gates.phase} · ${setup.gates.severe}%`,
    },
    { id: "confirm", title: t("projectSetup.confirm") },
    { id: "collect", title: t("projectSetup.startCollection") },
  ];
  return (
    <ol className="wizard-steps" aria-label={t("projectSetup.newProjectSteps")}>
      <li className="wizard-step done">
        <Check aria-hidden="true" />
        {t("projectSetup.connectSources")}
      </li>
      {steps.map((s, i) => {
        const current = i === at;
        const done = i < at;
        return (
          <li
            key={s.id}
            className={`wizard-step ${current ? "current" : done ? "done" : ""}`}
            aria-current={current ? "step" : undefined}
          >
            {done && !setup.created ? (
              <button type="button" onClick={() => setup.setStep(s.id)}>
                <Check aria-hidden="true" />
                {s.done ?? s.title}
              </button>
            ) : (
              <span>
                {done ? <Check aria-hidden="true" /> : null}
                {current ? `${i + 2} ` : ""}
                {done ? (s.done ?? s.title) : s.title}
              </span>
            )}
          </li>
        );
      })}
    </ol>
  );
}

function SkeletonRows({ count = 8 }: { count?: number }) {
  useTranslation();
  const widths = [62, 45, 70, 52, 58, 40, 66, 48];
  return (
    <div className="pick-list" aria-hidden="true">
      {Array.from({ length: count }, (_, i) => (
        <div className="pick-row" key={i}>
          <Skeleton style={{ width: `${widths[i % widths.length]}%` }} />
        </div>
      ))}
    </div>
  );
}

/** Rows with a count and a bar scaled to the largest; names are the only words. */
function CountList<T>({
  label,
  rows,
  selected,
  name,
  count,
  onPick,
  multiple,
}: {
  label: string;
  multiple?: boolean;
  rows: T[];
  selected: (row: T) => boolean;
  name: (row: T) => string;
  count: (row: T) => number | undefined;
  onPick: (row: T, add: boolean) => void;
}) {
  useTranslation();
  const max = Math.max(1, ...rows.map((r) => count(r) ?? 0));
  return (
    <div
      className="pick-list"
      role="listbox"
      aria-label={label}
      aria-multiselectable={multiple || undefined}
    >
      {rows.map((row) => {
        const n = count(row);
        const active = selected(row);
        return (
          <button
            type="button"
            role="option"
            aria-selected={active}
            className="pick-row"
            key={name(row)}
            onClick={(e) => onPick(row, e.ctrlKey || e.metaKey)}
          >
            <span className="pick-name">{name(row)}</span>
            <span className="pick-count">{n ?? ""}</span>
            <span className="pick-bar" aria-hidden="true">
              {n ? (
                <span
                  style={{
                    width: `${Math.max(2, Math.round((n / max) * 100))}%`,
                  }}
                />
              ) : null}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function Chips<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T | null;
  options: { value: T; title: string }[];
  onChange: (value: T) => void;
}) {
  useTranslation();
  return (
    <ToggleGroup
      variant="outline"
      size="sm"
      className="chips"
      value={value ? [value] : []}
      onValueChange={(next: string[]) => {
        if (next[0]) onChange(next[0] as T);
      }}
      aria-label={label}
    >
      {options.map((o) => (
        <ToggleGroupItem key={o.value} value={o.value}>
          {o.title}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}

function SearchBox({
  value,
  label,
  onChange,
}: {
  value: string;
  label: string;
  onChange: (value: string) => void;
}) {
  useTranslation();
  return (
    <label className="pick-search">
      <Search aria-hidden="true" />
      <Input
        value={value}
        placeholder={label}
        aria-label={label}
        maxLength={60}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}

function MeeglePicker({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const { lists, group } = setup;
  // 关联项目 → 项目; 影响版本 stays.
  const target = group
    ? group.name.replace(/^关联/, "") || group.name
    : t("projectSetup.project");
  const terms = searchTerms(setup.search);
  const rows = (lists.items ?? []).filter((i) =>
    matchesSearch(normalizeSearch(i.name), terms),
  );
  const allMatched =
    rows.length > 0 &&
    rows.every((row) => setup.picked.some((picked) => picked.id === row.id));
  return (
    <>
      <div className="picker-head">
        {lists.groups?.length ? (
          <Chips
            label={t("projectSetup.group")}
            value={group?.field ?? null}
            options={lists.groups.map((g) => ({
              value: g.field,
              title: g.name,
            }))}
            onChange={(field) =>
              setup.setGroup(
                lists.groups?.find((g) => g.field === field) ?? null,
              )
            }
          />
        ) : null}
        <SearchBox
          value={setup.search}
          label={t("projectSetup.search", { v0: target })}
          onChange={setup.setSearch}
        />
      </div>
      {!setup.showEmpty && terms.length > 0 && rows.length > 0 ? (
        <div className="pick-match-actions">
          <span className="quiet">
            {t("projectSetup.matchingGroups", { count: rows.length })}
          </span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              const ids = new Set(rows.map((row) => row.id));
              setup.setPicked((current) =>
                allMatched
                  ? current.filter((item) => !ids.has(item.id))
                  : [...current.filter((item) => !ids.has(item.id)), ...rows],
              );
            }}
          >
            {t(
              allMatched
                ? "projectSetup.unselectMatches"
                : "projectSetup.selectMatches",
            )}
          </Button>
        </div>
      ) : null}
      <div className="pick-columns" aria-hidden="true">
        <span>{target}</span>
        <span>{t("focus.issues")}</span>
        <span />
      </div>
      <div className="pick-scroll">
        {lists.items === null ? (
          <SkeletonRows />
        ) : setup.showEmpty ? (
          lists.empty === null ? (
            <SkeletonRows />
          ) : (
            <CountList
              label={t("projectSetup.withNoIssues", { v0: target })}
              rows={lists.empty}
              multiple
              selected={(i) => setup.picked.some((p) => p.id === i.id)}
              name={(i) => i.name}
              count={() => 0}
              onPick={setup.pick}
            />
          )
        ) : rows.length ? (
          <CountList
            label={target}
            rows={rows}
            multiple
            selected={(i) => setup.picked.some((p) => p.id === i.id)}
            name={(i) => i.name}
            count={(i) => i.count}
            onPick={setup.pick}
          />
        ) : (
          <p className="pick-empty">
            {t("projectSetup.noMatching", { v0: target })}
          </p>
        )}
      </div>
      <div className="picker-foot">
        <span>
          {setup.picked.length > 1
            ? t("projectSetup.selectedCombinedIntoOneScope", {
                v0: setup.picked.length,
              })
            : lists.items
              ? t("projectSetup.withIssuesOrderedByIssueCountCtrlClickTo", {
                  v0: lists.items.length,
                  v1: target,
                })
              : ""}
        </span>
        {lists.withoutIssues ? (
          <Button
            variant="link"
            size="sm"
            onClick={() => setup.setShowEmpty(!setup.showEmpty)}
          >
            {setup.showEmpty
              ? t("projectSetup.onlyShowThoseWithIssues")
              : t("projectSetup.showWithNoIssues", {
                  v0: lists.withoutIssues,
                  v1: target,
                })}
          </Button>
        ) : null}
      </div>
    </>
  );
}

function JiraPicker({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const { lists, project } = setup;
  const query = setup.search.trim().toLowerCase();
  if (!project) {
    const rows =
      lists.projects?.filter((p) =>
        `${p.key} ${p.name}`.toLowerCase().includes(query),
      ) ?? null;
    return (
      <>
        <div className="picker-head">
          <SearchBox
            value={setup.search}
            label={t("projectSetup.searchJiraProjects")}
            onChange={setup.setSearch}
          />
        </div>
        <div className="pick-scroll">
          {rows === null ? (
            <SkeletonRows />
          ) : (
            <CountList
              label={t("projectSetup.jiraProjects")}
              rows={rows}
              selected={() => false}
              name={(p) => `${p.key} · ${p.name}`}
              count={(p) => p.count}
              onPick={(p) => {
                setup.setSearch("");
                setup.setProject(p);
              }}
            />
          )}
        </div>
      </>
    );
  }
  const grouped = setup.jiraMode === "versions" || setup.jiraMode === "filters";
  const groups =
    lists.jiraGroups?.filter((g) => g.name.toLowerCase().includes(query)) ??
    null;
  return (
    <>
      <div className="picker-head">
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            setup.setProject(null);
            setup.setSearch("");
          }}
        >
          <ArrowLeft />
          {project.key}
        </Button>
        <Chips<JiraMode>
          label={t("projectSetup.scope")}
          value={setup.jiraMode}
          options={[
            { value: "all", title: t("projectSetup.entireProject") },
            { value: "versions", title: t("projectSetup.affectedVersion2") },
            { value: "filters", title: t("projectSetup.myFilters") },
            { value: "jql", title: "JQL" },
          ]}
          onChange={(mode) => {
            setup.setSearch("");
            setup.setJiraMode(mode);
          }}
        />
        {grouped ? (
          <SearchBox
            value={setup.search}
            label={
              setup.jiraMode === "versions"
                ? t("projectSetup.searchVersions")
                : t("projectSetup.searchFilters")
            }
            onChange={setup.setSearch}
          />
        ) : null}
      </div>
      {grouped ? (
        <div className="pick-scroll">
          {groups === null ? (
            <SkeletonRows />
          ) : groups.length ? (
            <CountList
              label={
                setup.jiraMode === "versions"
                  ? t("projectSetup.affectedVersion2")
                  : t("projectSetup.filters")
              }
              rows={groups}
              selected={(g) => g.name === setup.jiraGroup?.name}
              name={(g) => g.name}
              count={(g) => g.count}
              onPick={setup.setJiraGroup}
            />
          ) : (
            <p className="pick-empty">
              {setup.jiraMode === "filters"
                ? t("projectSetup.youHaveNoFavoriteFiltersYetAddFavoritesIn")
                : t("projectSetup.thisProjectHasNoVersions")}
            </p>
          )}
        </div>
      ) : setup.jiraMode === "jql" ? (
        <JqlField setup={setup} />
      ) : (
        <p className="pick-empty">
          {t("projectSetup.trackAllIssuesIn", { v0: project.key })}
        </p>
      )}
    </>
  );
}

/** JQL applies on Enter or blur, so typing does not re-read the scope per keystroke. */
function JqlField({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const [text, setText] = useState(setup.jql);
  const commit = () => setup.setJql(text.trim());
  return (
    <label className="jql-field">
      <span>{t("projectSetup.jqlFilterWithinThisProject")}</span>
      <Input
        value={text}
        placeholder={t("projectSetup.issuetypeBug")}
        maxLength={500}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
      />
    </label>
  );
}

function SeverityBar({ draft }: { draft: Draft }) {
  useTranslation();
  const counts = Object.fromEntries(
    severityOrder.map((s) => [
      s,
      draft.breakdown.severities
        .filter((v) => v.mapped === s)
        .reduce((n, v) => n + v.count, 0),
    ]),
  );
  return (
    <div className="preview-block">
      <span className="preview-label">{t("projectSetup.severity")}</span>
      <div className="severity-bar" aria-hidden="true">
        {severityOrder.map((s) =>
          counts[s] ? (
            <span
              key={s}
              style={{
                width: `${(counts[s] / draft.count) * 100}%`,
                background: severityColors[s],
              }}
            />
          ) : null,
        )}
      </div>
      <dl className="severity-legend">
        {severityOrder.map((s) => (
          <div key={s}>
            <dt>{severityNames[s]}</dt>
            <dd>{counts[s]}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function StatusSummary({ draft }: { draft: Draft }) {
  useTranslation();
  const m = draft.breakdown.measures;
  const top = draft.breakdown.statuses
    .slice(0, 6)
    .map((s) => `${s.value} ${s.count}`)
    .join(" · ");
  return (
    <div className="preview-block">
      <span className="preview-label">{t("projectSetup.status")}</span>
      <div className="status-line">
        <span>{t("projectSetup.resolved")}</span>
        <strong>{m.overall_resolved}</strong>
      </div>
      <div className="status-line">
        <span>{t("projectSetup.unresolved")}</span>
        <strong>{m.overall_total - m.overall_resolved}</strong>
      </div>
      <small>{top}</small>
    </div>
  );
}

/** Before a pick: what the list holds, so the panel is never an empty card. */
function Overview({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const { lists, group, project } = setup;
  const noun = group
    ? group.name.replace(/^关联/, "") || group.name
    : t("projectSetup.project");
  const [label, total, where, hint] =
    setup.source === "meegle"
      ? [
          t("projectSetup.meegle2", { v0: setup.space?.name ?? "" }),
          lists.items?.reduce((n, i) => n + (i.count ?? 0), 0),
          t("projectSetup.issuesAcross", {
            v0: lists.items?.length ?? 0,
            v1: noun,
          }),
          t("projectSetup.chooseOneOrHoldCtrlToCombineSelections", {
            v0: noun,
          }),
        ]
      : project
        ? [
            `Jira · ${project.key}`,
            project.count,
            t("projectSetup.issuesIn", { v0: project.name }),
            t("projectSetup.chooseTheEntireProjectAVersionOrAFilter"),
          ]
        : [
            "Jira",
            lists.projects?.reduce((n, p) => n + p.count, 0),
            t("projectSetup.issuesAcrossProjects", {
              v0: lists.projects?.length ?? 0,
            }),
            t("projectSetup.chooseAProject"),
          ];
  return (
    <div className="overview">
      <span className="preview-label">{label}</span>
      {total === undefined ? (
        <Skeleton className="count-skeleton" />
      ) : (
        <div className="big-count">
          <strong>{total}</strong>
          <span>{where}</span>
        </div>
      )}
      <p className="scope-hint">{hint}</p>
    </div>
  );
}

function Preview({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const { preview, scope } = setup;
  const heading =
    scope?.source === "meegle"
      ? setup.group?.name
      : setup.jiraMode === "versions"
        ? t("projectSetup.affectedVersion2")
        : setup.jiraMode === "filters"
          ? t("projectSetup.filters")
          : t("projectSetup.jiraProjects");
  const name =
    scope?.source === "meegle"
      ? filterLabel(scope)
      : (setup.jiraGroup?.name ?? setup.project?.key);
  return (
    <aside
      className="wizard-aside"
      aria-label={t("projectSetup.scopePreview")}
      aria-live="polite"
    >
      {!scope ? (
        <Overview setup={setup} />
      ) : (
        <>
          <div className="preview-name">
            <span className="preview-label">{heading}</span>
            <strong>{name}</strong>
          </div>
          {setup.previewFailure ? (
            <FailureNote failure={setup.previewFailure} />
          ) : !preview ? (
            <div className="preview-loading" aria-hidden="true">
              <Skeleton className="count-skeleton" />
              <span className="preview-label">
                {t("projectSetup.loadingSeveritiesAndStatuses")}
              </span>
              <Skeleton className="bar-skeleton" />
              <Skeleton style={{ width: "70%" }} />
              <Skeleton style={{ width: "52%" }} />
            </div>
          ) : (
            <>
              <div className="big-count">
                <strong key={preview.count}>{preview.count}</strong>
                <span>{t("projectSetup.issues")}</span>
              </div>
              <SeverityBar draft={preview} />
              <StatusSummary draft={preview} />
            </>
          )}
        </>
      )}
      <div className="aside-foot">
        <span>
          {setup.previewAt && preview
            ? t("projectSetup.loadedAt", {
                v0: setup.previewAt.toLocaleTimeString(locale(), {
                  hour: "2-digit",
                  minute: "2-digit",
                }),
              })
            : ""}
        </span>
        <Button disabled={!preview} onClick={() => setup.setStep("gates")}>
          {t("projectSetup.setTargets2")}
          <ArrowRight />
        </Button>
      </div>
    </aside>
  );
}

function ProjectStep({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  return (
    <>
      <div className="wizard-title">
        <div>
          <h2>{t("projectSetup.chooseProject")}</h2>
          <p>{t("projectSetup.theSelectedScopeIsSavedAsSpecificIssuesYou")}</p>
        </div>
        {setup.connected.length > 1 ? (
          <Chips<Source>
            label={t("projectSetup.issueSources")}
            value={setup.source}
            options={setup.connected.map((s) => ({
              value: s,
              title:
                s === "meegle" && setup.space
                  ? t("projectSetup.meegle2", { v0: setup.space.name })
                  : s === "jira" && setup.project
                    ? `Jira · ${setup.project.key}`
                    : sourceNames[s],
            }))}
            onChange={(s) => {
              setup.setSource(s);
              setup.setSearch("");
            }}
          />
        ) : null}
      </div>
      {setup.failure ? <FailureNote failure={setup.failure} /> : null}
      <div className="wizard-columns">
        <section
          className="wizard-main picker"
          aria-label={t("projectSetup.scope")}
        >
          {setup.source === "meegle" ? (
            <MeeglePicker setup={setup} />
          ) : setup.source === "jira" ? (
            <JiraPicker setup={setup} />
          ) : (
            <p className="scope-hint">
              {t("projectSetup.connectMeegleOrJiraFirst")}
            </p>
          )}
        </section>
        <Preview setup={setup} />
      </div>
    </>
  );
}

function TargetStepper({
  label,
  value,
  unit,
  max,
  onChange,
}: {
  label: string;
  value: number;
  unit: string;
  max: number;
  onChange: (n: number) => void;
}) {
  useTranslation();
  const clamp = (n: number) => Math.min(max, Math.max(0, n));
  return (
    <div className="stepper" role="group" aria-label={label}>
      <Button
        variant="outline"
        size="icon-sm"
        aria-label={t("projectSetup.decrease", { v0: label })}
        onClick={() => onChange(clamp(value - 1))}
      >
        <Minus />
      </Button>
      <span>
        {value}
        {unit}
      </span>
      <Button
        variant="outline"
        size="icon-sm"
        aria-label={t("projectSetup.increase", { v0: label })}
        onClick={() => onChange(clamp(value + 1))}
      >
        <Plus />
      </Button>
    </div>
  );
}

function Meter({
  now,
  target,
  ok,
}: {
  now: number;
  target: number;
  ok: boolean;
}) {
  useTranslation();
  return (
    <div className="meter" aria-hidden="true">
      <span
        className={ok ? "meter-fill ok" : "meter-fill short"}
        style={{ width: `${now}%` }}
      />
      <span className="meter-mark" style={{ left: `calc(${target}% - 1px)` }} />
    </div>
  );
}

function Verdict({ ok, text }: { ok: boolean; text: string }) {
  useTranslation();
  return <span className={ok ? "verdict ok" : "verdict short"}>{text}</span>;
}

function GatesStep({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const { gates, setGates, preview } = setup;
  if (!preview) return null;
  const set = (patch: Partial<Gates>) => setGates({ ...gates, ...patch });
  const j = judge(preview.breakdown.measures, gates);
  const m = preview.breakdown.measures;
  const days = daysUntil(gates.milestone);
  return (
    <>
      <div className="wizard-title">
        <div>
          <h2>{t("projectSetup.setTargets")}</h2>
          <p>
            {t(
              "projectSetup.calculatedFromTheCurrentIssuesDeliveredIssuesRegressionTesting",
              { v0: preview.count },
            )}
          </p>
        </div>
      </div>
      <div className="wizard-columns">
        <section className="wizard-main gates" aria-label={t("model.targets")}>
          <article
            className="gate-card"
            aria-label={t("focus.severeIssueResolution")}
          >
            <div className="gate-head">
              <strong>{t("focus.severeIssueResolution")}</strong>
              <span>
                {t("projectSetup.severeIssuesIncludingFatalIssues", {
                  v0: m.severe_total,
                })}
              </span>
            </div>
            <TargetStepper
              label={t("focus.severeIssueResolution")}
              value={gates.severe}
              unit="%"
              max={100}
              onChange={(severe) => set({ severe })}
            />
            <Meter now={j.severe.now} target={gates.severe} ok={j.severe.ok} />
            <div className="gate-now">
              <span>
                {t("projectSetup.now", {
                  v0: j.severe.now,
                  v1: j.severe.v,
                  v2: j.severe.n,
                })}
              </span>
              <Verdict
                ok={j.severe.ok}
                text={
                  j.severe.ok
                    ? t("focus.targetMet")
                    : t("focus.moreToResolve", { v0: j.severe.gap })
                }
              />
            </div>
          </article>
          <article
            className="gate-card"
            aria-label={t("focus.overallResolution")}
          >
            <div className="gate-head">
              <strong>{t("focus.overallResolution")}</strong>
              <span>
                {t("projectSetup.totalIssuesDuplicateAndWonTFixIssuesDo", {
                  v0: m.overall_total,
                })}
              </span>
            </div>
            <TargetStepper
              label={t("focus.overallResolution")}
              value={gates.overall}
              unit="%"
              max={100}
              onChange={(overall) => set({ overall })}
            />
            <Meter
              now={j.overall.now}
              target={gates.overall}
              ok={j.overall.ok}
            />
            <div className="gate-now">
              <span>
                {t("projectSetup.now", {
                  v0: j.overall.now,
                  v1: j.overall.v,
                  v2: j.overall.n,
                })}
              </span>
              <Verdict
                ok={j.overall.ok}
                text={
                  j.overall.ok
                    ? t("focus.targetMet")
                    : t("focus.moreToResolve", { v0: j.overall.gap })
                }
              />
            </div>
          </article>
          <article
            className="gate-card"
            aria-label={t("model.fatalIssueLimit")}
          >
            <div className="gate-head">
              <strong>{t("model.fatalIssueLimit")}</strong>
              <span>{t("projectSetup.unresolvedFatalIssues")}</span>
            </div>
            <TargetStepper
              label={t("model.fatalIssueLimit")}
              value={gates.fatal}
              unit={t("projectSetup.issues2")}
              max={999}
              onChange={(fatal) => set({ fatal })}
            />
            <div className="gate-now">
              <span>
                {t("projectSetup.nowUnresolved", { v0: j.fatal.open })}
              </span>
              <Verdict
                ok={j.fatal.ok}
                text={
                  j.fatal.ok
                    ? t("focus.targetMet")
                    : t("projectSetup.overLimit", {
                        v0: j.fatal.open - gates.fatal,
                      })
                }
              />
            </div>
          </article>
        </section>
        <aside className="wizard-aside">
          <div className="aside-block">
            <span className="preview-label">{t("projectSetup.phase2")}</span>
            <div
              className="phase-grid"
              role="group"
              aria-label={t("projectSetup.phase2")}
            >
              {phases.map((p) => (
                <Button
                  key={p}
                  size="sm"
                  variant={gates.phase === p ? "default" : "outline"}
                  aria-pressed={gates.phase === p}
                  onClick={() => set({ phase: p })}
                >
                  {p}
                </Button>
              ))}
            </div>
          </div>
          <label className="aside-block">
            <span className="preview-label">
              {t("projectSetup.milestoneDate")}
            </span>
            <Input
              type="date"
              value={gates.milestone}
              onChange={(e) => set({ milestone: e.target.value })}
            />
            {days !== null ? (
              <small>
                {days >= 0
                  ? t("projectSetup.daysRemaining", { v0: days })
                  : t("projectSetup.daysOverdue", { v0: -days })}
              </small>
            ) : null}
          </label>
          <p className="gate-sentence">{gateSentence(gates)}。</p>
          <div className="aside-foot">
            <Button variant="ghost" onClick={() => setup.setStep("project")}>
              <ArrowLeft /> {t("projectSetup.back")}
            </Button>
            <Button
              disabled={!validGates(gates)}
              onClick={() => setup.setStep("confirm")}
            >
              {t("projectSetup.confirm2")}
              <ArrowRight />
            </Button>
          </div>
        </aside>
      </div>
    </>
  );
}

function StageSelect({
  value,
  field,
  setup,
  optional,
}: {
  value: string;
  field: string;
  setup: ProjectSetup;
  optional?: boolean;
}) {
  useTranslation();
  const names = field === "severity" ? severityNames : stageNames;
  const options = Object.entries(names).map(([v, l]) => ({
    value: v,
    label: l,
  }));
  return (
    <Select
      value={setup.stages[value] ?? null}
      items={options}
      onValueChange={(stage) => {
        const next = { ...setup.stages };
        if (stage) next[value] = stage as string;
        else delete next[value];
        setup.setStages(next);
      }}
    >
      <SelectTrigger
        size="sm"
        aria-label={t("projectSetup.mappingFor", { v0: value })}
      >
        <SelectValue
          placeholder={
            optional ? t("projectSetup.decideLater") : t("projectSetup.choose")
          }
        />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function ConfirmStep({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const { preview, scope } = setup;
  if (!preview || !scope) return null;
  const blockingValues = new Set(setup.blocking.map((q) => q.value));
  const later = setup.questions.filter((q) => q.count === 0);
  const ready = validGates(setup.gates) && setup.answered && !setup.creating;
  return (
    <>
      <div className="wizard-title">
        <div>
          <h2>{t("projectSetup.confirm")}</h2>
          <p className="confirm-sentence">
            {t("projectSetup.trackIssuesIn", {
              v0: identifier(setup.ids.project),
              v1: identifier(setup.ids.scope),
              v2: describe(scope),
              v3: preview.count,
              v4: gateSentence(setup.gates),
            })}
          </p>
        </div>
      </div>
      <div className="wizard-columns">
        <section
          className="wizard-main mapping"
          aria-label={t("projectSetup.statusMapping")}
        >
          <div className="mapping-head">
            <strong>{t("projectSetup.statusMapping")}</strong>
            <span>
              {t("projectSetup.severityFrom", {
                v0: preview.mapping.severity_from,
              })}
            </span>
          </div>
          <table>
            <thead>
              <tr>
                <th>
                  {scope.source === "meegle"
                    ? t("projectSetup.meegleStatus")
                    : t("projectSetup.jiraStatus")}
                </th>
                <th>{t("focus.issues")}</th>
                <th>{t("projectSetup.spmStage")}</th>
              </tr>
            </thead>
            <tbody>
              {preview.breakdown.statuses.map((s) => (
                <tr key={s.value}>
                  <td>{s.value}</td>
                  <td>{s.count}</td>
                  <td>
                    {s.mapped ? (
                      stageNames[s.mapped]
                    ) : blockingValues.has(s.value) ? (
                      <StageSelect
                        value={s.value}
                        field="stage"
                        setup={setup}
                      />
                    ) : (
                      "—"
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {later.length ? (
            <div className="later-note">
              <p>
                {t(
                  "projectSetup.couldNotBeMappedAutomaticallyThereAreNoIssues",
                  { v0: later.map((q) => q.value).join(t("projectSetup.and")) },
                )}
              </p>
              <div className="later-selects">
                {later.map((q) => (
                  <label key={q.value}>
                    {q.value}
                    <StageSelect
                      value={q.value}
                      field={q.field}
                      setup={setup}
                      optional
                    />
                  </label>
                ))}
              </div>
            </div>
          ) : null}
        </section>
        <aside className="wizard-aside">
          <div className="id-fields">
            <label>
              <span className="preview-label">{t("projectSetup.project")}</span>
              <Input
                value={setup.ids.project}
                maxLength={64}
                onChange={(e) =>
                  setup.setIds({ ...setup.ids, project: e.target.value })
                }
              />
            </label>
            <label>
              <span className="preview-label">{t("projectSetup.scope")}</span>
              <Input
                value={setup.ids.scope}
                maxLength={64}
                onChange={(e) =>
                  setup.setIds({ ...setup.ids, scope: e.target.value })
                }
              />
            </label>
          </div>
          <label className="complete-field">
            <Checkbox
              checked={setup.scopeComplete}
              onCheckedChange={(checked) =>
                setup.setScopeComplete(checked === true)
              }
            />
            <span>
              {t("projectSetup.theseAreAllTheIssuesForThisDelivery")}
              <small>
                {t("projectSetup.ifUncheckedTheBoardWillIndicateThatTheScope")}
              </small>
            </span>
          </label>
          <div className="aside-note">
            <span>{t("projectSetup.writeTo", { v0: preview.directory })}</span>
            <span>
              {t(
                "projectSetup.theSpmdBackgroundServiceCollectsDataEvery5Minutes",
              )}
            </span>
          </div>
          <div className="confirm-lane" aria-live="polite">
            {setup.createFailure ? (
              <FailureNote failure={setup.createFailure} />
            ) : setup.changed ? (
              <p>
                {t("projectSetup.theScopeNowContainsIssuesPleaseConfirmAgain", {
                  v0: setup.changed.count,
                })}
              </p>
            ) : null}
          </div>
          <div className="aside-foot">
            <Button variant="ghost" onClick={() => setup.setStep("gates")}>
              <ArrowLeft /> {t("projectSetup.back")}
            </Button>
            <Button disabled={!ready} onClick={() => void setup.create()}>
              {setup.creating ? (
                <Spinner aria-hidden="true" />
              ) : (
                <ActionIcon name="project" />
              )}
              {t("projectSetup.createAndStartCollection")}
            </Button>
          </div>
        </aside>
      </div>
    </>
  );
}

/** Gate results from the daemon's own snapshot. */
function GateResults({
  gates,
  targets,
}: {
  gates: GateSummary[];
  targets: Gates;
}) {
  useTranslation();
  const find = (id: string) => gates.find((g) => g.gate_id === id);
  const rate = (id: string, title: string, target: number) => {
    const g = find(id);
    if (!g) return null;
    const now = percent(g.v, g.n);
    const ok = g.gap === 0;
    return (
      <article className="gate-card" aria-label={title}>
        <div className="gate-head row">
          <strong>{title}</strong>
          <Verdict
            ok={ok}
            text={
              ok
                ? t("focus.targetMet")
                : t("focus.moreToResolve", { v0: g.gap })
            }
          />
        </div>
        <span className="result-number">{now}%</span>
        <Meter now={now} target={target} ok={ok} />
        <small>
          {t("projectSetup.target", { v0: g.v, v1: g.n, v2: target })}
        </small>
      </article>
    );
  };
  const fatal = find("fatal");
  return (
    <div className="gate-results">
      {rate("severe", t("focus.severeIssueResolution"), targets.severe)}
      {rate("overall", t("focus.overallResolution"), targets.overall)}
      {fatal ? (
        <article
          className="gate-card"
          aria-label={t("projectSetup.fatalIssues")}
        >
          <div className="gate-head row">
            <strong>{t("projectSetup.fatalIssues")}</strong>
            <Verdict
              ok={fatal.gap === 0}
              text={
                fatal.gap === 0
                  ? t("focus.targetMet")
                  : t("projectSetup.overLimit", { v0: fatal.gap })
              }
            />
          </div>
          <span className="result-number">{fatal.gap + targets.fatal}</span>
          <small>
            {t("projectSetup.unresolvedFatalIssuesLimit", {
              v0: targets.fatal,
            })}
          </small>
        </article>
      ) : null}
    </div>
  );
}

/** Statuses that issues took after creation; answering restarts collection. */
export function UnmappedAlert({ setup }: { setup: ProjectSetup }) {
  useTranslation();
  const values = (setup.service?.unmapped ?? []).filter(
    (u) => u.field === "stage" && u.count > 0,
  );
  const [answers, setAnswers] = useState<Record<string, string>>({});
  if (!values.length || !setup.service?.project) return null;
  const options = Object.entries(stageNames).map(([v, l]) => ({
    value: v,
    label: l,
  }));
  const total = values.reduce((n, u) => n + u.count, 0);
  return (
    <div className="unmapped-alert" role="alert">
      <p>
        {t(
          "projectSetup.hasIssuesWithUnmappedStatusesCurrentlyCountedAsUnknown",
          { v0: setup.service.project, v1: total },
        )}
      </p>
      <div className="later-selects">
        {values.map((u) => (
          <label key={u.value}>
            {t("projectSetup.message", { v0: u.value, v1: u.count })}
            <Select
              value={answers[u.value] ?? null}
              items={options}
              onValueChange={(stage) =>
                stage && setAnswers({ ...answers, [u.value]: stage as string })
              }
            >
              <SelectTrigger
                size="sm"
                aria-label={t("projectSetup.mappingFor", { v0: u.value })}
              >
                <SelectValue placeholder={t("projectSetup.choose")} />
              </SelectTrigger>
              <SelectContent>
                {options.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </label>
        ))}
      </div>
      {setup.remapFailure ? <FailureNote failure={setup.remapFailure} /> : null}
      <div>
        <Button
          size="sm"
          disabled={
            setup.remapping ||
            setup.activating ||
            values.some((u) => !answers[u.value])
          }
          onClick={() => void setup.remapAndRestart(answers)}
        >
          {setup.remapping || setup.activating ? (
            <Spinner aria-hidden="true" />
          ) : null}
          {t("projectSetup.saveAndRestartCollection")}
        </Button>
      </div>
    </div>
  );
}

function CollectStep({
  setup,
  projects,
  onOpenWorkbench,
  onDone,
}: {
  setup: ProjectSetup;
  projects?: ProjectStatus | null;
  onOpenWorkbench?: OpenProjectWorkbench;
  onDone: () => void;
}) {
  useTranslation();
  const { created, service } = setup;
  if (!created) return null;
  const workspace = projects?.projects.find(
    (project) =>
      project.project === created.project && project.scope === created.scope,
  )?.workspace;
  const ours =
    service?.mode === "live" &&
    service.project === created.project &&
    service.scope === created.scope;
  const switched = ours || !!setup.activation;
  const read = !!setup.activation || (ours && (service?.revision ?? 0) > 0);
  const line = (ok: boolean, busy: boolean, text: string) => (
    <li className={ok ? "ok" : ""}>
      {ok ? (
        <Check aria-hidden="true" />
      ) : busy ? (
        <Spinner aria-hidden="true" />
      ) : (
        <span className="pending-dot" aria-hidden="true" />
      )}
      {text}
    </li>
  );
  return (
    <>
      <div className="wizard-title">
        <div>
          <h2>
            {read
              ? t("projectSetup.firstDataReceived")
              : t("projectSetup.startingCollection")}
          </h2>
          <p>
            {read
              ? t("projectSetup.issuesLoadedTargetsAreCalculatedUsing", {
                  v0: created.count,
                  v1: setup.gates.phase,
                })
              : switched
                ? t("projectSetup.theBackgroundServiceIsLoadingIssues", {
                    v0: created.count,
                  })
                : t(
                    "projectSetup.stoppingThePreviousBackgroundServiceAndSwitchingToThe",
                  )}
          </p>
        </div>
      </div>
      <ol
        className="collect-steps"
        aria-label={t("projectSetup.startCollection")}
      >
        {line(true, false, t("projectSetup.writeProjectConfiguration"))}
        {line(
          switched,
          setup.activating,
          t("projectSetup.switchBackgroundServiceTo", { v0: created.project }),
        )}
        {line(
          read,
          setup.activating && switched,
          t("projectSetup.loadIssues", { v0: created.count }),
        )}
      </ol>
      {setup.activateFailure ? (
        <div className="collect-failure">
          <FailureNote failure={setup.activateFailure} />
          <Button
            variant="outline"
            disabled={setup.activating}
            onClick={() => void setup.start(created.project, created.scope)}
          >
            <ActionIcon name="refresh" />
            {t("projectSetup.restartCollection")}
          </Button>
        </div>
      ) : null}
      {read && ours && service?.gates.length ? (
        <GateResults gates={service.gates} targets={setup.gates} />
      ) : null}
      {ours ? <UnmappedAlert setup={setup} /> : null}
      {read && workspace && onOpenWorkbench ? (
        <p className="quiet">{t("projectSetup.workbenchHandoff")}</p>
      ) : null}
      <div className="collect-actions">
        {read && workspace && onOpenWorkbench ? (
          <ProjectWorkbenchButton seed={workspace} onOpen={onOpenWorkbench} />
        ) : null}
        <Button variant="outline" onClick={onDone}>
          <ActionIcon name="back" />
          {t("projectSetup.backToSetupChecklist")}
        </Button>
      </div>
    </>
  );
}

/** 新建 SPM 项目: one focused page from scope to the first snapshot. */
export function ProjectWizard({
  setup,
  projects,
  onOpenWorkbench,
  onExit,
}: {
  setup: ProjectSetup;
  projects?: ProjectStatus | null;
  onOpenWorkbench?: OpenProjectWorkbench;
  onExit: () => void;
}) {
  useTranslation();
  return (
    <div className="wizard">
      <Stepper setup={setup} />
      {setup.step === "project" ? (
        <ProjectStep setup={setup} />
      ) : setup.step === "gates" ? (
        <GatesStep setup={setup} />
      ) : setup.step === "confirm" ? (
        <ConfirmStep setup={setup} />
      ) : (
        <CollectStep
          setup={setup}
          projects={projects}
          onOpenWorkbench={onOpenWorkbench}
          onDone={onExit}
        />
      )}
    </div>
  );
}

/** The checklist's line for a project step: what is chosen, and the way in. */
export function ProjectStepSummary({
  setup,
  step,
  onOpen,
}: {
  setup: ProjectSetup;
  step: "project" | "gates" | "confirm" | "start";
  onOpen: () => void;
}) {
  useTranslation();
  const { preview, created, scope, service } = setup;
  const live = service?.mode === "live" && service.project;
  const text =
    step === "project"
      ? preview && scope
        ? t("projectSetup.issues3", { v0: describe(scope), v1: preview.count })
        : null
      : step === "gates"
        ? preview
          ? `${gateSentence(setup.gates)}。`
          : null
        : step === "confirm"
          ? created
            ? t("projectSetup.created", {
                v0: created.project,
                v1: created.scope,
              })
            : null
          : live
            ? t("projectSetup.collecting", {
                v0: service.project,
                v1: service.scope,
                v2: service.last_commit
                  ? t("projectSetup.lastRun", {
                      v0: new Date(service.last_commit).toLocaleTimeString(
                        locale(),
                        { hour: "2-digit", minute: "2-digit" },
                      ),
                    })
                  : "",
              })
            : setup.activateFailure
              ? t("projectSetup.collectionDidNotStart")
              : null;
  return (
    <div className="step-summary">
      {text ? <p>{text}</p> : null}
      {step === "start" ? <UnmappedAlert setup={setup} /> : null}
      {step === "project" ? (
        <Button size="sm" onClick={onOpen}>
          {preview
            ? t("projectSetup.continueCreatingProject")
            : t("projectSetup.newProject")}{" "}
          <ArrowRight />
        </Button>
      ) : null}
    </div>
  );
}
