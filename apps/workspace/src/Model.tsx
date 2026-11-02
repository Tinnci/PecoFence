import { useEffect, useMemo, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { AnimatePresence, m as motion } from "motion/react";
import { Num } from "@/components/motion";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import type {
  BuildRun,
  Builds,
  Flagged,
  FocusModel,
  JobView,
  Listed,
  ModelSummary,
  Preview,
  SetupFailure,
} from "./contracts/setup";
import { focus, type ModelTab } from "./api";
import {
  actionNames,
  defaultRules,
  gate,
  ruleHow,
  ruleNames,
  ruleOrder,
  since,
  sourceText,
  Strip,
  type FocusState,
} from "./Focus";
import { FailureNote, failureOf } from "./TemplateImport";

/** The current-tab underline; one per `id`, it slides to whichever tab renders it. */
export function TabLine({ id }: { id: string }) {
  return (
    <motion.span
      className="tab-line"
      layoutId={`tab-line-${id}`}
      aria-hidden="true"
    />
  );
}

export const phaseOrder = ["main", "ir", "pb", "mp", "old"] as const;
export const phaseNames: Record<string, string> = {
  main: "主线",
  ir: "IR",
  pb: "PB",
  mp: "MP",
  old: "旧",
};
const tabNames: Record<ModelTab, string> = {
  overview: "概览",
  branches: "分支",
  builds: "构建",
  settings: "设置",
};
const resultNames: Record<string, string> = {
  SUCCESS: "成功",
  FAILURE: "失败",
  UNSTABLE: "不稳定",
  ABORTED: "中止",
  NOT_BUILT: "未构建",
};

/** 2 小时 43 分, 43 分, 不到 1 分. */
export function span(ms: number) {
  const minutes = Math.round(ms / 60000);
  if (minutes < 1) return "不到 1 分";
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h ? `${h} 小时${m ? ` ${m} 分` : ""}` : `${m} 分`;
}
const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
  });
const day = (ms: number) =>
  new Date(ms).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });

/** A running build against Jenkins' estimate: share done, and when it should end. */
export function progress(run: BuildRun, now: number) {
  const elapsed = Math.max(0, now - run.timestamp);
  const pct = run.estimated
    ? Math.min(99, Math.round((elapsed / run.estimated) * 100))
    : 0;
  return {
    elapsed,
    pct,
    eta: run.timestamp + run.estimated,
    late: run.estimated > 0 && elapsed > run.estimated,
  };
}
const runState = (run?: BuildRun) =>
  !run
    ? "none"
    : run.building
      ? "building"
      : (run.result ?? "NOT_BUILT").toLowerCase();
const show = (v: unknown) =>
  v === null || v === undefined || v === ""
    ? "—"
    : typeof v === "string"
      ? v
      : JSON.stringify(v);

/** A model's Jenkins jobs: the cached read at once, then a fresh one; while any job builds,
 * it rereads every minute. */
function useBuilds(code: string) {
  const [data, setData] = useState<Builds | null>(null);
  const [loading, setLoading] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const live = useRef(true);
  async function refresh() {
    setLoading(true);
    setFailure(null);
    try {
      const fresh = await focus("builds", { code });
      if (live.current && fresh.builds) setData(fresh.builds);
    } catch (error) {
      if (live.current) setFailure(failureOf(error));
    } finally {
      if (live.current) setLoading(false);
    }
  }
  useEffect(() => {
    live.current = true;
    setData(null);
    void (async () => {
      try {
        const cached = await focus("builds_cached", { code });
        if (live.current && cached.builds) setData(cached.builds);
      } catch {
        /* the fresh read reports */
      }
      await refresh();
    })();
    return () => {
      live.current = false;
    };
  }, [code]);
  const building = !!data?.jobs.some((j) => j.builds[0]?.building);
  useEffect(() => {
    const tick = setInterval(() => setNow(Date.now()), 30000);
    const reread = building
      ? setInterval(() => void refresh(), 60000)
      : undefined;
    return () => {
      clearInterval(tick);
      if (reread) clearInterval(reread);
    };
  }, [building, code]);
  return { data, loading, failure, refresh, now };
}
type BuildsState = ReturnType<typeof useBuilds>;

function FlaggedRow({
  item,
  onReview,
}: {
  item: Flagged;
  onReview: (action: string) => void;
}) {
  return (
    <motion.article
      className="flagged"
      layout="position"
      initial={false}
      exit={{
        opacity: 0,
        height: 0,
        paddingTop: 0,
        paddingBottom: 0,
        overflow: "hidden",
      }}
    >
      <div className="flagged-tags">
        {item.rules.map((r) => (
          <span key={r} className={`tag tag-${r}`}>
            {ruleNames[r] ?? r}
          </span>
        ))}
      </div>
      <span className="flagged-when">
        {item.created.slice(5)} ·{" "}
        {item.status === "MERGED"
          ? "已合入"
          : item.status === "ABANDONED"
            ? "已放弃"
            : "待评审"}
      </span>
      <p className="flagged-subject">{item.subject}</p>
      <span className="flagged-meta">
        {item.project} · {item.branch} · {item.owner} · {item.number}
      </span>
      <div className="flagged-actions">
        <Button size="sm" onClick={() => onReview("accept")}>
          接受
        </Button>
        <Button
          size="sm"
          variant={item.tracked ? "secondary" : "outline"}
          aria-pressed={item.tracked}
          onClick={() => onReview(item.tracked ? "untrack" : "track")}
        >
          {item.tracked ? "正在跟踪" : "继续跟踪"}
        </Button>
        <span className="flagged-meta">
          {item.issues.length
            ? `关联 ${item.issues.join("、")}`
            : "没有关联问题"}
        </span>
      </div>
    </motion.article>
  );
}

/** The newest build in one line: a progress bar while it runs, else how it ended. */
function RunLine({ job, now }: { job: JobView; now: number }) {
  const run = job.builds[0];
  if (!run) return <span className="quiet">没有保留的构建</span>;
  if (run.building) {
    const p = progress(run, now);
    return (
      <div className="run-line building">
        <Progress
          value={p.pct}
          aria-label={`${job.name} #${run.number} 进度`}
        />
        <span>
          #{run.number} 已运行 {span(p.elapsed)} ·{" "}
          {p.late
            ? `比通常多 ${span(p.elapsed - run.estimated)}`
            : `预计 ${clock(p.eta)} 完成`}
        </span>
      </div>
    );
  }
  return (
    <span className={`run-line run-${runState(run)}`}>
      #{run.number} {resultNames[run.result ?? ""] ?? run.result ?? ""} ·{" "}
      {day(run.timestamp)} · 用时 {span(run.duration)}
    </span>
  );
}

/** Ten builds as bars: height is duration, colour is the result; the running one is striped. */
function History({ builds, now }: { builds: BuildRun[]; now: number }) {
  const runs = [...builds].reverse();
  const longest = Math.max(
    1,
    ...runs.map((r) =>
      r.building ? Math.max(now - r.timestamp, r.estimated) : r.duration,
    ),
  );
  return (
    <div className="build-history" aria-hidden="true">
      {runs.map((r) => {
        const d = r.building ? now - r.timestamp : r.duration;
        return (
          <span
            key={r.number}
            className={`bar run-${runState(r)}`}
            style={{ height: `${Math.max(8, (d / longest) * 100)}%` }}
            title={`#${r.number} ${resultNames[r.result ?? ""] ?? "构建中"} · ${span(d)}`}
          />
        );
      })}
    </div>
  );
}

/** The parameters the job builds with now; ones the newest build changed show before → after. */
function Config({ job }: { job: JobView }) {
  const [all, setAll] = useState(false);
  const newest = job.builds.find((b) =>
    job.changes.some((c) => c.build === b.number),
  );
  const latest = job.changes.filter((c) => c.build === job.changes[0]?.build);
  const changed = new Set(latest.map((c) => c.name));
  const names = Object.keys(job.parameters);
  if (!names.length)
    return <p className="quiet config-empty">这个任务的构建不带参数。</p>;
  const ordered = [...names].sort(
    (a, b) => Number(changed.has(b)) - Number(changed.has(a)),
  );
  const shown = all ? ordered : ordered.slice(0, Math.max(8, changed.size));
  const older = job.changes
    .filter((c) => c.build !== job.changes[0]?.build)
    .slice(0, 6);
  return (
    <div className="config">
      <div className="config-head">
        <span>当前配置</span>
        <span className="quiet">
          {job.changes.length && newest
            ? `#${newest.number} 改了 ${changed.size} 项`
            : `最近 ${job.builds.length} 次构建没有改动`}
        </span>
      </div>
      <dl className="config-grid">
        {shown.map((name) => {
          const c = latest.find((x) => x.name === name);
          return (
            <div key={name} className={c ? "changed" : ""}>
              <dt>{name}</dt>
              <dd>
                {c ? <s>{show(c.before)}</s> : null}
                <span>{show(job.parameters[name])}</span>
              </dd>
            </div>
          );
        })}
      </dl>
      {ordered.length > shown.length || all ? (
        <button type="button" className="link" onClick={() => setAll(!all)}>
          {all ? "收起" : `全部 ${ordered.length} 项`}
        </button>
      ) : null}
      {older.length ? (
        <ul className="config-history">
          {older.map((c) => (
            <li key={`${c.build}-${c.name}`}>
              <span className="quiet">#{c.build}</span> {c.name}{" "}
              <s>{show(c.before)}</s> → {show(c.after)}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function JobRow({ job, now }: { job: JobView; now: number }) {
  return (
    <article
      className={`job job-${runState(job.builds[0])}`}
      aria-label={job.name}
    >
      <div className="job-main">
        <div className="job-title">
          <strong>{job.name}</strong>
          <span className={`phase phase-${job.phase}`}>
            {phaseNames[job.phase]}
          </span>
          {job.disabled ? <span className="quiet">已停用</span> : null}
        </div>
        {job.error ? (
          <span className="short">{job.error}</span>
        ) : (
          <>
            <RunLine job={job} now={now} />
            <div className="job-meta">
              <History builds={job.builds} now={now} />
              <span className="quiet">
                {job.builds[0]?.cause ?? ""}
                {job.builds[0]?.node ? ` · ${job.builds[0].node}` : ""}
              </span>
            </div>
          </>
        )}
      </div>
      {job.error ? null : <Config job={job} />}
    </article>
  );
}

function BuildsTab({ builds }: { builds: BuildsState }) {
  const { data, now } = builds;
  const [retired, setRetired] = useState(false);
  const active =
    data?.jobs.filter((j) => !j.disabled && j.phase !== "old") ?? [];
  const rest = data?.jobs.filter((j) => j.disabled || j.phase === "old") ?? [];
  return (
    <div className="builds-tab">
      <div className="tab-head">
        <span className="quiet">
          Jenkins{data?.folder ? ` · ${data.folder}` : ""}
          {data ? ` · ${since(data.computed_at)} 读取` : ""}
        </span>
        <Button
          variant="outline"
          size="sm"
          disabled={builds.loading}
          onClick={() => void builds.refresh()}
        >
          {builds.loading ? <Spinner aria-hidden="true" /> : <RefreshCw />}
          刷新
        </Button>
      </div>
      {builds.failure ? <FailureNote failure={builds.failure} /> : null}
      {data?.errors.length ? (
        <FailureNote
          failure={{ code: "source_failed", message: data.errors.join("；") }}
        />
      ) : null}
      {!data ? (
        <div className="job-skeletons" aria-hidden="true">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="job-skeleton" />
          ))}
        </div>
      ) : (
        phaseOrder
          .filter((p) => p !== "old")
          .map((p) => {
            const jobs = active.filter((j) => j.phase === p);
            return jobs.length ? (
              <section key={p} className="job-group" aria-label={phaseNames[p]}>
                <h2>{phaseNames[p]}</h2>
                {jobs.map((j) => (
                  <JobRow key={j.job} job={j} now={now} />
                ))}
              </section>
            ) : null;
          })
      )}
      {rest.length ? (
        <section className="job-group retired">
          <button
            type="button"
            className="link"
            onClick={() => setRetired(!retired)}
          >
            {retired ? "收起" : `旧的和已停用的任务 ${rest.length} 个`}
          </button>
          {retired
            ? rest.map((j) => <JobRow key={j.job} job={j} now={now} />)
            : null}
        </section>
      ) : null}
    </div>
  );
}

/** Branches the model follows, grouped by phase; strips from the summary. */
function branchGroups(m: ModelSummary, saved?: FocusModel) {
  const strips = new Map(m.branches.map((b) => [b.branch, b]));
  for (const name of saved?.branches ?? [])
    if (!strips.has(name))
      strips.set(name, {
        branch: name,
        phase: phaseOf(name),
        total: 0,
        days: Array(84).fill(0),
        flagged: 0,
      });
  const all = [...strips.values()];
  return phaseOrder
    .map((p) => ({ phase: p, branches: all.filter((b) => b.phase === p) }))
    .filter((g) => g.branches.length);
}
/** The same reading of names as SPM, for followed branches without recent changes. */
export function phaseOf(name: string) {
  const u = name.toUpperCase();
  const marked = (m: string) =>
    u.endsWith(m) || u.includes(`${m}_`) || u.includes(`${m}-`);
  return marked("_OLD")
    ? "old"
    : marked("_IR")
      ? "ir"
      : marked("_PB")
        ? "pb"
        : marked("_MP")
          ? "mp"
          : "main";
}
/** Jobs whose current parameters name the branch; otherwise jobs of the same phase whose name,
 * without its phase suffix, starts the branch name (E6767SM_IR → E6767SM_SYS_A16_IR). */
export function jobsFor(branch: string, phase: string, jobs: JobView[] = []) {
  const named = jobs.filter((j) =>
    Object.values(j.parameters).some(
      (v) => typeof v === "string" && v.split(/[\s,;:]+/).includes(branch),
    ),
  );
  if (named.length) return named;
  return jobs.filter((j) => {
    if (j.disabled || j.phase !== phase) return false;
    const stem = j.name.replace(/_(IR|PB|MP)$/i, "");
    return branch.toUpperCase().startsWith(`${stem.toUpperCase()}_`);
  });
}

const stateNames: Record<string, string> = {
  flagged: "需要留意",
  earlier: "两周前命中",
  tracked: "正在跟踪",
  accepted: "已接受",
  logged: "只记录",
  ignored: "已忽略",
  clean: "无规则命中",
};
const filters = [
  ["attention", "需要留意"],
  ["all", "全部"],
  ["logged", "只记录"],
  ["ignored", "已忽略"],
] as const;
const inFilter = (l: Listed, f: string) =>
  f === "all" ||
  (f === "attention"
    ? ["flagged", "tracked"].includes(l.state)
    : l.state === f);

function BranchesTab({
  m,
  saved,
  builds,
  selected,
  onSelect,
}: {
  m: ModelSummary;
  saved?: FocusModel;
  builds: BuildsState;
  selected: string | null;
  onSelect: (branch: string) => void;
}) {
  const groups = branchGroups(m, saved);
  const branch = selected ?? groups[0]?.branches[0]?.branch ?? null;
  const strip = groups
    .flatMap((g) => g.branches)
    .find((b) => b.branch === branch);
  const [list, setList] = useState<Listed[] | null>(null);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [filter, setFilter] = useState<string>("attention");
  useEffect(() => {
    if (!branch) return;
    let live = true;
    setList(null);
    setFailure(null);
    focus("changes", { code: m.code, branch })
      .then((d) => live && setList(d.changes ?? []))
      .catch((e) => live && setFailure(failureOf(e)));
    return () => {
      live = false;
    };
  }, [m.code, branch, m]);
  useEffect(() => {
    if (
      list &&
      filter === "attention" &&
      !list.some((l) => inFilter(l, "attention"))
    )
      setFilter("all");
  }, [list]);
  const jobs =
    branch && strip ? jobsFor(branch, strip.phase, builds.data?.jobs) : [];
  const shown = list?.filter((l) => inFilter(l, filter)) ?? [];
  return (
    <div className="branches-tab">
      <nav className="branch-rail" aria-label="分支">
        {groups.map((g) => (
          <section key={g.phase}>
            <h2>{phaseNames[g.phase]}</h2>
            {g.branches.map((b) => (
              <button
                type="button"
                key={b.branch}
                className="branch-pick"
                aria-current={b.branch === branch ? "true" : undefined}
                onClick={() => onSelect(b.branch)}
              >
                <span className="branch-pick-name">
                  <span>{b.branch}</span>
                  {b.flagged ? (
                    <span className="short">{b.flagged}</span>
                  ) : null}
                </span>
                <Strip days={b.days} />
              </button>
            ))}
          </section>
        ))}
      </nav>
      {branch && strip ? (
        <section className="branch-detail" aria-label={branch}>
          <header>
            <span className={`phase phase-${strip.phase}`}>
              {phaseNames[strip.phase]}
            </span>
            <h2 className="branch-name">{branch}</h2>
            <p className="quiet">
              近 12 周 {strip.total} 个变更 · 需要留意 {strip.flagged}
            </p>
          </header>
          {jobs.length ? (
            <div className="branch-jobs">
              {jobs.map((j) => (
                <div
                  key={j.job}
                  className={`branch-job job-${runState(j.builds[0])}`}
                >
                  <strong>{j.name}</strong>
                  <RunLine job={j} now={builds.now} />
                </div>
              ))}
            </div>
          ) : builds.data ? (
            <p className="quiet">没有对应这个分支的构建任务。</p>
          ) : null}
          <div className="segmented" role="radiogroup" aria-label="筛选">
            {filters.map(([key, label]) => (
              <button
                type="button"
                role="radio"
                key={key}
                aria-checked={filter === key}
                onClick={() => setFilter(key)}
              >
                {label}{" "}
                {list ? list.filter((l) => inFilter(l, key)).length : ""}
              </button>
            ))}
          </div>
          {failure ? <FailureNote failure={failure} /> : null}
          {!list && !failure ? (
            <Skeleton className="change-skeleton" />
          ) : (
            <ol className="change-list">
              {shown.map((l) => (
                <li key={l.number} className={`change state-${l.state}`}>
                  <span className="change-when">{l.created.slice(5)}</span>
                  <div>
                    <p>{l.subject}</p>
                    <span className="flagged-meta">
                      {stateNames[l.state]}
                      {l.rules.length
                        ? ` · ${l.rules.map((r) => ruleNames[r] ?? r).join("、")}`
                        : ""}{" "}
                      · {l.project} · {l.owner} · {l.number}
                    </span>
                  </div>
                </li>
              ))}
            </ol>
          )}
        </section>
      ) : (
        <p className="quiet">这个机型还没有关注的分支。</p>
      )}
    </div>
  );
}

function Overview({
  m,
  saved,
  state,
  builds,
  onBranch,
  onBuilds,
}: {
  m: ModelSummary;
  saved?: FocusModel;
  state: FocusState;
  builds: BuildsState;
  onBranch: (branch: string) => void;
  onBuilds: () => void;
}) {
  const jobs =
    builds.data?.jobs.filter((j) => !j.disabled && j.phase !== "old") ?? [];
  return (
    <div className="model-columns">
      <section className="flagged-list" aria-label="需要留意">
        <div className="column-head">
          <h2>需要留意</h2>
          <span className="quiet">规则命中 · 近两周与正在跟踪的</span>
        </div>
        <AnimatePresence initial={false}>
          {m.flagged.map((f) => (
            <FlaggedRow
              key={f.number}
              item={f}
              onReview={(a) => void state.review(m.code, f.number, a)}
            />
          ))}
        </AnimatePresence>
        {m.flagged.length ? null : (
          <p className="quiet empty-line">近两周没有需要留意的变更。</p>
        )}
      </section>
      <aside className="model-side">
        <section aria-label="构建">
          <div className="column-head">
            <h2>构建</h2>
            <button type="button" className="link" onClick={onBuilds}>
              全部
            </button>
          </div>
          {!builds.data ? (
            builds.failure ? (
              <span className="short">
                {builds.failure.message ?? "Jenkins 读取失败"}
              </span>
            ) : (
              <span className="quiet loading-line">
                <Spinner aria-hidden="true" /> 正在读取 Jenkins…
              </span>
            )
          ) : jobs.length ? (
            jobs.map((j) => (
              <button
                type="button"
                key={j.job}
                className={`job-line job-${runState(j.builds[0])}`}
                onClick={onBuilds}
              >
                <span className="job-line-name">
                  <span className={`phase phase-${j.phase}`}>
                    {phaseNames[j.phase]}
                  </span>
                  {j.name}
                </span>
                <RunLine job={j} now={builds.now} />
              </button>
            ))
          ) : (
            <span className="quiet">
              {builds.data.errors[0] ?? "没有找到构建任务"}
            </span>
          )}
        </section>
        <section aria-label="分支">
          <div className="column-head">
            <h2>分支</h2>
            <span className="quiet">近 12 周</span>
          </div>
          {branchGroups(m, saved).map((g) => (
            <div key={g.phase} className="phase-group">
              <span className={`phase phase-${g.phase}`}>
                {phaseNames[g.phase]}
              </span>
              {g.branches.map((b) => (
                <button
                  type="button"
                  className="branch-strip"
                  key={b.branch}
                  onClick={() => onBranch(b.branch)}
                >
                  <div>
                    <span>{b.branch}</span>
                    <span className="quiet">{b.total}</span>
                  </div>
                  <Strip days={b.days} />
                </button>
              ))}
            </div>
          ))}
        </section>
      </aside>
    </div>
  );
}

const actionOptions = Object.entries(actionNames).map(([value, label]) => ({
  value,
  label,
}));
type Draft = Pick<FocusModel, "rules" | "freeze" | "targets">;
const draftOf = (m: FocusModel): Draft => ({
  rules: { ...defaultRules, ...m.rules },
  freeze: m.freeze ?? "",
  targets: { ...m.targets },
});
const bounded = (v: string, max: number) =>
  Math.max(0, Math.min(max, Math.round(Number(v) || 0)));
const targetNames = {
  severe_pct: "严重问题解决率门槛",
  overall_pct: "整体解决率门槛",
  fatal_max: "致命问题上限",
} as const;
const modelOf = (saved: FocusModel, draft: Draft): FocusModel => {
  const { freeze, ...rest } = draft;
  return { ...saved, ...rest, freeze: freeze || undefined };
};

function Delta({ before, after }: { before: number; after: number }) {
  const d = after - before;
  return (
    <span className="delta">
      <span className="quiet">{before}</span> →{" "}
      <strong>
        <Num value={after} />
      </strong>
      {d ? (
        <span className={d > 0 ? "up" : "down"}>
          <Num value={d} sign />
        </span>
      ) : null}
    </span>
  );
}

function GateDelta({
  label,
  v,
  n,
  before,
  after,
}: {
  label: string;
  v: number;
  n: number;
  before: number;
  after: number;
}) {
  const b = gate(v, n, before);
  const a = gate(v, n, after);
  return (
    <div className="preview-line">
      <span>{label}</span>
      <span className="delta">
        <span className="quiet">{b.ok ? "达标" : `还差 ${b.gap}`}</span> →{" "}
        <strong className={a.ok ? "" : "short"}>
          {a.ok ? "达标" : `还差 ${a.gap} 个`}
        </strong>
      </span>
    </div>
  );
}

/** Before and after: 需要留意, alerts, rule counts that move, and the changes that enter or
 * leave; gates against the new targets come from the summary's measures. */
function PreviewPanel({
  m,
  saved,
  draft,
  dirty,
}: {
  m: ModelSummary;
  saved: FocusModel;
  draft: Draft;
  dirty: boolean;
}) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [busy, setBusy] = useState(false);
  const key = JSON.stringify(draft);
  useEffect(() => {
    let live = true;
    setBusy(true);
    const timer = setTimeout(() => {
      focus("preview", { model: modelOf(saved, draft) })
        .then((d) => {
          if (!live) return;
          setPreview(d.preview ?? null);
          setFailure(null);
        })
        .catch((e) => live && setFailure(failureOf(e)))
        .finally(() => live && setBusy(false));
    }, 300);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [key, saved]);
  const moved =
    preview && dirty
      ? ruleOrder.filter(
          (r) =>
            (preview.before.rule_counts[r] ?? 0) !==
            (preview.after.rule_counts[r] ?? 0),
        )
      : [];
  const targets = m.measures
    ? (
        [
          [
            "严重问题",
            m.measures.severe_resolved,
            m.measures.severe_total,
            "severe_pct",
          ],
          [
            "整体",
            m.measures.overall_resolved,
            m.measures.overall_total,
            "overall_pct",
          ],
        ] as const
      ).filter(([, , , k]) => saved.targets[k] !== draft.targets[k])
    : [];
  const fatalMoved = saved.targets.fatal_max !== draft.targets.fatal_max;
  return (
    <aside
      className={dirty ? "preview-panel dirty" : "preview-panel"}
      aria-label="保存前后"
    >
      <div className="column-head">
        <h2>{dirty ? "保存后" : "现在"}</h2>
        {busy ? <Spinner aria-hidden="true" /> : null}
      </div>
      {failure ? <FailureNote failure={failure} /> : null}
      {preview ? (
        <>
          <div className="preview-big">
            <span>需要留意</span>
            {dirty ? (
              <Delta
                before={preview.before.flagged}
                after={preview.after.flagged}
              />
            ) : (
              <strong>
                <Num value={preview.before.flagged} />
              </strong>
            )}
          </div>
          <div className="preview-line">
            <span>其中提醒</span>
            {dirty ? (
              <Delta
                before={preview.before.alerts}
                after={preview.after.alerts}
              />
            ) : (
              <span>{preview.before.alerts}</span>
            )}
          </div>
          {moved.map((r) => (
            <div className="preview-line" key={r}>
              <span>{ruleNames[r]}</span>
              <Delta
                before={preview.before.rule_counts[r] ?? 0}
                after={preview.after.rule_counts[r] ?? 0}
              />
            </div>
          ))}
          {targets.map(([label, v, n, k]) => (
            <GateDelta
              key={k}
              label={`${label} ${saved.targets[k]}% → ${draft.targets[k]}%`}
              v={v}
              n={n}
              before={saved.targets[k]}
              after={draft.targets[k]}
            />
          ))}
          {fatalMoved && m.measures ? (
            <div className="preview-line">
              <span>
                致命上限 {saved.targets.fatal_max} → {draft.targets.fatal_max}
              </span>
              <strong
                className={
                  m.measures.fatal_open > draft.targets.fatal_max ? "short" : ""
                }
              >
                {m.measures.fatal_open > draft.targets.fatal_max
                  ? "超出"
                  : "达标"}
              </strong>
            </div>
          ) : null}
          {dirty && (preview.entering.length || preview.leaving.length) ? (
            <div className="preview-moves">
              {preview.entering.length ? (
                <>
                  <span className="kicker">
                    新出现 {preview.entering.length}
                  </span>
                  {preview.entering.slice(0, 5).map((f) => (
                    <p key={f.number} className="move in">
                      {f.subject}
                    </p>
                  ))}
                </>
              ) : null}
              {preview.leaving.length ? (
                <>
                  <span className="kicker">
                    不再出现 {preview.leaving.length}
                  </span>
                  {preview.leaving.slice(0, 5).map((f) => (
                    <p key={f.number} className="move out">
                      {f.subject}
                    </p>
                  ))}
                </>
              ) : null}
            </div>
          ) : null}
          <span className="quiet">
            按上次读取的 {preview.changes} 个变更试算
          </span>
        </>
      ) : !failure ? (
        <Skeleton className="bar" />
      ) : null}
    </aside>
  );
}

/** Rules, freeze date and targets, edited in place, previewed, then saved together. */
function SettingsTab({
  m,
  saved,
  state,
  onEdit,
  onRemoved,
}: {
  m: ModelSummary;
  saved: FocusModel;
  state: FocusState;
  onEdit: () => void;
  onRemoved: () => void;
}) {
  const [draft, setDraft] = useState<Draft>(() => draftOf(saved));
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [confirm, setConfirm] = useState(false);
  useEffect(() => setDraft(draftOf(saved)), [saved]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(draftOf(saved));
  const target = (key: keyof Draft["targets"], max: number) => (
    <Input
      type="number"
      min={0}
      max={max}
      className="target-input"
      aria-label={targetNames[key]}
      value={draft.targets[key]}
      onChange={(e) =>
        setDraft({
          ...draft,
          targets: { ...draft.targets, [key]: bounded(e.target.value, max) },
        })
      }
    />
  );
  async function save() {
    setSaving(true);
    setFailure(null);
    try {
      await state.save(modelOf(saved, draft));
    } catch (error) {
      setFailure(failureOf(error));
    } finally {
      setSaving(false);
    }
  }
  async function remove() {
    setConfirm(false);
    try {
      await state.remove(saved.code);
      onRemoved();
    } catch (error) {
      setFailure(failureOf(error));
    }
  }
  return (
    <div className="settings-tab">
      <div className="settings-form">
        <section aria-label="规则">
          <div className="column-head">
            <h2>规则</h2>
            <span className="quiet">近 13 周命中</span>
          </div>
          {ruleOrder.map((r) => (
            <div className="rule-block" key={r}>
              <div className="rule-line">
                <span>
                  {ruleNames[r]}
                  <small className="quiet">{ruleHow[r]}</small>
                </span>
                <Select
                  value={draft.rules[r]}
                  items={actionOptions}
                  onValueChange={(a) =>
                    a &&
                    setDraft({
                      ...draft,
                      rules: { ...draft.rules, [r]: a as string },
                    })
                  }
                >
                  <SelectTrigger
                    size="sm"
                    className={`action-select action-${draft.rules[r]}`}
                    aria-label={`${ruleNames[r]}的处理`}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {actionOptions.map((o) => (
                      <SelectItem key={o.value} value={o.value}>
                        {o.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <span className="quiet">{m.rule_counts[r] ?? 0}</span>
              </div>
              {r === "feature_after_freeze" ? (
                <label
                  className={draft.freeze ? "freeze-line" : "freeze-line unset"}
                >
                  <span>
                    {draft.freeze
                      ? "MP 冻结日期"
                      : "设置 MP 冻结日期后才会命中"}
                  </span>
                  <Input
                    type="date"
                    aria-label="MP 冻结日期"
                    value={draft.freeze}
                    onChange={(e) =>
                      setDraft({ ...draft, freeze: e.target.value })
                    }
                  />
                </label>
              ) : null}
            </div>
          ))}
        </section>
        <section aria-label="门槛">
          <div className="column-head">
            <h2>门槛</h2>
          </div>
          <div className="target-line">
            <span>严重问题解决率不低于</span>
            {target("severe_pct", 100)}
            <span className="quiet">%</span>
          </div>
          <div className="target-line">
            <span>整体解决率不低于</span>
            {target("overall_pct", 100)}
            <span className="quiet">%</span>
          </div>
          <div className="target-line">
            <span>致命问题未解决不超过</span>
            {target("fatal_max", 9999)}
            <span className="quiet">个</span>
          </div>
        </section>
        {failure ? <FailureNote failure={failure} /> : null}
        {dirty || saving ? (
          <div className="settings-bar" role="status">
            <span>
              {saving ? "正在保存并按新设置重新读取…" : "有未保存的改动"}
            </span>
            <div>
              <Button
                variant="ghost"
                size="sm"
                className="bar-discard"
                disabled={saving}
                onClick={() => setDraft(draftOf(saved))}
              >
                放弃
              </Button>
              <Button
                size="sm"
                className="bar-save"
                disabled={saving}
                onClick={() => void save()}
              >
                {saving ? <Spinner aria-hidden="true" /> : null}
                保存
              </Button>
            </div>
          </div>
        ) : null}
        <section className="model-manage" aria-label="管理">
          <button type="button" className="link" onClick={onEdit}>
            更换问题来源和分支
          </button>
          <button
            type="button"
            className="link danger"
            onClick={() => setConfirm(true)}
          >
            停止关注
          </button>
        </section>
      </div>
      <PreviewPanel m={m} saved={saved} draft={draft} dirty={dirty} />
      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent>
          <DialogTitle>停止关注 {saved.code}？</DialogTitle>
          <DialogDescription>
            它的规则、接受和跟踪记录会一起删除。之后可以重新关注，但这些记录不会恢复。
          </DialogDescription>
          <DialogFooter>
            <DialogClose render={<Button variant="ghost" />}>取消</DialogClose>
            <Button variant="destructive" onClick={() => void remove()}>
              停止关注
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** 机型: the head with its gates, then 概览, 分支, 构建 and 设置. */
export function ModelPage({
  state,
  code,
  tab = "overview",
  onTab = () => {},
  onEdit,
  onRemoved,
}: {
  state: FocusState;
  code: string;
  tab?: ModelTab;
  onTab?: (tab: ModelTab) => void;
  onEdit: () => void;
  onRemoved: () => void;
}) {
  const m = state.summary?.models.find((x) => x.code === code);
  const saved = state.models?.find((x) => x.code === code);
  const builds = useBuilds(code);
  const [branch, setBranch] = useState<string | null>(null);
  useEffect(() => setBranch(null), [code]);
  const running = useMemo(
    () => builds.data?.jobs.filter((j) => j.builds[0]?.building).length ?? 0,
    [builds.data],
  );
  if (!m)
    return (
      <div className="model-page">
        <h1 className="model-code">{code}</h1>
        <p className="quiet">正在读取…</p>
      </div>
    );
  const sev = m.measures
    ? gate(
        m.measures.severe_resolved,
        m.measures.severe_total,
        m.targets.severe_pct,
      )
    : null;
  const all = m.measures
    ? gate(
        m.measures.overall_resolved,
        m.measures.overall_total,
        m.targets.overall_pct,
      )
    : null;
  return (
    <div className="model-page">
      <section className="model-head">
        <div>
          <h1 className="model-code">{m.code}</h1>
          <p className="quiet">
            {sourceText(m.sources)} · Gerrit {m.branches.length} 个分支 ·{" "}
            {saved?.freeze ? `MP 冻结于 ${saved.freeze}` : "未设 MP 冻结日期"}
          </p>
        </div>
        <div className="model-figures">
          {sev ? (
            <div>
              <span>严重问题解决率</span>
              <strong className={sev.ok ? "" : "short"}>
                <Num value={sev.pct} suffix="%" />
              </strong>
              <small className={sev.ok ? "" : "short"}>
                {sev.v} / {sev.n} · {sev.ok ? "达标" : `还差 ${sev.gap} 个`}
              </small>
            </div>
          ) : null}
          {all ? (
            <div>
              <span>整体解决率</span>
              <strong className={all.ok ? "" : "short"}>
                <Num value={all.pct} suffix="%" />
              </strong>
              <small className={all.ok ? "" : "short"}>
                {all.v} / {all.n} · {all.ok ? "达标" : `还差 ${all.gap} 个`}
              </small>
            </div>
          ) : null}
          <div>
            <span>本周</span>
            <strong>
              <Num value={m.new_issues} prefix="+" />
            </strong>
            <small>
              新问题 · {m.week_changes} 变更 · 合入 {m.week_merged}
            </small>
          </div>
        </div>
      </section>
      <nav className="model-subnav" aria-label={`${m.code} 页面`}>
        {(Object.keys(tabNames) as ModelTab[]).map((t) => (
          <button
            type="button"
            key={t}
            aria-current={tab === t ? "page" : undefined}
            onClick={() => onTab(t)}
          >
            {tabNames[t]}
            {tab === t ? <TabLine id="subnav" /> : null}
            {t === "overview" && m.flagged.length ? (
              <span className="tab-count short">{m.flagged.length}</span>
            ) : null}
            {t === "builds" && running ? (
              <span className="tab-count building">{running} 个构建中</span>
            ) : null}
            {t === "settings" && !saved?.freeze ? (
              <span className="tab-dot" aria-label="未设冻结日期" />
            ) : null}
          </button>
        ))}
      </nav>
      {m.errors.length ? (
        <FailureNote
          failure={{ code: "source_failed", message: m.errors.join("；") }}
        />
      ) : null}
      {tab === "overview" ? (
        <Overview
          m={m}
          saved={saved}
          state={state}
          builds={builds}
          onBranch={(b) => {
            setBranch(b);
            onTab("branches");
          }}
          onBuilds={() => onTab("builds")}
        />
      ) : tab === "branches" ? (
        <BranchesTab
          m={m}
          saved={saved}
          builds={builds}
          selected={branch}
          onSelect={setBranch}
        />
      ) : tab === "builds" ? (
        <BuildsTab builds={builds} />
      ) : saved ? (
        <SettingsTab
          m={m}
          saved={saved}
          state={state}
          onEdit={onEdit}
          onRemoved={onRemoved}
        />
      ) : null}
    </div>
  );
}
