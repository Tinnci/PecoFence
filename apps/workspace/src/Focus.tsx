import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, RefreshCw } from "lucide-react";
import { BrandMark } from "@/components/brand-mark";
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
  revert: "回退",
  feature_after_freeze: "冻结后进入 MP 的新功能",
  customer_feedback: "客户反馈",
  other_model: "其他机型的改动",
  no_issue_id: "没有问题单号",
  automation: "自动化改动（gerrit）",
};
export const ruleHow: Record<string, string> = {
  revert: "提交以 Revert 开头",
  feature_after_freeze: "标注 [Feature]，冻结日期之后进入以 _MP 结尾的分支",
  customer_feedback: "提交含“客户反馈”",
  other_model: "提交标注的机型都不是这个机型",
  no_issue_id: "提交里没有 W-、R-、Bug # 这样的编号",
  automation: "由 gerrit 账号提交",
};
export const actionNames: Record<string, string> = {
  alert: "提醒",
  track: "跟踪",
  log: "只记录",
  ignore: "忽略",
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
  sources.map((s) => `${s}问题`).join(" · ");

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
    if (s.gap) bits.push(`严重问题还差 ${s.gap} 个`);
    if (fatal) bits.push(`${behind.measures.fatal_open} 个致命问题未解决`);
    if (!bits.length) {
      const o = gate(
        behind.measures.overall_resolved,
        behind.measures.overall_total,
        behind.targets.overall_pct,
      );
      bits.push(`整体还差 ${o.gap} 个`);
    }
    parts.push(`${behind.code} 落后最多：${bits.join("，")}。`);
  } else {
    parts.push("关注的机型都达到了门槛。");
  }
  if (busiest.new_issues + busiest.week_changes > 0)
    parts.push(
      `${busiest.code} 本周最忙，新增 ${busiest.new_issues} 个问题，合入 ${busiest.week_merged} 个变更。`,
    );
  return parts.join("");
}

export const greens = ["#e2ded2", "#c6d8bd", "#8fb784", "#4f8a4a", "#24552b"];
export const level = (n: number) =>
  n === 0 ? 0 : n <= 2 ? 1 : n <= 5 ? 2 : n <= 15 ? 3 : 4;

/** Twelve Monday-first weeks, one cell per day. */
function WeekGrid({ days }: { days: number[] }) {
  return (
    <div className="week-grid" aria-hidden="true">
      {days.map((n, i) => (
        <span key={i} style={{ background: greens[level(n)] }} />
      ))}
    </div>
  );
}
export function Strip({ days }: { days: number[] }) {
  return (
    <div className="day-strip" aria-hidden="true">
      {days.map((n, i) => (
        <span key={i} style={{ background: greens[level(n)] }} />
      ))}
    </div>
  );
}

function GateCell({ v, n, target }: { v: number; n: number; target: number }) {
  const g = gate(v, n, target);
  return (
    <div className="gate-cell">
      <strong className={g.ok ? "" : "short"}>{g.pct}%</strong>
      <span className="gate-track">
        <span
          className={g.ok ? "ok" : "short"}
          style={{ width: `${Math.min(100, g.pct)}%` }}
        />
        <i style={{ left: `calc(${target}% - 1px)` }} />
      </span>
      <small className={g.ok ? "" : "short"}>
        {g.ok ? "达标" : `还差 ${g.gap} 个`} · 目标 {target}%
      </small>
    </div>
  );
}

/** Followed models: the list, the cached summary first, then a fresh read. */
export function useFocus(enabled: boolean) {
  const [models, setModels] = useState<FocusModel[] | null>(null);
  const [summary, setSummary] = useState<FocusSummary | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  async function refresh(code?: string) {
    setRefreshing(true);
    setFailure(null);
    try {
      const data = await focus(code ? "refresh" : "summary", { code });
      if (data.summary) setSummary(data.summary);
    } catch (error) {
      setFailure(failureOf(error));
    } finally {
      setRefreshing(false);
    }
  }
  useEffect(() => {
    if (!enabled) return;
    void (async () => {
      try {
        const list = await focus("list");
        setModels(list.models ?? []);
        const cached = await focus("cached");
        if (cached.summary) setSummary(cached.summary);
        if (list.models?.length) await refresh();
      } catch (error) {
        setFailure(failureOf(error));
      }
    })();
  }, [enabled]);
  async function save(model: FocusModel) {
    const data = await focus("save", { model });
    setModels(data.models ?? []);
    await refresh(model.code);
  }
  async function remove(code: string) {
    const data = await focus("remove", { code });
    setModels(data.models ?? []);
    setSummary((s) =>
      s ? { ...s, models: s.models.filter((m) => m.code !== code) } : s,
    );
  }
  /** Optimistic: the change leaves 需要留意 (accept) or gains its mark (track). */
  async function review(code: string, change: number, action: string) {
    setSummary((s) =>
      s
        ? {
            ...s,
            models: s.models.map((m) =>
              m.code !== code
                ? m
                : {
                    ...m,
                    flagged:
                      action === "accept"
                        ? m.flagged.filter((f) => f.number !== change)
                        : m.flagged.map((f) =>
                            f.number === change
                              ? { ...f, tracked: action === "track" }
                              : f,
                          ),
                  },
            ),
          }
        : s,
    );
    try {
      await focus("review", { code, change, review: action });
    } catch (error) {
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
    failure,
    refresh,
    save,
    remove,
    review,
  };
}
export type FocusState = ReturnType<typeof useFocus>;

export function since(at?: string) {
  if (!at) return "";
  return new Date(at).toLocaleTimeString("zh-CN", {
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
  const { ordered, summary } = state;
  const today = new Date().toLocaleDateString("zh-CN", {
    month: "long",
    day: "numeric",
    weekday: "long",
  });
  return (
    <div className="focus-home">
      <section className="lede">
        <span className="kicker">
          {today} · {state.models?.length ?? 0} 个关注机型
          {summary?.computed_at ? ` · ${since(summary.computed_at)} 读取` : ""}
          {state.refreshing ? " · 正在更新…" : ""}
        </span>
        {ordered.length ? (
          <h1>{lede(ordered)}</h1>
        ) : (
          <h1 className="lede-loading">
            <BrandMark size={44} busy /> 正在读取关注的机型…
          </h1>
        )}
      </section>
      {state.failure ? <FailureNote failure={state.failure} /> : null}
      <section className="ledger" aria-label="关注的机型">
        <div className="ledger-head" aria-hidden="true">
          <span>机型</span>
          <span>严重问题解决率</span>
          <span>整体解决率</span>
          <span>致命未解决</span>
          <span>近 12 周代码变更</span>
          <span>本周</span>
        </div>
        {ordered.length
          ? ordered.map((m) => (
              <button
                type="button"
                className="ledger-row"
                key={m.code}
                aria-label={m.code}
                onClick={() => onModel(m.code)}
              >
                <span className="ledger-model">
                  <strong>{m.code}</strong>
                  <small>{sourceText(m.sources) || "未设置问题来源"}</small>
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
                      <small>上限 {m.targets.fatal_max}</small>
                    </span>
                  </>
                ) : (
                  <span className="ledger-missing">
                    {m.errors[0] ?? "没有问题来源"}
                  </span>
                )}
                <WeekGrid days={m.days} />
                <span className="ledger-week">
                  <span>
                    <strong>{m.new_issues ? `+${m.new_issues}` : "0"}</strong>{" "}
                    新问题
                  </span>
                  <span>
                    <strong>{m.week_changes}</strong> 变更 · 合入{" "}
                    {m.week_merged}
                  </span>
                  {m.flagged.length ? (
                    <span className="short">{m.flagged.length} 个需要留意</span>
                  ) : null}
                </span>
              </button>
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
          <span className="kicker">机型代号</span>
          <Input
            className="code-input"
            value={code}
            aria-label="机型代号"
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
            按代号在飞书项目、Jira 和 Gerrit
            里找候选。勾选属于这个机型的，规则决定哪些开发提交值得你看。
          </p>
          <Button
            variant="outline"
            disabled={loading || !code.trim()}
            onClick={() => void find(code)}
          >
            {loading ? <Spinner aria-hidden="true" /> : <RefreshCw />}
            查找
          </Button>
        </div>
      </section>
      {failure ? <FailureNote failure={failure} /> : null}
      {suggestions ? (
        <>
          <div className="follow-columns">
            <section aria-label="问题">
              <h2>问题</h2>
              <span className="quiet column-note">飞书项目 · 关联项目</span>
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
                <p className="quiet empty-line">没有找到</p>
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
                <p className="quiet empty-line">没有找到</p>
              )}
            </section>
            <section aria-label="代码">
              <h2>代码</h2>
              <span className="quiet column-note">
                Gerrit 分支 · 名称含 {suggestions.stem} · 近一年变更
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
            <section aria-label="规则">
              <h2>规则</h2>
              <span className="quiet column-note">
                右侧为预选分支近 13 周会命中的次数
              </span>
              {ruleOrder.map((r) => (
                <div className="rule-edit" key={r}>
                  <div>
                    <span>{ruleNames[r]}</span>
                    <span className="quiet">
                      {suggestions.rule_counts[r] ?? 0} 次
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
                <span>MP 冻结日期</span>
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
              关注 {suggestions.code}：{chosenMeegle.length + chosenJira.length}{" "}
              个问题来源，共 {issueCount} 个问题 · {chosenBranches.length}{" "}
              个分支 · {alerts} 条规则会提醒你
            </span>
            <div>
              <Button variant="ghost" onClick={onCancel}>
                <ArrowLeft /> 取消
              </Button>
              <Button disabled={saving} onClick={() => void follow()}>
                {saving ? <Spinner aria-hidden="true" /> : null}
                {existing ? "保存" : `关注 ${suggestions.code}`}
              </Button>
            </div>
          </footer>
        </>
      ) : loading ? (
        <p className="quiet">
          <Spinner aria-hidden="true" /> 正在三个工具里查找 {code}…
        </p>
      ) : null}
    </div>
  );
}
