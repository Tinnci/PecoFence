import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { ExternalLink } from "@/components/external-link";
import type {
  FocusModel,
  TrackingSource,
  WorkItem,
  WorkItems,
} from "./contracts/setup";
import {
  prefetchWorkItems,
  queryClient,
  workItemsOptions,
  type WorkKind,
} from "./focus-cache";
import { since, type FocusState } from "./Focus";
import { FailureNote, failureOf } from "./TemplateImport";
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

/** Jira writes ISO times with +0800; Meegle writes epoch milliseconds. */
export function updatedAt(value: string) {
  if (/^\d{10,}$/.test(value)) return Number(value);
  const parsed = Date.parse(value.replace(/([+-]\d\d)(\d\d)$/, "$1:$2"));
  return Number.isFinite(parsed) ? parsed : null;
}
export function ageDays(value: string, now = Date.now()) {
  const at = updatedAt(value);
  return at === null ? null : Math.floor((now - at) / 86_400_000);
}
export const isStale = (item: WorkItem, now = Date.now()) =>
  !closed.has(item.group) && (ageDays(item.updated, now) ?? 0) >= STALE_DAYS;

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

/** One bar for the scope: each stage's share, clickable to filter. */
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
  return (
    <div className="stage-bar">
      <div className="stage-track" aria-hidden="true">
        {stages.map((s) =>
          counts.get(s) ? (
            <span
              key={s}
              className={`stage-${s}`}
              style={{ flexGrow: counts.get(s) }}
              title={`${t(`work.stage.${s}`)} ${counts.get(s)}`}
              data-dim={stage !== "open" && stage !== s ? "" : undefined}
            />
          ) : null,
        )}
      </div>
      <div
        className="stage-legend"
        role="radiogroup"
        aria-label={t("work.stage")}
      >
        <button
          type="button"
          role="radio"
          aria-checked={stage === "open"}
          onClick={() => onStage("open")}
        >
          {t("work.open")} <strong>{open}</strong>
        </button>
        {stages.map((s) =>
          counts.get(s) ? (
            <button
              type="button"
              role="radio"
              key={s}
              aria-checked={stage === s}
              onClick={() => onStage(s)}
            >
              <i className={`stage-dot stage-${s}`} />
              {t(`work.stage.${s}`)} <strong>{counts.get(s)}</strong>
            </button>
          ) : null,
        )}
        <button
          type="button"
          role="radio"
          aria-checked={stage === "all"}
          onClick={() => onStage("all")}
        >
          {t("work.all")} <strong>{items.length}</strong>
        </button>
      </div>
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
          ...(saved.tracking ?? { origin_overrides: {} }),
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
    <details className="classify">
      <summary>
        {t("work.classify", { field: c.field_name })}
        <span className="quiet">
          {c.configured ? t("work.classifySaved") : t("work.classifySuggested")}
        </span>
      </summary>
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
              {picked.has(v.value) ? t("work.software") : t("work.notSoftware")}
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
    </details>
  );
}

function Row({
  item,
  now,
  onOrigin,
}: {
  item: WorkItem;
  now: number;
  onOrigin?: (origin: "customer" | "internal") => void;
}) {
  const age = ageDays(item.updated, now);
  const stale = isStale(item, now);
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
          {item.resolution && item.resolution !== item.status ? (
            <span className="quiet">{item.resolution}</span>
          ) : null}
          {item.owner ? <span>{item.owner}</span> : null}
          {item.severity ? (
            <span className="quiet">{item.severity}</span>
          ) : null}
          {age !== null ? (
            <span className={stale ? "short" : "quiet"}>
              {t("work.updated", { count: age })}
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
        {item.origin ? (
          <span className="work-origin">
            <span className={`origin-chip origin-${item.origin}`}>
              {t(`work.origin.${item.origin}`)}
              {item.origin_kind
                ? ` · ${t(`work.kind.${item.origin_kind}`)}`
                : ""}
            </span>
            <span className="quiet">{item.origin_evidence}</span>
            {onOrigin ? (
              <button
                type="button"
                className="link"
                onClick={() =>
                  onOrigin(item.origin === "customer" ? "internal" : "customer")
                }
              >
                {item.origin === "customer"
                  ? t("work.markInternal")
                  : t("work.markCustomer")}
              </button>
            ) : null}
          </span>
        ) : null}
      </div>
    </li>
  );
}

const PAGE = 150;

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
  const [stage, setStage] = useState("open");
  const [origin, setOrigin] = useState("all");
  const [search, setSearch] = useState("");
  const [staleOnly, setStaleOnly] = useState(false);
  const [shown, setShown] = useState(PAGE);
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
  const list = useMemo(() => {
    const q = search.trim().toLowerCase();
    return scoped
      .filter((i) =>
        stage === "all"
          ? true
          : stage === "open"
            ? !closed.has(i.group)
            : i.group === stage,
      )
      .filter((i) => !staleOnly || isStale(i, now))
      .filter(
        (i) =>
          !q ||
          `${i.display_id} ${i.title} ${i.owner} ${i.status}`
            .toLowerCase()
            .includes(q),
      )
      .sort(
        (a, b) => (updatedAt(b.updated) ?? 0) - (updatedAt(a.updated) ?? 0),
      );
  }, [scoped, stage, staleOnly, search, now]);
  const origins = useMemo(() => {
    const items = data?.items ?? [];
    const open = (f: (i: WorkItem) => boolean) =>
      items.filter((i) => f(i) && !closed.has(i.group)).length;
    return {
      all: [items.length, open(() => true)],
      customer: [
        items.filter((i) => i.origin === "customer").length,
        open((i) => i.origin === "customer"),
      ],
      internal: [
        items.filter((i) => i.origin === "internal").length,
        open((i) => i.origin === "internal"),
      ],
      internet: [
        items.filter((i) => i.origin_kind === "internet").length,
        open((i) => i.origin_kind === "internet"),
      ],
    } as Record<string, number[]>;
  }, [data]);
  const staleCount = scoped.filter((i) => isStale(i, now)).length;

  async function markOrigin(item: WorkItem, next: "customer" | "internal") {
    if (!saved) return;
    const tracking = saved.tracking ?? { origin_overrides: {} };
    await state.save({
      ...saved,
      tracking: {
        ...tracking,
        origin_overrides: { ...tracking.origin_overrides, [item.id]: next },
      },
    });
    void work.refresh();
  }

  return (
    <div className="work-tab">
      <div className="tab-head">
        <span className="quiet">
          {data
            ? t("work.checked", { time: since(data.checked_at) })
            : work.loading
              ? t("work.reading")
              : ""}
        </span>
        <Button
          variant="outline"
          size="sm"
          disabled={work.loading}
          onClick={() => void work.refresh()}
        >
          {work.loading ? <Spinner aria-hidden="true" /> : <RefreshCw />}
          {t("work.refresh")}
        </Button>
      </div>
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
              <div key={s.id}>
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
          </section>
          {kind === "requirements" ? (
            <div
              className="origin-split"
              role="radiogroup"
              aria-label={t("work.originGroup")}
            >
              {(["all", "customer", "internal", "internet"] as const).map(
                (o) => (
                  <button
                    type="button"
                    role="radio"
                    key={o}
                    aria-checked={origin === o}
                    className={o === "internet" ? "origin-sub" : undefined}
                    onClick={() => setOrigin(o)}
                  >
                    <span>{t(`work.split.${o}`)}</span>
                    <strong>{origins[o][1]}</strong>
                    <span className="quiet">
                      {t("work.ofTotal", { count: origins[o][0] })}
                    </span>
                  </button>
                ),
              )}
            </div>
          ) : null}
          <StageBar items={scoped} stage={stage} onStage={setStage} />
          <div className="work-filters">
            <Input
              value={search}
              placeholder={t("work.search")}
              aria-label={t("work.search")}
              onChange={(e) => setSearch(e.target.value)}
            />
            <button
              type="button"
              role="switch"
              aria-checked={staleOnly}
              className={staleOnly ? "stale-toggle on" : "stale-toggle"}
              onClick={() => setStaleOnly(!staleOnly)}
            >
              {t("work.staleOnly", { days: STALE_DAYS, count: staleCount })}
            </button>
          </div>
          {list.length ? (
            <ol className="work-list">
              {list.slice(0, shown).map((i) => (
                <Row
                  key={`${i.scope_id}-${i.id}`}
                  item={i}
                  now={now}
                  onOrigin={
                    kind === "requirements" && saved
                      ? (o) => void markOrigin(i, o)
                      : undefined
                  }
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
        </>
      )}
    </div>
  );
}
