import { useEffect, useRef, useState } from "react";
import { CircleCheck, Search } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
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
  Draft,
  DraftChoices,
  Group,
  Item,
  JiraProject,
  ScopeQuery,
  SetupFailure,
  Space,
  WorkItemType,
} from "./contracts/setup";
import { discover, draft } from "./api";
import { FailureNote, failureOf } from "./TemplateImport";

export type Source = "meegle" | "jira";
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
const stageNames: Record<string, string> = {
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
export function describe(scope: ScopeQuery) {
  if (scope.source === "meegle")
    return scope.filter
      ? `飞书项目「${scope.type_name}」中${scope.filter.field_name}为 ${scope.filter.label} 的`
      : `飞书项目「${scope.type_name}」中全部`;
  return scope.jql
    ? `Jira ${scope.project} 中符合 ${scope.jql} 的`
    : `Jira ${scope.project} 中全部`;
}
export function gateSentence(gates: Gates) {
  const parts = [
    `严重问题解决率不低于 ${gates.severe}%`,
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

type Lists = {
  spaces: Space[] | null;
  types: WorkItemType[] | null;
  groups: Group[] | null;
  items: Item[] | null;
  projects: JiraProject[] | null;
};

/** State shared by 选择项目, 设置门槛 and 确认; lives in App so it survives page changes. */
export function useProjectSetup(
  connected: Source[],
  onCreated: () => Promise<void>,
) {
  const [source, setSource] = useState<Source | null>(null);
  const [lists, setLists] = useState<Lists>({
    spaces: null,
    types: null,
    groups: null,
    items: null,
    projects: null,
  });
  const [loading, setLoading] = useState<keyof Lists | null>(null);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [space, setSpace] = useState<Space | null>(null);
  const [type, setType] = useState<WorkItemType | null>(null);
  const [group, setGroup] = useState<Group | null>(null);
  const [search, setSearch] = useState("");
  const [item, setItem] = useState<Item | null>(null);
  const [project, setProject] = useState<JiraProject | null>(null);
  const [jql, setJql] = useState("");
  const [preview, setPreview] = useState<Draft | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [previewFailure, setPreviewFailure] = useState<SetupFailure | null>(
    null,
  );
  const [gates, setGates] = useState<Gates>(defaultGates);
  const [ids, setIds] = useState({ project: "", scope: "main" });
  const [scopeComplete, setScopeComplete] = useState(false);
  const [stages, setStages] = useState<Record<string, string>>({});
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<Draft | null>(null);
  const [createFailure, setCreateFailure] = useState<SetupFailure | null>(null);
  const [changed, setChanged] = useState<Draft | null>(null);
  const generation = useRef<Record<string, number>>({});

  async function load<K extends keyof Lists>(
    key: K,
    work: () => Promise<Lists[K]>,
  ) {
    const n = (generation.current[key] = (generation.current[key] ?? 0) + 1);
    setLoading(key);
    setFailure(null);
    try {
      const value = await work();
      if (generation.current[key] === n)
        setLists((l) => ({ ...l, [key]: value }));
      return value;
    } catch (error) {
      if (generation.current[key] === n) setFailure(failureOf(error));
      return null;
    } finally {
      if (generation.current[key] === n) setLoading(null);
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
  useEffect(() => {
    setItem(null);
    setLists((l) => ({ ...l, items: null }));
    if (!space || !group) return;
    const timer = setTimeout(
      () =>
        void load(
          "items",
          async () =>
            (
              await discover("items", {
                space: space.simple_name,
                kind: group.target_type,
                search: search.trim() || undefined,
              })
            ).items ?? [],
        ),
      search ? 350 : 0,
    );
    return () => clearTimeout(timer);
  }, [group?.field, search]);

  const scope: ScopeQuery | null =
    source === "meegle" && space && type && group && item
      ? {
          source: "meegle",
          space: space.simple_name,
          type_key: type.type_key,
          work_item_type: type.api_name,
          type_name: type.name,
          filter: {
            field: group.field,
            field_name: group.name,
            id: item.id,
            label: item.name,
          },
        }
      : source === "jira" && project
        ? {
            source: "jira",
            project: project.key,
            ...(jql.trim() ? { jql: jql.trim() } : {}),
          }
        : null;
  const scopeKey = scope ? JSON.stringify(scope) : "";

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
  const label =
    scope?.source === "meegle"
      ? (scope.filter?.label ?? "")
      : (project?.name ?? project?.key ?? "");

  // The count and the open mapping questions come from a draft of the scope.
  useEffect(() => {
    setPreview(null);
    setPreviewFailure(null);
    setCreated(null);
    setChanged(null);
    setStages({});
    if (!scope) return;
    setIds((current) => ({ ...current, project: identifier(label) }));
    const n = (generation.current.preview =
      (generation.current.preview ?? 0) + 1);
    setPreviewing(true);
    const request = choices({});
    if (!request) return;
    draft({ ...request, project: identifier(label) })
      .then((d) => {
        if (generation.current.preview === n) setPreview(d);
      })
      .catch((error) => {
        if (generation.current.preview === n)
          setPreviewFailure(failureOf(error));
      })
      .finally(() => {
        if (generation.current.preview === n) setPreviewing(false);
      });
  }, [scopeKey]);

  const questions = preview?.questions ?? [];
  const answered = questions.every((q) => stages[q.value]);
  async function create() {
    const request = choices(stages);
    if (!request || !preview) return;
    setCreating(true);
    setCreateFailure(null);
    setChanged(null);
    try {
      const planned = await draft(request);
      if (planned.count !== preview.count || planned.questions.length) {
        setChanged(planned);
        setPreview(planned);
        return;
      }
      const done = await draft(request, planned.template.sha256);
      setCreated(done);
      await onCreated();
    } catch (error) {
      setCreateFailure(failureOf(error));
    } finally {
      setCreating(false);
    }
  }

  return {
    connected,
    source,
    setSource,
    lists,
    loading,
    failure,
    space,
    setSpace,
    type,
    setType,
    group,
    setGroup,
    search,
    setSearch,
    item,
    setItem,
    project,
    setProject,
    jql,
    setJql,
    scope,
    preview,
    previewing,
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
    answered,
    creating,
    created,
    createFailure,
    changed,
    create,
  };
}
export type ProjectSetup = ReturnType<typeof useProjectSetup>;

/** Rows at the list's own geometry while it loads. */
function SkeletonRows({ count = 5 }: { count?: number }) {
  const widths = [62, 45, 70, 52, 58, 40];
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

function Picker<T>({
  label,
  rows,
  selected,
  name,
  detail,
  onPick,
  loading,
}: {
  label: string;
  rows: T[] | null;
  selected: T | null;
  name: (row: T) => string;
  detail?: (row: T) => string;
  onPick: (row: T) => void;
  loading: boolean;
}) {
  if (loading || !rows) return <SkeletonRows />;
  if (!rows.length) return <p className="pick-empty">没有匹配的{label}</p>;
  return (
    <ScrollArea className="pick-scroll">
      <div className="pick-list" role="listbox" aria-label={label}>
        {rows.map((row) => {
          const active = selected !== null && name(row) === name(selected);
          return (
            <button
              type="button"
              role="option"
              aria-selected={active}
              className="pick-row"
              key={name(row) + (detail?.(row) ?? "")}
              onClick={() => onPick(row)}
            >
              <span>{name(row)}</span>
              {detail ? <small>{detail(row)}</small> : null}
            </button>
          );
        })}
      </div>
    </ScrollArea>
  );
}

function Choose<T>({
  label,
  rows,
  value,
  keyOf,
  nameOf,
  onChange,
}: {
  label: string;
  rows: T[];
  value: T | null;
  keyOf: (row: T) => string;
  nameOf: (row: T) => string;
  onChange: (row: T) => void;
}) {
  return (
    <Field className="choose">
      <FieldLabel>{label}</FieldLabel>
      <Select
        value={value ? keyOf(value) : null}
        items={rows.map((r) => ({ value: keyOf(r), label: nameOf(r) }))}
        onValueChange={(key) => {
          const row = rows.find((r) => keyOf(r) === key);
          if (row) onChange(row);
        }}
      >
        <SelectTrigger aria-label={label}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {rows.map((r) => (
            <SelectItem key={keyOf(r)} value={keyOf(r)}>
              {nameOf(r)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
  );
}

/** The one number this step produces, in a lane reserved from the start. */
function ScopeCount({ setup }: { setup: ProjectSetup }) {
  if (setup.previewFailure)
    return <FailureNote failure={setup.previewFailure} />;
  if (!setup.scope)
    return <p className="scope-hint">选择一个范围后统计其中的问题。</p>;
  return (
    <div className="scope-count" aria-live="polite">
      {setup.previewing || !setup.preview ? (
        <>
          <Skeleton className="count-skeleton" />
          <span className="scope-description">
            正在读取{describe(setup.scope)}问题…
          </span>
        </>
      ) : (
        <>
          <strong key={setup.preview.count}>{setup.preview.count}</strong>
          <span className="scope-description">
            个问题 · {describe(setup.scope)}
          </span>
        </>
      )}
    </div>
  );
}

/** 选择项目: a source, its grouping and one group, frozen into record IDs by the draft. */
export function ScopeStep({ setup }: { setup: ProjectSetup }) {
  const { lists, loading } = setup;
  const filtered =
    lists.projects?.filter((p) =>
      `${p.key} ${p.name}`.toLowerCase().includes(setup.search.toLowerCase()),
    ) ?? null;
  if (!setup.connected.length)
    return <p className="scope-hint">先连接飞书项目或 Jira。</p>;
  return (
    <div className="project-setup">
      {setup.connected.length > 1 ? (
        <ToggleGroup
          variant="outline"
          size="sm"
          value={setup.source ? [setup.source] : []}
          onValueChange={(value: string[]) => {
            const next = value[0] as Source | undefined;
            if (next) {
              setup.setSource(next);
              setup.setSearch("");
            }
          }}
          aria-label="问题来源"
        >
          {setup.connected.map((s) => (
            <ToggleGroupItem key={s} value={s}>
              {sourceNames[s]}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      ) : null}
      {setup.failure ? <FailureNote failure={setup.failure} /> : null}
      {setup.source === "meegle" ? (
        <>
          <div className="scope-filters">
            {lists.spaces && lists.spaces.length > 1 ? (
              <Choose
                label="空间"
                rows={lists.spaces}
                value={setup.space}
                keyOf={(s) => s.project_key}
                nameOf={(s) => s.name}
                onChange={setup.setSpace}
              />
            ) : null}
            {lists.types?.length ? (
              <Choose
                label="类型"
                rows={lists.types}
                value={setup.type}
                keyOf={(t) => t.type_key}
                nameOf={(t) => t.name}
                onChange={setup.setType}
              />
            ) : null}
            {lists.groups?.length ? (
              <Choose
                label="按"
                rows={lists.groups}
                value={setup.group}
                keyOf={(g) => g.field}
                nameOf={(g) => g.name}
                onChange={(g) => {
                  setup.setSearch("");
                  setup.setGroup(g);
                }}
              />
            ) : null}
          </div>
          <div className="pick-search">
            <Search aria-hidden="true" />
            <Input
              value={setup.search}
              placeholder={`搜索${setup.group?.target_name ?? ""}`}
              aria-label={`搜索${setup.group?.target_name ?? ""}`}
              maxLength={60}
              onChange={(e) => setup.setSearch(e.target.value)}
            />
          </div>
          <Picker
            label={setup.group?.target_name ?? "项目"}
            rows={lists.items}
            selected={setup.item}
            name={(i) => i.name}
            onPick={setup.setItem}
            loading={loading === "items" || (!lists.items && !setup.failure)}
          />
        </>
      ) : setup.source === "jira" ? (
        <>
          <div className="pick-search">
            <Search aria-hidden="true" />
            <Input
              value={setup.search}
              placeholder="搜索 Jira 项目"
              aria-label="搜索 Jira 项目"
              onChange={(e) => setup.setSearch(e.target.value)}
            />
          </div>
          <Picker
            label="Jira 项目"
            rows={filtered}
            selected={setup.project}
            name={(p) => p.key}
            detail={(p) => p.name}
            onPick={setup.setProject}
            loading={loading === "projects"}
          />
          <JqlField setup={setup} />
        </>
      ) : null}
      <ScopeCount setup={setup} />
    </div>
  );
}

/** JQL applies on Enter or blur, so typing does not re-read the scope per keystroke. */
function JqlField({ setup }: { setup: ProjectSetup }) {
  const [text, setText] = useState(setup.jql);
  const commit = () => setup.setJql(text.trim());
  return (
    <Field>
      <FieldLabel htmlFor="scope-jql">筛选</FieldLabel>
      <Input
        id="scope-jql"
        value={text}
        placeholder="issuetype = 缺陷"
        maxLength={500}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
      />
      <FieldDescription>JQL，可留空</FieldDescription>
    </Field>
  );
}

function NumberField({
  id,
  label,
  unit,
  value,
  max,
  onChange,
}: {
  id: string;
  label: string;
  unit: string;
  value: number;
  max?: number;
  onChange: (n: number) => void;
}) {
  return (
    <Field>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <div className="unit-input">
        <Input
          id={id}
          type="number"
          inputMode="numeric"
          min={0}
          max={max}
          step={1}
          value={Number.isFinite(value) ? value : ""}
          onChange={(e) => onChange(e.target.valueAsNumber)}
        />
        <span>{unit}</span>
      </div>
    </Field>
  );
}

/** 设置门槛: the policy the snapshot is judged against. */
export function GateStep({ setup }: { setup: ProjectSetup }) {
  const { gates, setGates } = setup;
  const set = (patch: Partial<Gates>) => setGates({ ...gates, ...patch });
  return (
    <div className="project-setup">
      <FieldGroup className="gate-fields">
        <NumberField
          id="gate-severe"
          label="严重问题解决率"
          unit="%"
          max={100}
          value={gates.severe}
          onChange={(severe) => set({ severe })}
        />
        <NumberField
          id="gate-overall"
          label="整体解决率"
          unit="%"
          max={100}
          value={gates.overall}
          onChange={(overall) => set({ overall })}
        />
        <NumberField
          id="gate-fatal"
          label="致命问题上限"
          unit="个"
          value={gates.fatal}
          onChange={(fatal) => set({ fatal })}
        />
        <Field>
          <FieldLabel>阶段</FieldLabel>
          <Select
            value={gates.phase}
            onValueChange={(phase) => phase && set({ phase: phase as string })}
          >
            <SelectTrigger aria-label="阶段">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {phases.map((p) => (
                <SelectItem key={p} value={p}>
                  {p}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor="gate-milestone">里程碑日期</FieldLabel>
          <Input
            id="gate-milestone"
            type="date"
            value={gates.milestone}
            onChange={(e) => set({ milestone: e.target.value })}
          />
        </Field>
      </FieldGroup>
      <p className="gate-sentence">
        {validGates(gates)
          ? `${gateSentence(gates)}。`
          : "门槛需为 0–100 的整数。"}
      </p>
    </div>
  );
}

/** 确认: one sentence, the open mapping questions, then the project is written. */
export function ConfirmStep({ setup }: { setup: ProjectSetup }) {
  const { preview, scope, created } = setup;
  if (created)
    return (
      <Alert className="alert-success">
        <CircleCheck />
        <AlertDescription>
          已创建 {created.project} · {created.scope}，{created.count} 个问题
          {created.check?.ready ? "，配置检查通过" : ""}。
        </AlertDescription>
      </Alert>
    );
  if (!scope || !preview)
    return <p className="scope-hint">选择项目后在这里确认。</p>;
  const ready = validGates(setup.gates) && setup.answered && !setup.creating;
  return (
    <div className="project-setup">
      <p className="confirm-sentence">
        {identifier(setup.ids.project)} · {identifier(setup.ids.scope)}：
        {describe(scope)} {preview.count} 个问题；{gateSentence(setup.gates)}。
      </p>
      <FieldGroup className="id-fields">
        <Field>
          <FieldLabel htmlFor="draft-project">项目</FieldLabel>
          <Input
            id="draft-project"
            value={setup.ids.project}
            maxLength={64}
            onChange={(e) =>
              setup.setIds({ ...setup.ids, project: e.target.value })
            }
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="draft-scope">范围</FieldLabel>
          <Input
            id="draft-scope"
            value={setup.ids.scope}
            maxLength={64}
            onChange={(e) =>
              setup.setIds({ ...setup.ids, scope: e.target.value })
            }
          />
        </Field>
      </FieldGroup>
      {setup.questions.length ? (
        <div className="questions" role="group" aria-label="状态对应">
          <p>这些状态没有自动对应，请选择：</p>
          {setup.questions.map((q) => (
            <div className="question" key={q.field + q.value}>
              <span>{q.value}</span>
              <Select
                value={setup.stages[q.value] ?? null}
                items={Object.entries(
                  q.field === "severity" ? severityNames : stageNames,
                ).map(([value, label]) => ({ value, label }))}
                onValueChange={(stage) =>
                  stage &&
                  setup.setStages({
                    ...setup.stages,
                    [q.value]: stage as string,
                  })
                }
              >
                <SelectTrigger aria-label={`${q.value} 对应`}>
                  <SelectValue placeholder="选择" />
                </SelectTrigger>
                <SelectContent>
                  {Object.entries(
                    q.field === "severity" ? severityNames : stageNames,
                  ).map(([value, label]) => (
                    <SelectItem key={value} value={value}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          ))}
        </div>
      ) : null}
      <Field orientation="horizontal" className="complete-field">
        <Checkbox
          id="scope-complete"
          checked={setup.scopeComplete}
          onCheckedChange={(checked) =>
            setup.setScopeComplete(checked === true)
          }
        />
        <FieldLabel htmlFor="scope-complete">这是本次交付的全部问题</FieldLabel>
      </Field>
      <div className="confirm-lane" aria-live="polite">
        {setup.createFailure ? (
          <FailureNote failure={setup.createFailure} />
        ) : setup.changed ? (
          <p>范围已更新为 {setup.changed.count} 个问题，请再次确认。</p>
        ) : null}
      </div>
      <div className="import-actions">
        <Button disabled={!ready} onClick={() => void setup.create()}>
          {setup.creating ? <Spinner aria-hidden="true" /> : null}
          创建项目
        </Button>
      </div>
    </div>
  );
}
