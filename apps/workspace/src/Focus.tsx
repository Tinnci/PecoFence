import { ActionIcon } from "./components/action-icon";
import { ContextTarget } from "./components/context-menu";
import { modelContextActions } from "./context-actions";
import { useTranslation } from "react-i18next";
import { t, locale } from "./i18n";
import { useQuery } from "@tanstack/react-query";
import {
  queryClient,
  modelsOptions,
  modelsKey,
  summaryOptions,
  summaryKey,
  updateSummary,
  refreshModel,
  invalidateModel,
  optimisticReview,
  focusKey,
} from "./focus-cache";
import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, RefreshCw } from "lucide-react";
import { BrandMark } from "@/components/brand-mark";
import { Num } from "@/components/motion";
import { m as motion } from "motion/react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import type {
  Candidate,
  FocusModel,
  FocusSummary,
  ModelSummary,
  SetupFailure,
  Suggestions,
} from "./contracts/setup";
import { focus } from "./api";
import { FailureNote, failureOf } from "./TemplateImport";

export const ruleOrder = [
  "revert",
  "feature_after_freeze",
  "customer_feedback",
  "other_model",
  "no_issue_id",
  "automation",
] as const;
export const ruleNames: Record<string, string> = {
  get revert() {
    return t("focus.reverts");
  },
  get feature_after_freeze() {
    return t("focus.featuresEnteringMPAfterFreeze");
  },
  get customer_feedback() {
    return t("focus.customerFeedback");
  },
  get other_model() {
    return t("focus.changesForOtherModels");
  },
  get no_issue_id() {
    return t("focus.noIssueID");
  },
  get automation() {
    return t("focus.automatedChangesGerrit");
  },
};
export const ruleHow: Record<string, string> = {
  get revert() {
    return t("focus.commitStartsWithRevert");
  },
  get feature_after_freeze() {
    return t("focus.taggedFeatureAndEntersABranchEndingInMP");
  },
  get customer_feedback() {
    return t("focus.commitContainsCustomerFeedback");
  },
  get other_model() {
    return t("focus.noneOfTheModelsTaggedInTheCommitMatch");
  },
  get no_issue_id() {
    return t("focus.commitHasNoIdentifierSuchAsWROr");
  },
  get automation() {
    return t("focus.submittedByTheGerritAccount");
  },
};
export const actionNames: Record<string, string> = {
  get alert() {
    return t("focus.alert");
  },
  get track() {
    return t("focus.track");
  },
  get log() {
    return t("focus.logOnly");
  },
  get ignore() {
    return t("focus.ignore");
  },
};
export const defaultRules: Record<string, string> = {
  revert: "alert",
  feature_after_freeze: "alert",
  customer_feedback: "track",
  other_model: "log",
  no_issue_id: "log",
  automation: "ignore",
};

// Scope descriptions end in "的" or "全部", ready for the noun.
export const sourceText = (sources: string[]) =>
  sources.map((s) => t("focus.issuesFrom", { v0: s })).join(" · ");

/** One gate against its target: percent, the gap in issues, and whether it holds. */
export function gate(v: number, n: number, target: number) {
  const pct = n ? Math.round((v / n) * 1000) / 10 : 0;
  const gap = Math.max(0, Math.ceil((target / 100) * n) - v);
  return { pct, gap, ok: gap === 0, v, n };
}
/** A model falls short when any gate does; that orders the ledger and colours its tab. */
export function shortfall(m: ModelSummary) {
  if (!m.measures) return 0;
  const s = gate(
    m.measures.severe_resolved,
    m.measures.severe_total,
    m.targets.severe_pct,
  );
  const o = gate(
    m.measures.overall_resolved,
    m.measures.overall_total,
    m.targets.overall_pct,
  );
  return (
    s.gap * 10 +
    o.gap +
    Math.max(0, m.measures.fatal_open - m.targets.fatal_max) * 100
  );
}
/** The home page's opening sentence: who is furthest behind, and who was busiest. */
export function lede(models: ModelSummary[]) {
  if (!models.length) return "";
  const behind = [...models].sort((a, b) => shortfall(b) - shortfall(a))[0];
  const busiest = [...models].sort(
    (a, b) => b.new_issues + b.week_changes - (a.new_issues + a.week_changes),
  )[0];
  const parts: string[] = [];
  if (behind.measures && shortfall(behind) > 0) {
    const s = gate(
      behind.measures.severe_resolved,
      behind.measures.severe_total,
      behind.targets.severe_pct,
    );
    const fatal = Math.max(
      0,
      behind.measures.fatal_open - behind.targets.fatal_max,
    );
    const bits = [];
    if (s.gap)
      bits.push(t("focus.moreSevereIssuesToResolve", { count: s.gap }));
    if (fatal)
      bits.push(
        t("focus.fatalIssuesUnresolved", { count: behind.measures.fatal_open }),
      );
    if (!bits.length) {
      const o = gate(
        behind.measures.overall_resolved,
        behind.measures.overall_total,
        behind.targets.overall_pct,
      );
      bits.push(t("focus.moreIssuesToResolveOverall", { count: o.gap }));
    }
    parts.push(
      t("focus.isFurthestBehind", {
        v0: behind.code,
        v1: bits.join(t("format.listSeparator")),
      }),
    );
  } else {
    parts.push(t("focus.allFollowedModelsMeetTheirTargets"));
  }
  if (busiest.new_issues + busiest.week_changes > 0)
    parts.push(
      t("focus.wasBusiestThisWeekNewIssuesAndMergedChanges", {
        v0: busiest.code,
        v1: busiest.new_issues,
        v2: busiest.week_merged,
      }),
    );
  return parts.join(t("format.sentenceSeparator"));
}

export const greens = ["#e2ded2", "#c6d8bd", "#8fb784", "#4f8a4a", "#24552b"];
export const level = (n: number) =>
  n === 0 ? 0 : n <= 2 ? 1 : n <= 5 ? 2 : n <= 15 ? 3 : 4;

/** Twelve Monday-first weeks, one cell per day. */
function WeekGrid({ days }: { days: number[] }) {
  useTranslation();
  return (
    <div className="week-grid" aria-hidden="true">
      {days.map((n, i) => (
        <span key={i} style={{ background: greens[level(n)] }} />
      ))}
    </div>
  );
}
export function Strip({ days }: { days: number[] }) {
  useTranslation();
  return (
    <div className="day-strip" aria-hidden="true">
      {days.map((n, i) => (
        <span key={i} style={{ background: greens[level(n)] }} />
      ))}
    </div>
  );
}

function GateCell({ v, n, target }: { v: number; n: number; target: number }) {
  useTranslation();
  const g = gate(v, n, target);
  return (
    <div className="gate-cell">
      <strong className={g.ok ? "" : "short"}>
        <Num value={g.pct} suffix="%" />
      </strong>
      <span className="gate-track">
        <span
          className={g.ok ? "ok" : "short"}
          style={{ width: `${Math.min(100, g.pct)}%` }}
        />
        <i style={{ left: `calc(${target}% - 1px)` }} />
      </span>
      <small className={g.ok ? "" : "short"}>
        {t("focus.target", {
          v0: g.ok
            ? t("focus.targetMet")
            : t("focus.moreToResolve", { v0: g.gap }),
          v1: target,
        })}
      </small>
    </div>
  );
}

/** Followed models: the list, the cached summary first, then a fresh read. */
export function useFocus(enabled: boolean) {
  const modelsQuery = useQuery({ ...modelsOptions, enabled }, queryClient);
  const summaryQuery = useQuery({ ...summaryOptions, enabled }, queryClient);
  const models = modelsQuery.data ?? null;
  const summary = useMemo(() => {
    if (!summaryQuery.data) return null;
    if (!models) return summaryQuery.data;
    const followed = new Set(models.map((model) => model.code));
    return {
      ...summaryQuery.data,
      models: summaryQuery.data.models.filter((model) =>
        followed.has(model.code),
      ),
    };
  }, [summaryQuery.data, models]);
  const [refreshing, setRefreshing] = useState(false);
  const pendingRefreshes = useRef(0);
  const reviewVersions = useRef(new Map<string, number>());
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  async function refresh(code?: string, force = true) {
    pendingRefreshes.current++;
    setRefreshing(true);
    setFailure(null);
    try {
      const current = queryClient.getQueryData<FocusModel[]>(modelsKey) ?? [];
      const errors: unknown[] = [];
      for (const target of code ? [code] : current.map((model) => model.code)) {
        try {
          await refreshModel(target, force);
        } catch (error) {
          errors.push(error);
        }
      }
      if (errors.length) throw errors[0];
    } catch (error) {
      setFailure(failureOf(error));
    } finally {
      pendingRefreshes.current--;
      setRefreshing(pendingRefreshes.current > 0);
    }
  }
  useEffect(() => {
    if (enabled && modelsQuery.isSuccess && summaryQuery.isFetched)
      void refresh(undefined, false);
  }, [enabled, modelsQuery.isSuccess, summaryQuery.isFetched]);
  async function save(model: FocusModel, refreshSources = true) {
    invalidateModel(model.code);
    await queryClient.cancelQueries({ queryKey: modelsKey });
    await queryClient.cancelQueries({ queryKey: summaryKey });
    const data = await focus("save", { model });
    queryClient.setQueryData(modelsKey, data.models ?? []);
    if (refreshSources) await refresh(model.code);
  }
  async function remove(code: string) {
    invalidateModel(code);
    await queryClient.cancelQueries({ queryKey: modelsKey });
    const data = await focus("remove", { code });
    queryClient.setQueryData(modelsKey, data.models ?? []);
    updateSummary((previous) => ({
      ...previous,
      models: previous.models.filter((model) => model.code !== code),
    }));
    queryClient.removeQueries({ queryKey: [...focusKey, "model", code] });
    queryClient.removeQueries({ queryKey: [...focusKey, "view", code] });
  }
  async function review(code: string, change: number, action: string) {
    invalidateModel(code);
    const reviewKey = `${code}:${change}`;
    const version = (reviewVersions.current.get(reviewKey) ?? 0) + 1;
    reviewVersions.current.set(reviewKey, version);
    const previous = queryClient
      .getQueryData<FocusSummary>(summaryKey)
      ?.models.find((model) => model.code === code);
    const previousItem = previous?.flagged.find(
      (item) => item.number === change,
    );
    updateSummary((summary) => optimisticReview(summary, code, change, action));
    try {
      await focus("review", { code, change, review: action });
      await queryClient.invalidateQueries({
        queryKey: [...focusKey, "model", code, "changes"],
      });
    } catch (error) {
      if (previousItem && reviewVersions.current.get(reviewKey) === version) {
        updateSummary((summary) => ({
          ...summary,
          models: summary.models.map((model) => {
            if (model.code !== code) return model;
            const flagged = model.flagged.filter(
              (item) => item.number !== change,
            );
            flagged.splice(
              previous!.flagged.indexOf(previousItem),
              0,
              previousItem,
            );
            return { ...model, flagged };
          }),
        }));
      }
      setFailure(failureOf(error));
    }
  }
  const ordered = useMemo(
    () =>
      summary
        ? [...summary.models].sort((a, b) => shortfall(b) - shortfall(a))
        : [],
    [summary],
  );
  return {
    models,
    summary,
    ordered,
    refreshing,
    failure:
      failure ??
      (modelsQuery.error
        ? failureOf(modelsQuery.error)
        : summaryQuery.error
          ? failureOf(summaryQuery.error)
          : null),
    refresh,
    ensure: (code: string) => refresh(code, false),
    save,
    remove,
    review,
  };
}
export type FocusState = ReturnType<typeof useFocus>;

export function since(at?: string) {
  if (!at) return "";
  return new Date(at).toLocaleTimeString(locale(), {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** 关注: one ledger row per followed model, most behind first. */
export function FocusHome({
  state,
  onModel,
}: {
  state: FocusState;
  onModel: (code: string) => void;
}) {
  useTranslation();
  const { ordered, summary } = state;
  const today = new Date().toLocaleDateString(locale(), {
    month: "long",
    day: "numeric",
    weekday: "long",
  });
  return (
    <div className="focus-home">
      <section className="lede">
        <span className="kicker">
          {t("focus.followedModels", {
            v0: today,
            count: state.models?.length ?? 0,
            v2: summary?.computed_at
              ? t("focus.loaded", { v0: since(summary.computed_at) })
              : "",
            v3: state.refreshing ? t("focus.updating") : "",
          })}
        </span>
        {ordered.length ? (
          <h1>{lede(ordered)}</h1>
        ) : (
          <h1 className="lede-loading">
            <BrandMark size={44} busy /> {t("focus.loadingFollowedModels")}
          </h1>
        )}
      </section>
      {state.failure ? <FailureNote failure={state.failure} /> : null}
      <section className="ledger" aria-label={t("focus.followedModels2")}>
        <div className="ledger-head" aria-hidden="true">
          <span>{t("app.model")}</span>
          <span>{t("focus.severeIssueResolution")}</span>
          <span>{t("focus.overallResolution")}</span>
          <span>{t("focus.fatalIssuesUnresolved2")}</span>
          <span>{t("focus.codeChangesOver12Weeks")}</span>
          <span>{t("focus.thisWeek")}</span>
        </div>
        {ordered.length
          ? ordered.map((m) => (
              <ContextTarget
                key={m.code}
                title={m.code}
                actions={modelContextActions(m.code, onModel, state.refresh)}
              >
                <motion.button
                  layout="position"
                  type="button"
                  className="ledger-row"
                  aria-label={m.code}
                  onClick={() => onModel(m.code)}
                >
                  <span className="ledger-model">
                    <strong>{m.code}</strong>
                    <small>
                      {sourceText(m.sources) ||
                        t("focus.noIssueSourcesConfigured")}
                    </small>
                  </span>
                  {m.measures ? (
                    <>
                      <GateCell
                        v={m.measures.severe_resolved}
                        n={m.measures.severe_total}
                        target={m.targets.severe_pct}
                      />
                      <GateCell
                        v={m.measures.overall_resolved}
                        n={m.measures.overall_total}
                        target={m.targets.overall_pct}
                      />
                      <span className="gate-cell">
                        <strong
                          className={
                            m.measures.fatal_open > m.targets.fatal_max
                              ? "short"
                              : ""
                          }
                        >
                          {m.measures.fatal_open}
                        </strong>
                        <small>
                          {t("focus.limit", { v0: m.targets.fatal_max })}
                        </small>
                      </span>
                    </>
                  ) : (
                    <span className="ledger-missing">
                      {m.errors[0] ?? t("focus.noIssueSources")}
                    </span>
                  )}
                  <WeekGrid days={m.days} />
                  <span className="ledger-week">
                    <span>
                      <strong>{m.new_issues ? `+${m.new_issues}` : "0"}</strong>
                      {t("focus.newIssues")}
                    </span>
                    <span>
                      <strong>{m.week_changes}</strong>{" "}
                      {t("focus.changesMerged", { v0: m.week_merged })}
                    </span>
                    {m.flagged.length ? (
                      <span className="short">
                        {t("focus.needAttention", { v0: m.flagged.length })}
                      </span>
                    ) : null}
                  </span>
                </motion.button>
              </ContextTarget>
            ))
          : (state.models ?? []).map((m) => (
              <div className="ledger-row" key={m.code} aria-hidden="true">
                <span className="ledger-model">
                  <strong>{m.code}</strong>
                </span>
                <Skeleton className="bar" />
                <Skeleton className="bar" />
                <Skeleton className="bar" />
                <Skeleton className="bar" />
                <Skeleton className="bar" />
              </div>
            ))}
      </section>
    </div>
  );
}

function Pick({
  candidate,
  on,
  toggle,
}: {
  candidate: Candidate;
  on: boolean;
  toggle: () => void;
}) {
  useTranslation();
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={on}
      className="pick-line"
      onClick={toggle}
    >
      <span className={on ? "box on" : "box"} aria-hidden="true" />
      <span className={on ? "name on" : "name"}>{candidate.name}</span>
      <span className="quiet">{candidate.count}</span>
    </button>
  );
}

/** 关注一个机型: choose what belongs to the model, then what its rules do. */
export function FollowPage({
  state,
  initial,
  onDone,
  onCancel,
}: {
  state: FocusState;
  initial?: string;
  onDone: (code: string) => void;
  onCancel: () => void;
}) {
  useTranslation();
  const existing = state.models?.find((m) => m.code === initial) ?? null;
  const [code, setCode] = useState(initial ?? "");
  const [suggestions, setSuggestions] = useState<Suggestions | null>(null);
  const [loading, setLoading] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [picked, setPicked] = useState<Record<string, boolean>>({});
  const [rules, setRules] = useState<Record<string, string>>(
    existing?.rules && Object.keys(existing.rules).length
      ? existing.rules
      : defaultRules,
  );
  const [freeze, setFreeze] = useState(existing?.freeze ?? "");
  const [saving, setSaving] = useState(false);

  async function find(value: string) {
    const c = value.trim();
    if (!c) return;
    setLoading(true);
    setFailure(null);
    try {
      const data = await focus("suggest", { code: c });
      const s = data.suggestions ?? null;
      setSuggestions(s);
      const chosen: Record<string, boolean> = {};
      const known = (name: string) =>
        existing
          ? existing.branches.includes(name) ||
            existing.issues.some((q) =>
              q.source === "meegle"
                ? q.filter?.values.some((v) => v.label === name)
                : name.startsWith(`${q.project} `),
            )
          : null;
      for (const cand of [
        ...(s?.meegle ?? []),
        ...(s?.jira ?? []),
        ...(s?.branches ?? []),
      ])
        chosen[cand.name] = known(cand.name) ?? cand.picked;
      setPicked(chosen);
    } catch (error) {
      setFailure(failureOf(error));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => {
    if (initial) void find(initial);
  }, [initial]);

  const on = (c: Candidate) => !!picked[c.name];
  const toggle = (c: Candidate) =>
    setPicked((p) => ({ ...p, [c.name]: !p[c.name] }));
  const chosenMeegle = suggestions?.meegle.filter(on) ?? [];
  const chosenJira = suggestions?.jira.filter(on) ?? [];
  const chosenBranches = suggestions?.branches.filter(on) ?? [];
  const issueCount = [...chosenMeegle, ...chosenJira].reduce(
    (n, c) => n + c.count,
    0,
  );
  const alerts = ruleOrder.filter((r) => rules[r] === "alert").length;

  function model(): FocusModel | null {
    if (!suggestions) return null;
    const issues = [];
    // Picked Meegle groups of one field form one scope with several values.
    const meegle = chosenMeegle.flatMap((c) =>
      c.scope?.source === "meegle" ? [c.scope] : [],
    );
    if (meegle.length && meegle[0].source === "meegle") {
      const first = meegle[0];
      issues.push({
        ...first,
        filter: first.filter
          ? {
              ...first.filter,
              values: meegle.flatMap((q) =>
                q.source === "meegle" ? (q.filter?.values ?? []) : [],
              ),
            }
          : undefined,
      });
    }
    for (const c of chosenJira) if (c.scope) issues.push(c.scope);
    return {
      code: suggestions.code,
      issues,
      branches: chosenBranches.map((b) => b.name),
      jobs: existing?.jobs ?? [],
      release_lines: existing?.release_lines?.map((line) => ({
        ...line,
        branches: line.branches.filter((b) =>
          chosenBranches.some((c) => c.name === b),
        ),
      })),
      ignored_jobs: existing?.ignored_jobs,
      job_sources: Object.fromEntries(
        Object.entries(existing?.job_sources ?? {})
          .filter(([, source]) =>
            chosenBranches.some((b) => b.name === source.branch),
          )
          .map(([job, source]) => [
            job,
            {
              ...source,
              history: source.history?.filter((range) =>
                chosenBranches.some((b) => b.name === range.branch),
              ),
              previous: source.previous.filter((branch) =>
                chosenBranches.some((b) => b.name === branch),
              ),
            },
          ]),
      ),
      branch_jobs: Object.fromEntries(
        Object.entries(existing?.branch_jobs ?? {}).filter(([branch]) =>
          chosenBranches.some((b) => b.name === branch),
        ),
      ),
      tracking: existing?.tracking,
      rules,
      ...(freeze ? { freeze } : {}),
      targets: existing?.targets ?? {
        severe_pct: 95,
        overall_pct: 85,
        fatal_max: 0,
      },
      accepted: existing?.accepted ?? [],
      tracked: existing?.tracked ?? [],
    };
  }
  async function follow() {
    const m = model();
    if (!m) return;
    setSaving(true);
    setFailure(null);
    try {
      await state.save(m);
      onDone(m.code);
    } catch (error) {
      setFailure(failureOf(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="follow-page">
      <section className="follow-head">
        <label>
          <span className="kicker">{t("focus.modelCode")}</span>
          <Input
            className="code-input"
            value={code}
            aria-label={t("focus.modelCode")}
            placeholder="E6751SM"
            maxLength={32}
            autoFocus={!initial}
            onChange={(e) => setCode(e.target.value.toUpperCase())}
            onKeyDown={(e) => {
              if (e.key === "Enter") void find(code);
            }}
          />
        </label>
        <div className="follow-intro">
          <p className="quiet">
            {t("focus.findCandidatesByModelCodeInMeegleJiraAnd")}
          </p>
          <Button
            variant="outline"
            disabled={loading || !code.trim()}
            onClick={() => void find(code)}
          >
            {loading ? <Spinner aria-hidden="true" /> : <RefreshCw />}
            {t("focus.find")}
          </Button>
        </div>
      </section>
      {failure ? <FailureNote failure={failure} /> : null}
      {suggestions ? (
        <>
          <div className="follow-columns">
            <section aria-label={t("focus.issues")}>
              <h2>{t("focus.issues")}</h2>
              <span className="quiet column-note">
                {t("focus.meegleLinkedProject")}
              </span>
              {suggestions.meegle.length ? (
                suggestions.meegle.map((c) => (
                  <Pick
                    key={c.name}
                    candidate={c}
                    on={on(c)}
                    toggle={() => toggle(c)}
                  />
                ))
              ) : (
                <p className="quiet empty-line">{t("focus.noMatches")}</p>
              )}
              <span className="quiet column-note">Jira</span>
              {suggestions.jira.length ? (
                suggestions.jira.map((c) => (
                  <Pick
                    key={c.name}
                    candidate={c}
                    on={on(c)}
                    toggle={() => toggle(c)}
                  />
                ))
              ) : (
                <p className="quiet empty-line">{t("focus.noMatches")}</p>
              )}
            </section>
            <section aria-label={t("focus.code")}>
              <h2>{t("focus.code")}</h2>
              <span className="quiet column-note">
                {t(
                  "focus.gerritBranchesNamesContainingChangesOverThePastYear",
                  { v0: suggestions.stem },
                )}
              </span>
              {suggestions.branches.map((c) => (
                <Pick
                  key={c.name}
                  candidate={c}
                  on={on(c)}
                  toggle={() => toggle(c)}
                />
              ))}
            </section>
            <section aria-label={t("focus.rules")}>
              <h2>{t("focus.rules")}</h2>
              <span className="quiet column-note">
                {t("focus.theRightColumnShowsMatchesInTheSelectedBranches")}
              </span>
              {ruleOrder.map((r) => (
                <div className="rule-edit" key={r}>
                  <div>
                    <span>{ruleNames[r]}</span>
                    <span className="quiet">
                      {t("focus.matches", {
                        count: suggestions.rule_counts[r] ?? 0,
                      })}
                    </span>
                  </div>
                  <span className="quiet">{ruleHow[r]}</span>
                  <div
                    className="segmented"
                    role="radiogroup"
                    aria-label={ruleNames[r]}
                  >
                    {Object.entries(actionNames).map(([a, label]) => (
                      <button
                        type="button"
                        role="radio"
                        aria-checked={rules[r] === a}
                        key={a}
                        onClick={() => setRules({ ...rules, [r]: a })}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                </div>
              ))}
              <label className="freeze">
                <span>{t("focus.mpFreezeDate")}</span>
                <Input
                  type="date"
                  value={freeze}
                  onChange={(e) => setFreeze(e.target.value)}
                />
              </label>
            </section>
          </div>
          <footer className="follow-foot">
            <span>
              {t("focus.followIssueSourcesIssuesBranchesAlertRules", {
                v0: suggestions.code,
                v1: chosenMeegle.length + chosenJira.length,
                v2: issueCount,
                v3: chosenBranches.length,
                v4: alerts,
              })}
            </span>
            <div>
              <Button variant="ghost" onClick={onCancel}>
                <ArrowLeft /> {t("focus.cancel")}
              </Button>
              <Button disabled={saving} onClick={() => void follow()}>
                {saving ? (
                  <Spinner aria-hidden="true" />
                ) : (
                  <ActionIcon name={existing ? "save" : "follow"} />
                )}
                {existing
                  ? t("focus.save")
                  : t("focus.follow", { v0: suggestions.code })}
              </Button>
            </div>
          </footer>
        </>
      ) : loading ? (
        <p className="quiet">
          <Spinner aria-hidden="true" />{" "}
          {t("focus.searchingThreeToolsFor", { v0: code })}
        </p>
      ) : null}
    </div>
  );
}
