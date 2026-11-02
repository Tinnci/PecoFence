import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { RefreshCw, Tag } from "lucide-react";
import { Toggle } from "@base-ui/react/toggle";
import { ToggleGroup } from "@base-ui/react/toggle-group";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { ExternalLink } from "@/components/external-link";
import type {
  Correction,
  FocusModel,
  SetupFailure,
  TrackingSource,
  WorkItem,
  WorkItems,
} from "./contracts/setup";
import { focus } from "./api";
import {
  prefetchWorkItems,
  queryClient,
  workItemsOptions,
  type WorkKind,
} from "./focus-cache";
import { since, type FocusState } from "./Focus";
import { FailureNote, failureOf } from "./TemplateImport";
import { useModelPreference } from "./model-navigation";
import { t } from "./i18n";

/** Stages in the order work moves; done and cancelled close the item. */
export const stages = [
  "pending",
  "active",
  "review",
  "testing",
  "blocked",
  "unknown",
  "done",
  "cancelled",
] as const;
const closed = new Set(["done", "cancelled"]);
const STALE_DAYS = 14;
const fields = [
  "found_version",
  "target_version",
  "severity",
  "owner",
  "stage",
  "origin",
] as const;
type Field = (typeof fields)[number];

/** Jira writes ISO times with +0800; SPM writes 飞书项目 times as RFC 3339 (+08:00), and older
 * reads kept its zoneless China time ("2026-11-02 13:51:04", "2026-10-30"). */
export function updatedAt(value: string) {
  if (/^\d{10,}$/.test(value)) return Number(value);
  const china = /^(\d{4}-\d\d-\d\d)(?:[ T](\d\d:\d\d(?::\d\d)?))?$/.exec(
    value.trim(),
  );
  const text = china
    ? `${china[1]}T${china[2] ?? "00:00:00"}+08:00`
    : value.replace(/([+-]\d\d)(\d\d)$/, "$1:$2");
  const parsed = Date.parse(text);
  return Number.isFinite(parsed) ? parsed : null;
}
export function ageDays(value: string, now = Date.now()) {
  const at = updatedAt(value);
  return at === null ? null : Math.floor((now - at) / 86_400_000);
}
/** The last status change or human comment; `updated` moves on any edit, bots included. */
export const lastActivity = (item: WorkItem) =>
  item.activity.last_activity || item.updated;
export const isStale = (item: WorkItem, now = Date.now()) =>
  !closed.has(item.group) &&
  (ageDays(lastActivity(item), now) ?? 0) >= STALE_DAYS;
const today = () => new Date().toLocaleDateString("sv-SE");
export const isDue = (item: WorkItem, day = today()) =>
  !!item.local?.next_check && item.local.next_check <= day;

/** A build name's version and date: LAVA_LXX516_SW_V_V14_HW_V1.0_20260923_user → V14 · 09-23. */
export function shortVersion(name: string) {
  const version = /(?:^|_)(V\d+(?:\.\d+)?)(?=_|$)/i.exec(name)?.[1];
  const date = /_(20\d{2})(\d{2})(\d{2})(?:\d{2})?(?=_|$)/.exec(name);
  if (!version && !date)
    return name.length > 28 ? `${name.slice(0, 26)}…` : name;
  return [version, date ? `${date[2]}-${date[3]}` : ""]
    .filter(Boolean)
    .join(" · ");
}
/** What a field shows now, and what the source says. */
function shown(item: WorkItem, field: Field) {
  switch (field) {
    case "found_version":
      return item.activity.found_versions.join(", ");
    case "target_version":
      return item.activity.target_versions.join(", ");
    case "severity":
      return item.severity;
    case "owner":
      return item.owner;
    case "stage":
      return item.group;
    case "origin":
      return item.origin;
  }
}
export function remoteOf(item: WorkItem, field: Field) {
  const c = item.local?.corrections.find((x) => x.field === field);
  return c ? c.remote_now : shown(item, field);
}

export function useWorkItems(code: string, kind: WorkKind, enabled = true) {
  const query = useQuery(
    { ...workItemsOptions(code, kind), enabled: false },
    queryClient,
  );
  useEffect(() => {
    if (enabled) void prefetchWorkItems(code, kind).catch(() => {});
  }, [code, kind, enabled]);
  return {
    data: query.data ?? null,
    loading: query.isFetching,
    failure: query.error ? failureOf(query.error) : null,
    refresh: () => query.refetch(),
    openCount: query.data
      ? query.data.items.filter((i) => !closed.has(i.group)).length
      : null,
  };
}

/** After a local change: the cached read again, with the layer laid over it. */
async function reloadLocal(code: string, kind: WorkKind) {
  const cached = (await focus(`${kind}_cached`, { code })).work_items;
  if (cached?.code === code)
    queryClient.setQueryData<WorkItems>(
      workItemsOptions(code, kind).queryKey,
      cached,
    );
}

/** A single-choice chip group (Base UI ToggleGroup: arrow keys, one Tab stop, aria-pressed).
 * Pressing the chosen chip again falls back to `empty`. */
function Choices({
  label,
  value,
  empty,
  onChange,
  className,
  children,
}: {
  label: string;
  value: string;
  empty: string;
  onChange: (value: string) => void;
  className: string;
  children: ReactNode;
}) {
  return (
    <ToggleGroup
      aria-label={label}
      className={className}
      value={[value]}
      onValueChange={(next: string[]) => onChange(next[0] ?? empty)}
    >
      {children}
    </ToggleGroup>
  );
}
/** A hover and focus hint over inline text. */
function Hint({
  text,
  children,
  className,
}: {
  text: string;
  children: ReactNode;
  className?: string;
}) {
  if (!text) return <span className={className}>{children}</span>;
  return (
    <Tooltip>
      <TooltipTrigger render={<span className={className} tabIndex={0} />}>
        {children}
      </TooltipTrigger>
      <TooltipContent>{text}</TooltipContent>
    </Tooltip>
  );
}

/** Open work by stage: the bar shares out only what is still open; closed counts sit apart. */
function StageBar({
  items,
  stage,
  onStage,
}: {
  items: WorkItem[];
  stage: string;
  onStage: (stage: string) => void;
}) {
  const counts = new Map<string, number>();
  for (const i of items) counts.set(i.group, (counts.get(i.group) ?? 0) + 1);
  const open = items.filter((i) => !closed.has(i.group)).length;
  const openStages = stages.filter((s) => !closed.has(s) && counts.get(s));
  const radio = (key: string, label: ReactNode) => (
    <Toggle key={key} value={key}>
      {label}
    </Toggle>
  );
  return (
    <div className="stage-bar">
      {open ? (
        <div className="stage-track" aria-hidden="true">
          {openStages.map((s) => (
            <span
              key={s}
              className={`stage-${s}`}
              style={{ flexGrow: counts.get(s) }}
              title={`${t(`work.stage.${s}`)} ${counts.get(s)}`}
              data-dim={stage !== "open" && stage !== s ? "" : undefined}
            />
          ))}
        </div>
      ) : null}
      <Choices
        className="stage-legend"
        label={t("work.stage")}
        value={stage}
        empty="open"
        onChange={onStage}
      >
        {radio(
          "open",
          <>
            {t("work.open")} <strong>{open}</strong>
          </>,
        )}
        {openStages.map((s) =>
          radio(
            s,
            <>
              <i className={`stage-dot stage-${s}`} />
              {t(`work.stage.${s}`)} <strong>{counts.get(s)}</strong>
            </>,
          ),
        )}
        <span className="stage-closed">
          {stages
            .filter((s) => closed.has(s) && counts.get(s))
            .map((s) =>
              radio(
                s,
                <>
                  {t(`work.stage.${s}`)} {counts.get(s)}
                </>,
              ),
            )}
          {radio(
            "all",
            <>
              {t("work.all")} {items.length}
            </>,
          )}
        </span>
      </Choices>
    </div>
  );
}

/** What each source read and how software was decided. */
function SourceLine({ source }: { source: TrackingSource }) {
  return (
    <div className="work-source">
      <span>{source.label}</span>
      <span className="quiet">
        {source.software_filter} ·{" "}
        {t("work.counts", {
          total: source.total,
          matched: source.matched,
          excluded: source.excluded,
        })}
        {source.unclassified ? (
          <span className="short">
            {" "}
            · {t("work.unclassified", { count: source.unclassified })}
          </span>
        ) : null}
      </span>
      {source.error ? (
        <span className="short">
          {source.stale ? t("work.staleSource") : ""}
          {source.error}
        </span>
      ) : null}
    </div>
  );
}

/** Which Jira domain values count as software; saved to the model. */
function Classify({
  source,
  saved,
  state,
  onSaved,
}: {
  source: TrackingSource;
  saved: FocusModel;
  state: FocusState;
  onSaved: () => void;
}) {
  const c = source.classification!;
  const initial = useMemo(
    () => new Set(c.values.filter((v) => v.software).map((v) => v.value)),
    [c],
  );
  const [picked, setPicked] = useState(initial);
  const [saving, setSaving] = useState(false);
  useEffect(() => setPicked(initial), [initial]);
  const dirty =
    picked.size !== initial.size || [...picked].some((v) => !initial.has(v));
  async function save() {
    setSaving(true);
    try {
      await state.save({
        ...saved,
        tracking: {
          ...(saved.tracking ?? {}),
          jira_software_field: c.field,
          jira_software_values: c.values
            .map((v) => v.value)
            .filter((v) => picked.has(v)),
        },
      });
      onSaved();
    } finally {
      setSaving(false);
    }
  }
  return (
    <Collapsible className="classify">
      <CollapsibleTrigger className="classify-trigger">
        {t("work.classify", { field: c.field_name })}
        <span className="quiet">
          {c.configured ? t("work.classifySaved") : t("work.classifySuggested")}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="classify-values">
          {c.values.map((v) => (
            <button
              type="button"
              key={v.value}
              role="switch"
              aria-checked={picked.has(v.value)}
              onClick={() => {
                const next = new Set(picked);
                if (next.has(v.value)) next.delete(v.value);
                else next.add(v.value);
                setPicked(next);
              }}
            >
              <span>{v.value}</span>
              <span className="quiet">{v.count}</span>
              <span
                className={picked.has(v.value) ? "is-software" : "not-software"}
              >
                {picked.has(v.value)
                  ? t("work.software")
                  : t("work.notSoftware")}
              </span>
            </button>
          ))}
        </div>
        {dirty ? (
          <Button size="sm" disabled={saving} onClick={() => void save()}>
            {saving ? <Spinner aria-hidden="true" /> : null}
            {t("work.saveClassification")}
          </Button>
        ) : null}
      </CollapsibleContent>
    </Collapsible>
  );
}

const fieldValue = (field: Field, value: string) =>
  field === "stage"
    ? t(`work.stage.${value}`)
    : field === "origin"
      ? t(`work.origin.${value}`)
      : value;

function CorrectionChip({ c }: { c: Correction }) {
  const field = c.field as Field;
  return (
    <span className={`correction correction-${c.state}`}>
      {t(`work.field.${field}`)}：{fieldValue(field, c.value)}
      {c.state !== "resolved" && c.remote_now ? (
        <s>{fieldValue(field, c.remote_now)}</s>
      ) : null}
      <span className="correction-state">
        {t(`work.correction.${c.state}`)}
      </span>
    </span>
  );
}

function Row({
  item,
  now,
  onLocal,
}: {
  item: WorkItem;
  now: number;
  onLocal: () => void;
}) {
  const a = item.activity;
  const held = ageDays(a.status_since, now);
  const created = ageDays(a.created, now);
  const comment = a.last_comment;
  const stale = isStale(item, now);
  const local = item.local;
  return (
    <li className={`work-row stage-edge-${item.group}`}>
      <ExternalLink href={item.url} className="work-id">
        {item.display_id}
      </ExternalLink>
      <div className="work-main">
        <ExternalLink href={item.url} className="work-title">
          {item.title}
        </ExternalLink>
        <span className="work-meta">
          <span className={`status-chip stage-${item.group}`}>
            {item.status}
          </span>
          {held !== null ? (
            <Hint
              className={stale ? "short" : undefined}
              text={t(`work.since.${a.status_since_source || "unknown"}`)}
            >
              {t("work.held", { count: held })}
            </Hint>
          ) : null}
          {item.owner ? <span>{item.owner}</span> : null}
          {item.severity ? (
            <span className="quiet">{item.severity}</span>
          ) : null}
          {created !== null ? (
            <span className="quiet">
              {t("work.created", { count: created })}
            </span>
          ) : null}
          {a.found_versions.length || a.target_versions.length ? (
            <Hint
              className="work-versions"
              text={[a.found_versions.join(", "), a.target_versions.join(", ")]
                .filter(Boolean)
                .join(" → ")}
            >
              {a.found_versions.map(shortVersion).join(", ") || "—"}
              {a.target_versions.length
                ? ` → ${a.target_versions.map(shortVersion).join(", ")}`
                : ""}
            </Hint>
          ) : null}
          {a.reopened ? (
            <span className="short">
              {t("work.reopened", { count: a.reopened })}
            </span>
          ) : null}
          {item.commits.length ? (
            <span
              className="work-commits"
              title={item.commits.map((n) => `#${n}`).join(" ")}
            >
              {t("work.commits", { count: item.commits.length })}
            </span>
          ) : null}
        </span>
        {comment ? (
          <span className="work-activity">
            <strong>{comment.by}</strong>
            <span className="quiet">
              {t("work.ago", { count: ageDays(comment.at, now) ?? 0 })}
            </span>
            <span className="work-comment">{comment.text}</span>
            {a.comment_count > 1 ? (
              <span className="quiet">
                {t("work.commentCount", { count: a.comment_count })}
              </span>
            ) : null}
          </span>
        ) : a.progress ? (
          <span className="work-activity">
            <strong>{t("work.progress")}</strong>
            <span className="work-comment">{a.progress}</span>
          </span>
        ) : null}
        {item.origin ? (
          <span className="work-origin">
            <span className={`origin-chip origin-${item.origin}`}>
              {t(`work.origin.${item.origin}`)}
              {item.origin_kind
                ? ` · ${t(`work.kind.${item.origin_kind}`)}`
                : ""}
            </span>
            <span className="quiet">{item.origin_evidence}</span>
          </span>
        ) : null}
        <span className="work-local">
          {local?.tags.map((tag) => (
            <span key={tag} className="local-tag">
              {tag}
            </span>
          ))}
          {local?.corrections.map((c) => (
            <CorrectionChip key={c.field} c={c} />
          ))}
          {local?.next_check ? (
            <span className={isDue(item) ? "short" : "quiet"}>
              {t("work.nextCheck", { date: local.next_check.slice(5) })}
            </span>
          ) : null}
          {local?.notes.length ? (
            <span className="quiet" title={local.notes.at(-1)?.text}>
              {t("work.notes", { count: local.notes.length })}
            </span>
          ) : null}
          <button type="button" className="link local-edit" onClick={onLocal}>
            <Tag aria-hidden="true" />
            {t("work.mark")}
          </button>
        </span>
      </div>
    </li>
  );
}

/** Tags, corrections, the next check and notes for one item; each change saves at once. */
function LocalDialog({
  item,
  kind,
  code,
  tags,
  onClose,
}: {
  item: WorkItem;
  kind: WorkKind;
  code: string;
  tags: string[];
  onClose: () => void;
}) {
  const local = item.local;
  const [tag, setTag] = useState("");
  const [field, setField] = useState<Field>(
    kind === "requirements" ? "origin" : "found_version",
  );
  const [value, setValue] = useState("");
  const [reason, setReason] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const remote = remoteOf(item, field);
  async function apply(op: Record<string, unknown>) {
    setBusy(true);
    setFailure(null);
    try {
      await focus("annotate", { source: item.source, item: item.id, op });
      await reloadLocal(code, kind);
      return true;
    } catch (error) {
      setFailure(failureOf(error));
      return false;
    } finally {
      setBusy(false);
    }
  }
  const current = local?.tags ?? [];
  const setTags = (next: string[]) => apply({ op: "tags", tags: next });
  const choices: Record<string, string[]> = {
    stage: [...stages],
    origin: ["customer", "internal"],
  };
  return (
    <Dialog open onOpenChange={(open) => (open ? null : onClose())}>
      <DialogContent className="local-dialog">
        <DialogTitle>
          {item.display_id} · {t("work.localTitle")}
        </DialogTitle>
        <DialogDescription>{item.title}</DialogDescription>
        {failure ? <FailureNote failure={failure} /> : null}
        <section aria-label={t("work.tags")}>
          <h3>{t("work.tags")}</h3>
          <div className="local-tags">
            {current.map((x) => (
              <button
                type="button"
                key={x}
                className="local-tag removable"
                aria-label={t("work.removeTag", { tag: x })}
                disabled={busy}
                onClick={() => void setTags(current.filter((y) => y !== x))}
              >
                {x} ×
              </button>
            ))}
            {tags
              .filter((x) => !current.includes(x))
              .slice(0, 12)
              .map((x) => (
                <button
                  type="button"
                  key={x}
                  className="local-tag suggestion"
                  disabled={busy}
                  onClick={() => void setTags([...current, x])}
                >
                  + {x}
                </button>
              ))}
          </div>
          <form
            className="local-row"
            onSubmit={(e) => {
              e.preventDefault();
              const next = tag.trim();
              if (next && !current.includes(next))
                void setTags([...current, next]).then((ok) => ok && setTag(""));
            }}
          >
            <Input
              value={tag}
              maxLength={40}
              placeholder={t("work.newTag")}
              aria-label={t("work.newTag")}
              onChange={(e) => setTag(e.target.value)}
            />
            <Button size="sm" type="submit" disabled={busy || !tag.trim()}>
              {t("work.addTag")}
            </Button>
          </form>
        </section>
        <section aria-label={t("work.corrections")}>
          <h3>{t("work.corrections")}</h3>
          {local?.corrections.map((c) => (
            <div key={c.field} className="local-row">
              <CorrectionChip c={c} />
              <span className="quiet">{c.reason}</span>
              <button
                type="button"
                className="link"
                disabled={busy}
                onClick={() => void apply({ op: "uncorrect", field: c.field })}
              >
                {t("work.undo")}
              </button>
            </div>
          ))}
          <form
            className="local-correct"
            onSubmit={(e) => {
              e.preventDefault();
              void apply({
                op: "correct",
                field,
                value: value.trim(),
                remote,
                reason: reason.trim(),
              }).then((ok) => {
                if (ok) {
                  setValue("");
                  setReason("");
                }
              });
            }}
          >
            <Select
              value={field}
              items={fields.map((f) => ({
                value: f,
                label: t(`work.field.${f}`),
              }))}
              onValueChange={(f) => {
                if (!f) return;
                setField(f as Field);
                setValue("");
              }}
            >
              <SelectTrigger size="sm" aria-label={t("work.field")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {fields.map((f) => (
                  <SelectItem key={f} value={f}>
                    {t(`work.field.${f}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <span className="quiet">
              {t("work.remoteNow", {
                value: remote ? fieldValue(field, remote) : "—",
              })}
            </span>
            {choices[field] ? (
              <Select
                value={value || null}
                items={choices[field].map((v) => ({
                  value: v,
                  label: fieldValue(field, v),
                }))}
                onValueChange={(v) => setValue((v as string) ?? "")}
              >
                <SelectTrigger size="sm" aria-label={t("work.value")}>
                  <SelectValue placeholder={t("work.pick")} />
                </SelectTrigger>
                <SelectContent>
                  {choices[field].map((v) => (
                    <SelectItem key={v} value={v}>
                      {fieldValue(field, v)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                value={value}
                maxLength={200}
                aria-label={t("work.value")}
                placeholder={t("work.value")}
                onChange={(e) => setValue(e.target.value)}
              />
            )}
            <Input
              value={reason}
              maxLength={500}
              aria-label={t("work.reason")}
              placeholder={t("work.reason")}
              onChange={(e) => setReason(e.target.value)}
            />
            <Button
              size="sm"
              type="submit"
              disabled={busy || !value.trim() || value.trim() === remote}
            >
              {t("work.saveCorrection")}
            </Button>
          </form>
        </section>
        <section aria-label={t("work.nextCheckTitle")}>
          <h3>{t("work.nextCheckTitle")}</h3>
          <div className="local-row">
            <Input
              type="date"
              aria-label={t("work.nextCheckTitle")}
              value={local?.next_check ?? ""}
              disabled={busy}
              onChange={(e) =>
                void apply({ op: "next_check", date: e.target.value || null })
              }
            />
            {local?.next_check ? (
              <button
                type="button"
                className="link"
                disabled={busy}
                onClick={() => void apply({ op: "next_check", date: null })}
              >
                {t("work.clear")}
              </button>
            ) : null}
          </div>
        </section>
        <section aria-label={t("work.notesTitle")}>
          <h3>{t("work.notesTitle")}</h3>
          <ol className="local-notes">
            {local?.notes.map((n, i) => (
              <li key={i}>
                <span className="quiet">
                  {new Date(n.at).toLocaleString(undefined, {
                    month: "2-digit",
                    day: "2-digit",
                    hour: "2-digit",
                    minute: "2-digit",
                  })}
                </span>
                <span>{n.text}</span>
              </li>
            ))}
          </ol>
          <form
            className="local-row"
            onSubmit={(e) => {
              e.preventDefault();
              void apply({ op: "note", text: note.trim() }).then(
                (ok) => ok && setNote(""),
              );
            }}
          >
            <Input
              value={note}
              maxLength={4000}
              placeholder={t("work.newNote")}
              aria-label={t("work.newNote")}
              onChange={(e) => setNote(e.target.value)}
            />
            <Button size="sm" type="submit" disabled={busy || !note.trim()}>
              {t("work.addNote")}
            </Button>
          </form>
        </section>
        {busy ? <Spinner aria-hidden="true" /> : null}
      </DialogContent>
    </Dialog>
  );
}

const PAGE = 150;
const sorts = ["activity", "held", "created"] as const;
type Sort = (typeof sorts)[number];

/** What needs a look: one filter at a time, kept per model so 概览 can open the tab on it. */
export type Attention =
  "" | "blocked" | "stale" | "due" | "corrected" | "conflict" | `tag:${string}`;
export function attends(
  item: WorkItem,
  attention: Attention,
  now = Date.now(),
) {
  switch (attention) {
    case "":
      return true;
    case "blocked":
      return item.group === "blocked";
    case "stale":
      return isStale(item, now);
    case "due":
      return isDue(item);
    case "corrected":
      return !!item.local?.corrections.some((c) => c.state !== "resolved");
    case "conflict":
      return !!item.local?.corrections.some((c) => c.state === "conflict");
    default:
      return !!item.local?.tags.includes(attention.slice(4));
  }
}
/** The per-model view state of 问题 and 需求, shared with 概览's shortcuts. */
export function useWorkView(code: string, kind: WorkKind) {
  const [stage, setStage] = useModelPreference(code, `${kind}-stage`, "open");
  const [attention, setAttention] = useModelPreference<Attention>(
    code,
    `${kind}-attention`,
    "",
  );
  const [origin, setOrigin] = useModelPreference(code, `${kind}-origin`, "all");
  return { stage, setStage, attention, setAttention, origin, setOrigin };
}

/** 问题 and 需求: software items in the model's scope, by stage, with what decided each. */
export function WorkItemsTab({
  code,
  kind,
  saved,
  state,
}: {
  code: string;
  kind: WorkKind;
  saved?: FocusModel;
  state: FocusState;
}) {
  useTranslation();
  const work = useWorkItems(code, kind);
  const view = useWorkView(code, kind);
  const { stage, setStage, attention, setAttention, origin, setOrigin } = view;
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<Sort>("activity");
  const [shown, setShown] = useState(PAGE);
  const [editing, setEditing] = useState<string | null>(null);
  const [now] = useState(() => Date.now());
  const data: WorkItems | null = work.data;
  const scoped = useMemo(
    () =>
      (data?.items ?? []).filter(
        (i) =>
          origin === "all" ||
          (origin === "internet"
            ? i.origin_kind === "internet"
            : i.origin === origin),
      ),
    [data, origin],
  );
  const tags = useMemo(
    () =>
      [
        ...new Set((data?.items ?? []).flatMap((i) => i.local?.tags ?? [])),
      ].sort(),
    [data],
  );
  const attentionCounts = useMemo(() => {
    const open = scoped.filter((i) => !closed.has(i.group));
    const count = (a: Attention) =>
      (a === "corrected" || a === "conflict" || a.startsWith("tag:")
        ? scoped
        : open
      ).filter((i) => attends(i, a, now)).length;
    return [
      ...(["blocked", "stale", "due", "corrected", "conflict"] as const),
      ...tags.map((tag) => `tag:${tag}` as const),
    ]
      .map((a) => [a, count(a)] as const)
      .filter(([, n]) => n > 0);
  }, [scoped, tags, now]);
  const list = useMemo(() => {
    const q = search.trim().toLowerCase();
    const time = (v: string) => updatedAt(v) ?? 0;
    return scoped
      .filter((i) =>
        stage === "all"
          ? true
          : stage === "open"
            ? !closed.has(i.group)
            : i.group === stage,
      )
      .filter((i) => attends(i, attention, now))
      .filter(
        (i) =>
          !q ||
          `${i.display_id} ${i.title} ${i.owner} ${i.status} ${(i.local?.tags ?? []).join(" ")}`
            .toLowerCase()
            .includes(q),
      )
      .sort((a, b) =>
        sort === "activity"
          ? time(lastActivity(b)) - time(lastActivity(a))
          : sort === "held"
            ? time(a.activity.status_since || a.updated) -
              time(b.activity.status_since || b.updated)
            : time(a.activity.created) - time(b.activity.created),
      );
  }, [scoped, stage, attention, search, sort, now]);
  const origins = useMemo(() => {
    const items = data?.items ?? [];
    const count = (f: (i: WorkItem) => boolean) => [
      items.filter(f).length,
      items.filter((i) => f(i) && !closed.has(i.group)).length,
    ];
    return {
      all: count(() => true),
      customer: count((i) => i.origin === "customer"),
      internal: count((i) => i.origin === "internal"),
      internet: count((i) => i.origin_kind === "internet"),
    } as Record<string, number[]>;
  }, [data]);
  const editingItem = data?.items.find(
    (i) => `${i.source}:${i.id}` === editing,
  );
  const attentionLabel = (a: Attention, n: number) =>
    a.startsWith("tag:")
      ? `# ${a.slice(4)} ${n}`
      : t(`work.attention.${a}`, { count: n, days: STALE_DAYS });

  return (
    <div className="work-tab">
      {work.failure ? <FailureNote failure={work.failure} /> : null}
      {!data ? (
        <div className="job-skeletons" aria-hidden="true">
          <Skeleton className="stage-skeleton" />
          <Skeleton className="change-skeleton" />
        </div>
      ) : (
        <>
          <section className="work-sources" aria-label={t("work.scope")}>
            {data.sources.map((s) => (
              <div key={s.id} className="work-source-line">
                <SourceLine source={s} />
                {s.classification && saved && kind === "issues" ? (
                  <Classify
                    source={s}
                    saved={saved}
                    state={state}
                    onSaved={() => void work.refresh()}
                  />
                ) : null}
              </div>
            ))}
            <span className="work-read">
              <span className="quiet">
                {t("work.checked", { time: since(data.checked_at) })}
              </span>
              <Button
                variant="ghost"
                size="sm"
                disabled={work.loading}
                onClick={() => void work.refresh()}
              >
                {work.loading ? <Spinner aria-hidden="true" /> : <RefreshCw />}
                {t("work.refresh")}
              </Button>
            </span>
          </section>
          {kind === "requirements" ? (
            <Choices
              className="origin-split"
              label={t("work.originGroup")}
              value={origin}
              empty="all"
              onChange={setOrigin}
            >
              {(["all", "customer", "internal", "internet"] as const).map(
                (o) => (
                  <Toggle
                    key={o}
                    value={o}
                    className={o === "internet" ? "origin-sub" : undefined}
                  >
                    <span>{t(`work.split.${o}`)}</span>
                    <strong>{origins[o][1]}</strong>
                    <span className="quiet">
                      {t("work.ofTotal", { count: origins[o][0] })}
                    </span>
                  </Toggle>
                ),
              )}
            </Choices>
          ) : null}
          <StageBar items={scoped} stage={stage} onStage={setStage} />
          {attentionCounts.length ? (
            <div className="attention-row">
              <span className="attention-title">{t("work.needsLook")}</span>
              <Choices
                className="attention-chips"
                label={t("work.needsLook")}
                value={attention}
                empty=""
                onChange={(a) => setAttention(a as Attention)}
              >
                {attentionCounts.map(([a, n]) => (
                  <Toggle
                    key={a}
                    value={a}
                    className={`attention-${a.startsWith("tag:") ? "tag" : a}`}
                  >
                    {attentionLabel(a, n)}
                  </Toggle>
                ))}
              </Choices>
            </div>
          ) : null}
          <div className="work-filters">
            <Input
              value={search}
              placeholder={t("work.search")}
              aria-label={t("work.search")}
              onChange={(e) => setSearch(e.target.value)}
            />
            <Select
              value={sort}
              items={sorts.map((s) => ({
                value: s,
                label: t(`work.sort.${s}`),
              }))}
              onValueChange={(v) => v && setSort(v as Sort)}
            >
              <SelectTrigger size="sm" aria-label={t("work.sort")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {sorts.map((s) => (
                  <SelectItem key={s} value={s}>
                    {t(`work.sort.${s}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <span className="quiet work-count">
              {t("work.showing", { count: list.length })}
            </span>
          </div>
          {list.length ? (
            <ol className="work-list">
              {list.slice(0, shown).map((i) => (
                <Row
                  key={`${i.scope_id}-${i.id}`}
                  item={i}
                  now={now}
                  onLocal={() => setEditing(`${i.source}:${i.id}`)}
                />
              ))}
            </ol>
          ) : (
            <p className="quiet empty-line">{t("work.none")}</p>
          )}
          {list.length > shown ? (
            <button
              type="button"
              className="link"
              onClick={() => setShown(shown + PAGE)}
            >
              {t("work.more", { count: list.length - shown })}
            </button>
          ) : null}
          {editingItem ? (
            <LocalDialog
              item={editingItem}
              kind={kind}
              code={code}
              tags={tags}
              onClose={() => setEditing(null)}
            />
          ) : null}
        </>
      )}
    </div>
  );
}

/** 概览's card: open issues and requirements, and the shortcuts into what needs a look. */
export function WorkSummary({
  code,
  issues,
  requirements,
  onOpen,
}: {
  code: string;
  issues: WorkItems | null;
  requirements: WorkItems | null;
  onOpen: (kind: WorkKind) => void;
}) {
  useTranslation();
  const issueView = useWorkView(code, "issues");
  const requirementView = useWorkView(code, "requirements");
  const [now] = useState(() => Date.now());
  const open = (w: WorkItems | null) =>
    w?.items.filter((i) => !closed.has(i.group)) ?? [];
  const go = (
    kind: WorkKind,
    view: ReturnType<typeof useWorkView>,
    attention: Attention,
    origin = "all",
  ) => {
    view.setStage("open");
    view.setAttention(attention);
    view.setOrigin(origin);
    onOpen(kind);
  };
  const shortcut = (label: string, n: number, run: () => void, tone = "") =>
    n ? (
      <button type="button" className={`work-shortcut ${tone}`} onClick={run}>
        <strong>{n}</strong> {label}
      </button>
    ) : null;
  const openIssues = open(issues);
  const openRequirements = open(requirements);
  return (
    <section aria-label={t("work.summary")} className="work-summary">
      <div className="column-head">
        <h2>{t("work.summary")}</h2>
      </div>
      <div className="work-summary-line">
        <button
          type="button"
          className="work-summary-total"
          onClick={() => go("issues", issueView, "")}
        >
          <span>{t("model.issues")}</span>
          <strong>{issues ? openIssues.length : "—"}</strong>
          <span className="quiet">{t("work.open")}</span>
        </button>
        <span className="work-shortcuts">
          {shortcut(
            t("work.stage.blocked"),
            openIssues.filter((i) => i.group === "blocked").length,
            () => go("issues", issueView, "blocked"),
            "short",
          )}
          {shortcut(
            t("work.noActivity", { days: STALE_DAYS }),
            openIssues.filter((i) => isStale(i, now)).length,
            () => go("issues", issueView, "stale"),
          )}
          {shortcut(
            t("work.checkDue"),
            openIssues.filter((i) => isDue(i)).length,
            () => go("issues", issueView, "due"),
            "short",
          )}
        </span>
      </div>
      <div className="work-summary-line">
        <button
          type="button"
          className="work-summary-total"
          onClick={() => go("requirements", requirementView, "")}
        >
          <span>{t("model.requirements")}</span>
          <strong>{requirements ? openRequirements.length : "—"}</strong>
          <span className="quiet">{t("work.open")}</span>
        </button>
        <span className="work-shortcuts">
          {shortcut(
            t("work.origin.customer"),
            openRequirements.filter((i) => i.origin === "customer").length,
            () => go("requirements", requirementView, "", "customer"),
          )}
          {shortcut(
            t("work.origin.internal"),
            openRequirements.filter((i) => i.origin === "internal").length,
            () => go("requirements", requirementView, "", "internal"),
          )}
          {shortcut(
            t("work.checkDue"),
            openRequirements.filter((i) => isDue(i)).length,
            () => go("requirements", requirementView, "due"),
            "short",
          )}
        </span>
      </div>
    </section>
  );
}
