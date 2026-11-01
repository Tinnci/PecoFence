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

export type Source = "meegle" | "jira";
export type Step = "project" | "gates" | "confirm" | "collect";
export const sourceNames: Record<Source, string> = {
  meegle: "飞书项目",
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
  Open: "待处理",
  Implementing: "修改中",
  Reviewing: "审核中",
  Regression: "回归测试",
  Closed: "已关闭",
  WaitInfo: "等待信息",
  Track: "跟踪",
  Ccb: "CCB 评审",
};
const severityNames: Record<string, string> = {
  Fatal: "致命",
  Severe: "严重",
  Normal: "一般",
  Hint: "提示",
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
      ? `飞书项目「${scope.type_name}」中${scope.filter.field_name}为 ${filterLabel(scope)} 的`
      : `飞书项目「${scope.type_name}」中全部`;
  if (scope.label) return `Jira ${scope.project} 中${scope.label}的`;
  return scope.jql
    ? `Jira ${scope.project} 中符合 ${scope.jql} 的`
    : `Jira ${scope.project} 中全部`;
}
export function gateSentence(gates: Gates) {
  const parts = [
    `严重问题解决率不低于 ${gates.severe}%`,
    `整体不低于 ${gates.overall}%`,
    `致命问题最多 ${gates.fatal} 个`,
    `阶段 ${gates.phase}`,
  ];
  if (gates.milestone) parts.push(`里程碑 ${gates.milestone}`);
  return parts.join("，");
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
                      ? `影响版本为 ${jiraGroup.name} `
                      : `筛选器「${jiraGroup.name}」中`,
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
      title: "选择项目",
      done: preview ? `${name} · ${preview.count}` : undefined,
    },
    {
      id: "gates",
      title: "设置门槛",
      done: `${setup.gates.phase} · ${setup.gates.severe}%`,
    },
    { id: "confirm", title: "确认" },
    { id: "collect", title: "开始采集" },
  ];
  return (
    <ol className="wizard-steps" aria-label="新建项目步骤">
      <li className="wizard-step done">
        <Check aria-hidden="true" />
        连接来源
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
  const { lists, group } = setup;
  // 关联项目 → 项目; 影响版本 stays.
  const target = group ? group.name.replace(/^关联/, "") || group.name : "项目";
  const query = setup.search.trim().toLowerCase();
  const rows = (lists.items ?? []).filter((i) =>
    i.name.toLowerCase().includes(query),
  );
  return (
    <>
      <div className="picker-head">
        {lists.groups?.length ? (
          <Chips
            label="分组"
            value={group?.field ?? null}
            options={lists.groups
              .slice(0, 4)
              .map((g) => ({ value: g.field, title: g.name }))}
            onChange={(field) =>
              setup.setGroup(
                lists.groups?.find((g) => g.field === field) ?? null,
              )
            }
          />
        ) : null}
        <SearchBox
          value={setup.search}
          label={`搜索${target}`}
          onChange={setup.setSearch}
        />
      </div>
      <div className="pick-columns" aria-hidden="true">
        <span>{target}</span>
        <span>问题</span>
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
              label={`没有问题的${target}`}
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
          <p className="pick-empty">没有匹配的{target}</p>
        )}
      </div>
      <div className="picker-foot">
        <span>
          {setup.picked.length > 1
            ? `已选 ${setup.picked.length} 个，合并为一个范围`
            : lists.items
              ? `${lists.items.length} 个${target}有问题，按问题数排列 · 按住 Ctrl 点击可合并多个`
              : ""}
        </span>
        {lists.withoutIssues ? (
          <Button
            variant="link"
            size="sm"
            onClick={() => setup.setShowEmpty(!setup.showEmpty)}
          >
            {setup.showEmpty
              ? "只看有问题的"
              : `显示没有问题的 ${lists.withoutIssues} 个${target}`}
          </Button>
        ) : null}
      </div>
    </>
  );
}

function JiraPicker({ setup }: { setup: ProjectSetup }) {
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
            label="搜索 Jira 项目"
            onChange={setup.setSearch}
          />
        </div>
        <div className="pick-scroll">
          {rows === null ? (
            <SkeletonRows />
          ) : (
            <CountList
              label="Jira 项目"
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
          label="范围"
          value={setup.jiraMode}
          options={[
            { value: "all", title: "整个项目" },
            { value: "versions", title: "影响版本" },
            { value: "filters", title: "我的筛选器" },
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
            label={setup.jiraMode === "versions" ? "搜索版本" : "搜索筛选器"}
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
              label={setup.jiraMode === "versions" ? "影响版本" : "筛选器"}
              rows={groups}
              selected={(g) => g.name === setup.jiraGroup?.name}
              name={(g) => g.name}
              count={(g) => g.count}
              onPick={setup.setJiraGroup}
            />
          ) : (
            <p className="pick-empty">
              {setup.jiraMode === "filters"
                ? "你还没有收藏筛选器。在 Jira 中收藏后，这里会列出来。"
                : "这个项目没有版本。"}
            </p>
          )}
        </div>
      ) : setup.jiraMode === "jql" ? (
        <JqlField setup={setup} />
      ) : (
        <p className="pick-empty">跟踪 {project.key} 的全部问题。</p>
      )}
    </>
  );
}

/** JQL applies on Enter or blur, so typing does not re-read the scope per keystroke. */
function JqlField({ setup }: { setup: ProjectSetup }) {
  const [text, setText] = useState(setup.jql);
  const commit = () => setup.setJql(text.trim());
  return (
    <label className="jql-field">
      <span>JQL，在项目内筛选</span>
      <Input
        value={text}
        placeholder="issuetype = 缺陷"
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
      <span className="preview-label">严重度</span>
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
  const m = draft.breakdown.measures;
  const top = draft.breakdown.statuses
    .slice(0, 6)
    .map((s) => `${s.value} ${s.count}`)
    .join(" · ");
  return (
    <div className="preview-block">
      <span className="preview-label">状态</span>
      <div className="status-line">
        <span>已解决</span>
        <strong>{m.overall_resolved}</strong>
      </div>
      <div className="status-line">
        <span>未解决</span>
        <strong>{m.overall_total - m.overall_resolved}</strong>
      </div>
      <small>{top}</small>
    </div>
  );
}

/** Before a pick: what the list holds, so the panel is never an empty card. */
function Overview({ setup }: { setup: ProjectSetup }) {
  const { lists, group, project } = setup;
  const noun = group ? group.name.replace(/^关联/, "") || group.name : "项目";
  const [label, total, where, hint] =
    setup.source === "meegle"
      ? [
          `飞书项目 · ${setup.space?.name ?? ""}`,
          lists.items?.reduce((n, i) => n + (i.count ?? 0), 0),
          `个问题分布在 ${lists.items?.length ?? 0} 个${noun}`,
          `选择一个${noun}，或按住 Ctrl 合并多个。`,
        ]
      : project
        ? [
            `Jira · ${project.key}`,
            project.count,
            `个问题在 ${project.name}`,
            "选择整个项目、一个版本或一个筛选器。",
          ]
        : [
            "Jira",
            lists.projects?.reduce((n, p) => n + p.count, 0),
            `个问题分布在 ${lists.projects?.length ?? 0} 个项目`,
            "选择一个项目。",
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
  const { preview, scope } = setup;
  const heading =
    scope?.source === "meegle"
      ? setup.group?.name
      : setup.jiraMode === "versions"
        ? "影响版本"
        : setup.jiraMode === "filters"
          ? "筛选器"
          : "Jira 项目";
  const name =
    scope?.source === "meegle"
      ? filterLabel(scope)
      : (setup.jiraGroup?.name ?? setup.project?.key);
  return (
    <aside className="wizard-aside" aria-label="范围预览" aria-live="polite">
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
              <span className="preview-label">正在读取严重度和状态…</span>
              <Skeleton className="bar-skeleton" />
              <Skeleton style={{ width: "70%" }} />
              <Skeleton style={{ width: "52%" }} />
            </div>
          ) : (
            <>
              <div className="big-count">
                <strong key={preview.count}>{preview.count}</strong>
                <span>个问题</span>
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
            ? `读取于 ${setup.previewAt.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`
            : ""}
        </span>
        <Button disabled={!preview} onClick={() => setup.setStep("gates")}>
          设置门槛 <ArrowRight />
        </Button>
      </div>
    </aside>
  );
}

function ProjectStep({ setup }: { setup: ProjectSetup }) {
  return (
    <>
      <div className="wizard-title">
        <div>
          <h2>选择项目</h2>
          <p>选中的范围会冻结为具体的问题单，之后可以刷新。</p>
        </div>
        {setup.connected.length > 1 ? (
          <Chips<Source>
            label="问题来源"
            value={setup.source}
            options={setup.connected.map((s) => ({
              value: s,
              title:
                s === "meegle" && setup.space
                  ? `飞书项目 · ${setup.space.name}`
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
        <section className="wizard-main picker" aria-label="范围">
          {setup.source === "meegle" ? (
            <MeeglePicker setup={setup} />
          ) : setup.source === "jira" ? (
            <JiraPicker setup={setup} />
          ) : (
            <p className="scope-hint">先连接飞书项目或 Jira。</p>
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
  const clamp = (n: number) => Math.min(max, Math.max(0, n));
  return (
    <div className="stepper" role="group" aria-label={label}>
      <Button
        variant="outline"
        size="icon-sm"
        aria-label={`降低${label}`}
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
        aria-label={`提高${label}`}
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
  return <span className={ok ? "verdict ok" : "verdict short"}>{text}</span>;
}

function GatesStep({ setup }: { setup: ProjectSetup }) {
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
          <h2>设置门槛</h2>
          <p>
            按现在的 {preview.count}{" "}
            个问题即时计算，研发交付（回归测试或关闭）即算解决。
          </p>
        </div>
      </div>
      <div className="wizard-columns">
        <section className="wizard-main gates" aria-label="门槛">
          <article className="gate-card" aria-label="严重问题解决率">
            <div className="gate-head">
              <strong>严重问题解决率</strong>
              <span>严重 {m.severe_total} 个，致命计入严重</span>
            </div>
            <TargetStepper
              label="严重问题解决率"
              value={gates.severe}
              unit="%"
              max={100}
              onChange={(severe) => set({ severe })}
            />
            <Meter now={j.severe.now} target={gates.severe} ok={j.severe.ok} />
            <div className="gate-now">
              <span>
                现在 {j.severe.now}%（{j.severe.v} / {j.severe.n}）
              </span>
              <Verdict
                ok={j.severe.ok}
                text={j.severe.ok ? "达标" : `还差 ${j.severe.gap} 个`}
              />
            </div>
          </article>
          <article className="gate-card" aria-label="整体解决率">
            <div className="gate-head">
              <strong>整体解决率</strong>
              <span>全部 {m.overall_total} 个，重复和不修改不算解决</span>
            </div>
            <TargetStepper
              label="整体解决率"
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
                现在 {j.overall.now}%（{j.overall.v} / {j.overall.n}）
              </span>
              <Verdict
                ok={j.overall.ok}
                text={j.overall.ok ? "达标" : `还差 ${j.overall.gap} 个`}
              />
            </div>
          </article>
          <article className="gate-card" aria-label="致命问题上限">
            <div className="gate-head">
              <strong>致命问题上限</strong>
              <span>未解决的致命问题</span>
            </div>
            <TargetStepper
              label="致命问题上限"
              value={gates.fatal}
              unit=" 个"
              max={999}
              onChange={(fatal) => set({ fatal })}
            />
            <div className="gate-now">
              <span>现在 {j.fatal.open} 个未解决</span>
              <Verdict
                ok={j.fatal.ok}
                text={
                  j.fatal.ok ? "达标" : `超出 ${j.fatal.open - gates.fatal} 个`
                }
              />
            </div>
          </article>
        </section>
        <aside className="wizard-aside">
          <div className="aside-block">
            <span className="preview-label">阶段</span>
            <div className="phase-grid" role="group" aria-label="阶段">
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
            <span className="preview-label">里程碑日期</span>
            <Input
              type="date"
              value={gates.milestone}
              onChange={(e) => set({ milestone: e.target.value })}
            />
            {days !== null ? (
              <small>
                {days >= 0 ? `还有 ${days} 天` : `已过 ${-days} 天`}
              </small>
            ) : null}
          </label>
          <p className="gate-sentence">{gateSentence(gates)}。</p>
          <div className="aside-foot">
            <Button variant="ghost" onClick={() => setup.setStep("project")}>
              <ArrowLeft /> 返回
            </Button>
            <Button
              disabled={!validGates(gates)}
              onClick={() => setup.setStep("confirm")}
            >
              确认 <ArrowRight />
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
      <SelectTrigger size="sm" aria-label={`${value} 对应`}>
        <SelectValue placeholder={optional ? "稍后再定" : "选择"} />
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
  const { preview, scope } = setup;
  if (!preview || !scope) return null;
  const blockingValues = new Set(setup.blocking.map((q) => q.value));
  const later = setup.questions.filter((q) => q.count === 0);
  const ready = validGates(setup.gates) && setup.answered && !setup.creating;
  return (
    <>
      <div className="wizard-title">
        <div>
          <h2>确认</h2>
          <p className="confirm-sentence">
            {identifier(setup.ids.project)} · {identifier(setup.ids.scope)} 跟踪
            {describe(scope)} {preview.count} 个问题；
            {gateSentence(setup.gates)}。
          </p>
        </div>
      </div>
      <div className="wizard-columns">
        <section className="wizard-main mapping" aria-label="状态对应">
          <div className="mapping-head">
            <strong>状态对应</strong>
            <span>严重度取「{preview.mapping.severity_from}」</span>
          </div>
          <table>
            <thead>
              <tr>
                <th>
                  {scope.source === "meegle" ? "飞书项目状态" : "Jira 状态"}
                </th>
                <th>问题</th>
                <th>SPM 阶段</th>
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
                {later.map((q) => q.value).join(" 和 ")}{" "}
                没有自动对应。本范围里没有这些状态的问题，可以先跳过；出现时，设置清单的“开始采集”会请你选择。
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
              <span className="preview-label">项目</span>
              <Input
                value={setup.ids.project}
                maxLength={64}
                onChange={(e) =>
                  setup.setIds({ ...setup.ids, project: e.target.value })
                }
              />
            </label>
            <label>
              <span className="preview-label">范围</span>
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
              这是本次交付的全部问题
              <small>不勾选时，看板会提示范围可能不完整。</small>
            </span>
          </label>
          <div className="aside-note">
            <span>写入 {preview.directory}</span>
            <span>采集每 5 分钟一次，由后台服务 spmd 完成</span>
          </div>
          <div className="confirm-lane" aria-live="polite">
            {setup.createFailure ? (
              <FailureNote failure={setup.createFailure} />
            ) : setup.changed ? (
              <p>范围已更新为 {setup.changed.count} 个问题，请再次确认。</p>
            ) : null}
          </div>
          <div className="aside-foot">
            <Button variant="ghost" onClick={() => setup.setStep("gates")}>
              <ArrowLeft /> 返回
            </Button>
            <Button disabled={!ready} onClick={() => void setup.create()}>
              {setup.creating ? <Spinner aria-hidden="true" /> : null}
              创建并开始采集
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
          <Verdict ok={ok} text={ok ? "达标" : `还差 ${g.gap} 个`} />
        </div>
        <span className="result-number">{now}%</span>
        <Meter now={now} target={target} ok={ok} />
        <small>
          {g.v} / {g.n} · 目标 {target}%
        </small>
      </article>
    );
  };
  const fatal = find("fatal");
  return (
    <div className="gate-results">
      {rate("severe", "严重问题解决率", targets.severe)}
      {rate("overall", "整体解决率", targets.overall)}
      {fatal ? (
        <article className="gate-card" aria-label="致命问题">
          <div className="gate-head row">
            <strong>致命问题</strong>
            <Verdict
              ok={fatal.gap === 0}
              text={fatal.gap === 0 ? "达标" : `超出 ${fatal.gap} 个`}
            />
          </div>
          <span className="result-number">{fatal.gap + targets.fatal}</span>
          <small>未解决的致命问题 · 上限 {targets.fatal} 个</small>
        </article>
      ) : null}
    </div>
  );
}

/** Statuses that issues took after creation; answering restarts collection. */
export function UnmappedAlert({ setup }: { setup: ProjectSetup }) {
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
        {setup.service.project} 有 {total}{" "}
        个问题处于没有对应的状态，暂按“未知”计算。选择对应的 SPM
        阶段后会重新开始采集。
      </p>
      <div className="later-selects">
        {values.map((u) => (
          <label key={u.value}>
            {u.value} · {u.count} 个
            <Select
              value={answers[u.value] ?? null}
              items={options}
              onValueChange={(stage) =>
                stage && setAnswers({ ...answers, [u.value]: stage as string })
              }
            >
              <SelectTrigger size="sm" aria-label={`${u.value} 对应`}>
                <SelectValue placeholder="选择" />
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
          保存并重新开始采集
        </Button>
      </div>
    </div>
  );
}

function CollectStep({
  setup,
  onDone,
}: {
  setup: ProjectSetup;
  onDone: () => void;
}) {
  const { created, service } = setup;
  if (!created) return null;
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
          <h2>{read ? "第一批数据已到" : "正在开始采集"}</h2>
          <p>
            {read
              ? `${created.count} 个问题已读取，门槛按 ${setup.gates.phase} 计算。`
              : switched
                ? `后台服务正在读取 ${created.count} 个问题。`
                : "正在停止原来的后台服务，换成新项目。"}
          </p>
        </div>
      </div>
      <ol className="collect-steps" aria-label="开始采集">
        {line(true, false, "写入项目配置")}
        {line(switched, setup.activating, `后台服务切换到 ${created.project}`)}
        {line(
          read,
          setup.activating && switched,
          `读取 ${created.count} 个问题`,
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
            重新开始采集
          </Button>
        </div>
      ) : null}
      {read && ours && service?.gates.length ? (
        <GateResults gates={service.gates} targets={setup.gates} />
      ) : null}
      {ours ? <UnmappedAlert setup={setup} /> : null}
      <div className="collect-actions">
        <Button variant={read ? "default" : "outline"} onClick={onDone}>
          返回设置清单
        </Button>
      </div>
    </>
  );
}

/** 新建 SPM 项目: one focused page from scope to the first snapshot. */
export function ProjectWizard({
  setup,
  onExit,
}: {
  setup: ProjectSetup;
  onExit: () => void;
}) {
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
        <CollectStep setup={setup} onDone={onExit} />
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
  const { preview, created, scope, service } = setup;
  const live = service?.mode === "live" && service.project;
  const text =
    step === "project"
      ? preview && scope
        ? `${describe(scope)} ${preview.count} 个问题`
        : null
      : step === "gates"
        ? preview
          ? `${gateSentence(setup.gates)}。`
          : null
        : step === "confirm"
          ? created
            ? `已创建 ${created.project} · ${created.scope}`
            : null
          : live
            ? `采集中：${service.project} · ${service.scope}${
                service.last_commit
                  ? `，最近一次 ${new Date(service.last_commit).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`
                  : ""
              }`
            : setup.activateFailure
              ? "开始采集没有完成"
              : null;
  return (
    <div className="step-summary">
      {text ? <p>{text}</p> : null}
      {step === "start" ? <UnmappedAlert setup={setup} /> : null}
      {step === "project" ? (
        <Button size="sm" onClick={onOpen}>
          {preview ? "继续新建项目" : "新建项目"} <ArrowRight />
        </Button>
      ) : null}
    </div>
  );
}
