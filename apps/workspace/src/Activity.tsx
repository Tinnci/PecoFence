import { useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import type { Activity, DayActivity, SetupFailure } from "./contracts/setup";
import { discover } from "./api";
import { FailureNote, failureOf } from "./TemplateImport";

const REPO_KEY = "workspace.activity.repo";
const levels = ["#edf0e8", "#c9dcc0", "#9cbd8f", "#6f9466", "#405b3e"];
const weekdays = ["一", "", "三", "", "五", "", ""];

function remember(): string {
  try {
    return localStorage.getItem(REPO_KEY) ?? "";
  } catch {
    return "";
  }
}
function keep(repo: string) {
  try {
    localStorage.setItem(REPO_KEY, repo);
  } catch {
    /* Remembering the repository is a convenience only. */
  }
}

/** Thresholds at the quartiles of the busy days, as GitHub's grid does. */
export function thresholds(days: DayActivity[]) {
  const busy = days
    .map((d) => d.created)
    .filter((n) => n > 0)
    .sort((a, b) => a - b);
  if (!busy.length) return [1, 1, 1];
  const at = (q: number) =>
    busy[Math.min(busy.length - 1, Math.floor(q * busy.length))];
  return [at(0.25), at(0.5), at(0.75)];
}
export function level(n: number, cuts: number[]) {
  if (n === 0) return 0;
  if (n <= cuts[0]) return 1;
  if (n <= cuts[1]) return 2;
  if (n <= cuts[2]) return 3;
  return 4;
}

/** Monday-first weeks; the first column is padded so each row is one weekday. */
export function weeks(days: DayActivity[]) {
  if (!days.length) return [];
  const first = new Date(`${days[0].date}T00:00:00`);
  const pad = (first.getDay() + 6) % 7;
  const cells: (DayActivity | null)[] = [
    ...Array.from({ length: pad }, () => null),
    ...days,
  ];
  const columns: (DayActivity | null)[][] = [];
  for (let i = 0; i < cells.length; i += 7) columns.push(cells.slice(i, i + 7));
  return columns;
}

/** A month's label sits on its first week; one starting within three columns of the next is dropped. */
export function monthLabels(columns: (DayActivity | null)[][]) {
  const starts: [number, string][] = [];
  columns.forEach((week, i) => {
    const day = week.find(Boolean);
    const previous = columns[i - 1]?.find(Boolean);
    if (
      day &&
      (!previous || previous.date.slice(5, 7) !== day.date.slice(5, 7))
    )
      starts.push([i, `${Number(day.date.slice(5, 7))}月`]);
  });
  const labels = columns.map(() => "");
  starts.forEach(([i, label], k) => {
    const next = starts[k + 1];
    if (!next || next[0] - i >= 3) labels[i] = label;
  });
  return labels;
}

function Grid({ activity }: { activity: Activity }) {
  const [hover, setHover] = useState<DayActivity | null>(null);
  const columns = useMemo(() => weeks(activity.days), [activity]);
  const cuts = useMemo(() => thresholds(activity.days), [activity]);
  const months = monthLabels(columns);
  return (
    <div className="activity-grid-wrap">
      <div
        className="activity-grid"
        role="img"
        aria-label={`${activity.total} 个变更的每日分布`}
      >
        <div className="grid-months" aria-hidden="true">
          {months.map((m, i) => (
            <span key={i}>{m}</span>
          ))}
        </div>
        <div className="grid-weekdays" aria-hidden="true">
          {weekdays.map((d, i) => (
            <span key={i}>{d}</span>
          ))}
        </div>
        <div className="grid-cells">
          {columns.map((week, i) => (
            <div className="grid-week" key={i}>
              {week.map((day, j) =>
                day ? (
                  <span
                    key={day.date}
                    className="grid-cell"
                    style={{ background: levels[level(day.created, cuts)] }}
                    title={`${day.date} · ${day.created} 个变更`}
                    onMouseEnter={() => setHover(day)}
                    onMouseLeave={() => setHover(null)}
                  />
                ) : (
                  <span key={`pad-${j}`} className="grid-cell pad" />
                ),
              )}
            </div>
          ))}
        </div>
      </div>
      <div className="grid-foot">
        <span className="grid-hover" aria-live="polite">
          {hover
            ? `${hover.date} · ${hover.created} 个变更${hover.created ? `，已合入 ${hover.merged} 个` : ""}`
            : ""}
        </span>
        <span className="grid-legend" aria-hidden="true">
          少
          {levels.map((c) => (
            <i key={c} style={{ background: c }} />
          ))}
          多
        </span>
      </div>
    </div>
  );
}

/** 代码活动: Gerrit changes per day for a repository or branch, a year at a glance. */
export default function ActivityPage() {
  const [projects, setProjects] = useState<string[] | null>(null);
  const [repo, setRepo] = useState(remember);
  const [query, setQuery] = useState(remember);
  const [branch, setBranch] = useState<string | null>(null);
  const [activity, setActivity] = useState<Activity | null>(null);
  const [loading, setLoading] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  const [open, setOpen] = useState(false);
  // The repository's branches, kept while one branch is shown.
  const [repoBranches, setRepoBranches] = useState<Activity["branches"]>([]);

  useEffect(() => {
    discover("gerrit_projects")
      .then((d) => setProjects(d.gerrit_projects ?? []))
      .catch((error) => setFailure(failureOf(error)));
  }, []);
  useEffect(() => {
    if (!repo) return;
    let current = true;
    setLoading(true);
    setFailure(null);
    discover("gerrit_activity", {
      repo,
      ...(branch ? { branch } : {}),
    })
      .then((d) => {
        if (!current) return;
        setActivity(d.activity ?? null);
        if (!branch) setRepoBranches(d.activity?.branches ?? []);
      })
      .catch((error) => {
        if (current) setFailure(failureOf(error));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [repo, branch]);

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!projects || !q) return [];
    return projects.filter((p) => p.toLowerCase().includes(q)).slice(0, 8);
  }, [projects, query]);
  function pick(name: string) {
    setRepo(name);
    setQuery(name);
    setBranch(null);
    setOpen(false);
    keep(name);
  }
  const branches = repoBranches;
  const max = Math.max(1, ...branches.map((b) => b.count));

  return (
    <div className="activity">
      <div className="activity-head">
        <div className="repo-picker">
          <label className="pick-search">
            <Search aria-hidden="true" />
            <Input
              value={query}
              placeholder={
                projects ? `搜索 ${projects.length} 个仓库` : "正在读取仓库…"
              }
              aria-label="仓库"
              autoFocus={!repo}
              onChange={(e) => {
                setQuery(e.target.value);
                setOpen(true);
              }}
              onFocus={() => setOpen(true)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && matches[0]) pick(matches[0]);
                if (e.key === "Escape") setOpen(false);
              }}
            />
          </label>
          {open && matches.length ? (
            <div
              className="repo-matches"
              role="listbox"
              aria-label="匹配的仓库"
            >
              {matches.map((m) => (
                <button
                  type="button"
                  role="option"
                  aria-selected={m === repo}
                  key={m}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => pick(m)}
                >
                  {m}
                </button>
              ))}
            </div>
          ) : null}
        </div>
      </div>
      {failure ? <FailureNote failure={failure} /> : null}
      {!repo ? (
        <p className="scope-hint">选择一个仓库，查看它过去一年每天的变更。</p>
      ) : (
        <section className="activity-card" aria-label="变更分布">
          <div className="activity-summary">
            {activity && !loading ? (
              <>
                <div className="big-count">
                  <strong key={activity.total}>{activity.total}</strong>
                  <span>
                    个变更 · 过去一年
                    {activity.branch ? ` · ${activity.branch}` : ""}
                  </span>
                </div>
                <dl className="activity-status">
                  <div>
                    <dt>已合入</dt>
                    <dd>{activity.merged}</dd>
                  </div>
                  <div>
                    <dt>进行中</dt>
                    <dd>{activity.open}</dd>
                  </div>
                  <div>
                    <dt>已放弃</dt>
                    <dd>{activity.abandoned}</dd>
                  </div>
                </dl>
              </>
            ) : (
              <Skeleton className="count-skeleton" />
            )}
          </div>
          {activity && !loading ? (
            <Grid activity={activity} />
          ) : (
            <Skeleton className="grid-skeleton" />
          )}
          {branches.length ? (
            <div className="activity-branches">
              <div className="branch-head">
                <span className="preview-label">分支</span>
                {branch ? (
                  <button
                    type="button"
                    className="link"
                    onClick={() => setBranch(null)}
                  >
                    全部分支
                  </button>
                ) : null}
              </div>
              <div className="pick-list" role="listbox" aria-label="分支">
                {branches.slice(0, 8).map((b) => (
                  <button
                    type="button"
                    role="option"
                    aria-selected={b.branch === branch}
                    className="pick-row"
                    key={b.branch}
                    onClick={() =>
                      setBranch(b.branch === branch ? null : b.branch)
                    }
                  >
                    <span className="pick-name">{b.branch}</span>
                    <span className="pick-count">{b.count}</span>
                    <span className="pick-bar" aria-hidden="true">
                      <span
                        style={{
                          width: `${Math.max(2, Math.round((b.count / max) * 100))}%`,
                        }}
                      />
                    </span>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
        </section>
      )}
    </div>
  );
}
