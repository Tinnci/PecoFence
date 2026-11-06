import { ModelPlatform, profilesOptions } from "./Platforms";
import { ModelOtaTab, OtaLead } from "./Ota";
import {
  BuildActivity,
  DurationSummary,
  RunningEstimate,
} from "./BuildInsights";
import ReleaseChecklist from "./ReleaseChecklist";
import {
  sourceBranches,
  sourceRoutes,
  sourceTimelineJobs,
} from "./job-sources";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { JobSourcesEditor, SourceRoutingNote } from "./JobSources";
import {
  buildAssociation,
  withBranchJob,
  withBranchJobs,
} from "./build-association";
import { BranchBuildAssociation } from "./BranchBuildAssociation";
import { ManifestInputs } from "./ManifestInputs";
import {
  releaseBranchGroups as branchGroups,
  releaseJobs,
  releaseJobGroups,
} from "./release-lines";
export { namedPhase as phaseOf } from "./release-lines";
import { ReleaseLinesEditor, lifecycleName } from "./ReleaseLinesEditor";
import { branchTimeline } from "./branch-timeline";
import { BuildMarker, timelineTime } from "./BuildMarker";
import { ActionIcon } from "./components/action-icon";
import { IssueBrowser, LinkedIssues, type IssueFilter } from "./IssueBrowser";
import { ContextTarget } from "./components/context-menu";
import {
  buildContextActions,
  commitContextActions,
  copyContextAction,
  openContextAction,
} from "./context-actions";
import { ExternalLink } from "./components/external-link";
import {
  jobBrowserUrl,
  consoleBrowserUrl,
  gerritSearchUrl,
  gerritTerm,
} from "./browser-links";
import { useTranslation } from "react-i18next";
import { t, locale } from "./i18n";
import { ChangeFiles, ChangeStatus, prefetchChange } from "./ChangeDiff";
import { OwnerChip } from "./Badges";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { WorkItemsTab, WorkSummary, useWorkItems } from "./WorkItems";
import { useModelPreference } from "./model-navigation";
import { useQuery } from "@tanstack/react-query";
import {
  queryClient,
  buildsOptions,
  prefetchBuilds,
  changesOptions,
} from "./focus-cache";
import { Fragment, useEffect, useMemo, useState } from "react";
import { Archive, Info, RefreshCw } from "lucide-react";
import { ModelForecast, useForecasts } from "./Forecast";
import { Tabs } from "@base-ui/react/tabs";
import { AnimatePresence, m as motion } from "motion/react";
import { Num } from "@/components/motion";
import { Button } from "@/components/ui/button";
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
  JobSource,
  Listed,
  ModelSummary,
  Preview,
  SetupFailure,
  WorkItems,
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
  useTranslation();
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
  get main() {
    return t("model.mainline");
  },
  ir: "IR",
  pb: "PB",
  mp: "MP",
  get old() {
    return t("model.old");
  },
};
const tabNames: Record<ModelTab, string> = {
  get overview() {
    return t("model.overview");
  },
  get issues() {
    return t("model.issues");
  },
  get requirements() {
    return t("model.requirements");
  },
  get ota() {
    return t("nav.ota");
  },
  get branches() {
    return t("model.branches");
  },
  get builds() {
    return t("model.builds");
  },
  get checklist() {
    return t("releaseCheck.title");
  },
  get settings() {
    return t("model.settings");
  },
};
const resultNames: Record<string, string> = {
  get SUCCESS() {
    return t("model.success");
  },
  get FAILURE() {
    return t("model.failed");
  },
  get UNSTABLE() {
    return t("model.unstable");
  },
  get ABORTED() {
    return t("model.aborted");
  },
  get NOT_BUILT() {
    return t("model.notBuilt");
  },
};

/** 2 小时 43 分, 43 分, 不到 1 分. */
export function span(ms: number) {
  const minutes = Math.round(ms / 60000);
  if (minutes < 1) return t("model.lessThan1Min");
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h
    ? t("model.hr", { v0: h, v1: m ? t("model.min", { v0: m }) : "" })
    : t("model.min2", { v0: m });
}
const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString(locale(), {
    hour: "2-digit",
    minute: "2-digit",
  });
const day = (ms: number) =>
  new Date(ms).toLocaleString(locale(), {
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
export function useBuilds(code: string, enabled = true, watch = false) {
  const query = useQuery(
    { ...buildsOptions(code), enabled: false },
    queryClient,
  );
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (enabled) void prefetchBuilds(code);
  }, [code, enabled]);
  const building = !!query.data?.jobs.some((job) =>
    job.builds.some((run) => run.building),
  );
  const queued = !!query.data?.queue?.items.length;
  useEffect(() => {
    if (!enabled || (!building && !queued && !watch)) return;
    setNow(Date.now());
    const tick = setInterval(() => setNow(Date.now()), 30_000);
    const refresh = setInterval(() => {
      if (
        !document.hidden &&
        !queryClient.isFetching({ queryKey: buildsOptions(code).queryKey })
      )
        void query.refetch();
    }, 60_000);
    return () => {
      clearInterval(tick);
      clearInterval(refresh);
    };
  }, [enabled, building, queued, watch, code, query.refetch]);
  return {
    data: query.data ?? null,
    loading: query.isFetching,
    failure: query.error ? failureOf(query.error) : null,
    refresh: () => query.refetch(),
    now,
  };
}
type BuildsState = ReturnType<typeof useBuilds>;

function FlaggedRow({
  item,
  issues,
  onReview,
}: {
  item: Flagged;
  issues: NonNullable<ModelSummary["issue_links"]>;
  onReview: (action: string) => void | Promise<unknown>;
}) {
  useTranslation();
  return (
    <ContextTarget
      title={`Gerrit #${item.number}`}
      actions={commitContextActions(item, onReview)}
    >
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
            ? t("model.merged")
            : item.status === "ABANDONED"
              ? t("model.abandoned")
              : t("model.awaitingReview")}
        </span>
        <p className="flagged-subject">
          <ExternalLink href={item.url} contextMenu={false}>
            {item.subject}
          </ExternalLink>
        </p>
        <span className="flagged-meta">
          <ExternalLink
            href={gerritSearchUrl(
              item.url,
              gerritTerm("project", item.project),
            )}
          >
            {item.project}
          </ExternalLink>{" "}
          ·{" "}
          <ExternalLink
            href={gerritSearchUrl(
              item.url,
              `${gerritTerm("project", item.project)} ${gerritTerm("branch", item.branch)}`,
            )}
          >
            {item.branch}
          </ExternalLink>{" "}
          ·{" "}
          <ExternalLink
            href={gerritSearchUrl(item.url, gerritTerm("owner", item.owner))}
          >
            <OwnerChip name={item.owner} />
          </ExternalLink>{" "}
          · {item.number}
        </span>
        <div className="flagged-actions">
          <Button size="sm" onClick={() => onReview("accept")}>
            <ActionIcon name="check" />
            {t("model.accept")}
          </Button>
          <Button
            size="sm"
            variant={item.tracked ? "secondary" : "outline"}
            aria-pressed={item.tracked}
            onClick={() => onReview(item.tracked ? "untrack" : "track")}
          >
            <ActionIcon name={item.tracked ? "untrack" : "track"} />
            {item.tracked ? t("model.tracking") : t("model.keepTracking")}
          </Button>
          <span className="flagged-meta">
            {item.issues.length ? (
              <>
                {t("context.linkedPrefix")}{" "}
                <LinkedIssues ids={item.issues} issues={issues} />
              </>
            ) : (
              t("model.noLinkedIssues")
            )}
          </span>
        </div>
      </motion.article>
    </ContextTarget>
  );
}

/** The newest build in one line: a progress bar while it runs, else how it ended. */
function RunLine({ job, now }: { job: JobView; now: number }) {
  useTranslation();
  const run = job.builds[0];
  if (!run) return <span className="quiet">{t("model.noRetainedBuilds")}</span>;
  const running = job.builds.filter((r) => r.building);
  if (running.length)
    return (
      <>
        {running.map((r) => (
          <RunningEstimate
            key={r.number}
            job={{
              ...job,
              builds: [r, ...job.builds.filter((b) => b.number !== r.number)],
            }}
            now={now}
          />
        ))}
      </>
    );
  return (
    <span className={`run-line run-${runState(run)}`}>
      <ExternalLink href={run.url} actions={buildContextActions(run)}>
        {t("model.duration", {
          v0: run.number,
          v1: resultNames[run.result ?? ""] ?? run.result ?? "",
          v2: day(run.timestamp),
          v3: span(run.duration),
        })}
      </ExternalLink>
    </span>
  );
}

/** Ten builds as bars: height is duration, colour is the result; the running one is striped. */
function History({ builds, now }: { builds: BuildRun[]; now: number }) {
  useTranslation();
  const runs = [...builds].reverse();
  const longest = Math.max(
    1,
    ...runs.map((r) =>
      r.building ? Math.max(now - r.timestamp, r.estimated) : r.duration,
    ),
  );
  return (
    <div
      className="build-history"
      role="group"
      aria-label={t("links.buildHistory")}
    >
      {runs.map((r) => {
        const d = r.building ? now - r.timestamp : r.duration;
        return (
          <ExternalLink
            href={r.url}
            actions={buildContextActions(r)}
            key={r.number}
            className={`bar run-${runState(r)}`}
            style={{ height: `${Math.max(8, (d / longest) * 100)}%` }}
            title={`#${r.number} ${resultNames[r.result ?? ""] ?? t("model.building")} · ${span(d)}`}
          />
        );
      })}
    </div>
  );
}

/** The parameters the job builds with now; ones the newest build changed show before → after. */
function Config({ job }: { job: JobView }) {
  useTranslation();
  const [all, setAll] = useState(false);
  const newest = job.builds.find((b) =>
    job.changes.some((c) => c.build === b.number),
  );
  const latest = job.changes.filter((c) => c.build === job.changes[0]?.build);
  const changed = new Set(latest.map((c) => c.name));
  const names = Object.keys(job.parameters);
  if (!names.length)
    return (
      <p className="quiet config-empty">
        {t("model.thisJobHasNoBuildParameters")}
      </p>
    );
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
        <span>{t("model.currentConfiguration")}</span>
        <span className="quiet">
          {job.changes.length && newest
            ? t("model.changedParameters", {
                v0: newest.number,
                count: changed.size,
              })
            : t("model.noChangesInTheLastBuilds", { v0: job.builds.length })}
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
          <ActionIcon name={all ? "collapse" : "expand"} />
          {all
            ? t("model.collapse")
            : t("model.allParameters", { v0: ordered.length })}
        </button>
      ) : null}
      {older.length ? (
        <ul className="config-history">
          {older.map((c) => (
            <li key={`${c.build}-${c.name}`}>
              <ExternalLink
                className="quiet"
                href={job.builds.find((run) => run.number === c.build)?.url}
              >
                #{c.build}
              </ExternalLink>{" "}
              {c.name} <s>{show(c.before)}</s> → {show(c.after)}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function JobRow({
  job,
  now,
  phaseConfirmed = false,
  source,
}: {
  job: JobView;
  now: number;
  phaseConfirmed?: boolean;
  source?: JobSource;
}) {
  useTranslation();
  return (
    <ContextTarget
      title={job.name}
      actions={[
        openContextAction("job", t("context.openJob"), jobBrowserUrl(job)),
        openContextAction("build", t("context.openBuild"), job.builds[0]?.url),
        openContextAction(
          "console",
          t("links.console"),
          consoleBrowserUrl(job.builds[0]),
        ),
        copyContextAction("copy-job", t("context.copyJobName"), job.job, true),
      ].filter((action) => action !== null)}
    >
      <article
        className={`job job-${runState(job.builds[0])}`}
        aria-label={job.name}
      >
        <div className="job-main">
          <div className="job-title">
            <strong>
              <ExternalLink href={jobBrowserUrl(job)} contextMenu={false}>
                {job.name}
              </ExternalLink>
            </strong>
            <span className={`phase phase-${job.phase}`}>
              {job.phase === "main" && !phaseConfirmed
                ? t("association.phaseUnspecified")
                : phaseNames[job.phase]}
            </span>
            {job.disabled ? (
              <span className="quiet">{t("model.disabled")}</span>
            ) : null}
          </div>
          {source ? (
            <p className="job-source-label">
              {t("source.currentLocal")} · <code>{source.branch}</code>
            </p>
          ) : null}
          {job.error ? (
            <span className="short">{job.error}</span>
          ) : (
            <>
              <RunLine job={job} now={now} />
              <DurationSummary job={job} />
              <div className="job-meta">
                <History builds={job.builds} now={now} />
                <span className="quiet">
                  {job.builds[0]?.cause ?? ""}
                  {job.builds[0]?.node ? ` · ${job.builds[0].node}` : ""}
                </span>
              </div>
            </>
          )}
          {consoleBrowserUrl(job.builds[0]) ? (
            <ExternalLink
              href={consoleBrowserUrl(job.builds[0])}
              className="build-console-link"
            >
              {t("links.console")}
            </ExternalLink>
          ) : null}
        </div>
        {job.error ? null : <Config job={job} />}
      </article>
    </ContextTarget>
  );
}

function BuildsTab({
  builds,
  saved,
}: {
  builds: BuildsState;
  saved?: FocusModel;
}) {
  useTranslation();
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
          {data ? t("focus.loaded", { v0: since(data.computed_at) }) : ""}
        </span>
        <Button
          variant="outline"
          size="sm"
          disabled={builds.loading}
          onClick={() => void builds.refresh()}
        >
          {builds.loading ? <Spinner aria-hidden="true" /> : <RefreshCw />}
          {t("model.refresh")}
        </Button>
      </div>
      {builds.failure ? <FailureNote failure={builds.failure} /> : null}
      {data?.errors.length ? (
        <FailureNote
          failure={{ code: "source_failed", message: data.errors.join("；") }}
        />
      ) : null}
      {data ? <BuildActivity data={data} now={now} /> : null}
      {!data ? (
        <div className="job-skeletons" aria-hidden="true">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="job-skeleton" />
          ))}
        </div>
      ) : (
        releaseJobGroups(active, saved).map((g) => (
          <section
            key={g.id}
            className="job-group"
            aria-label={
              g.label ??
              (g.phase === "main"
                ? t("association.phaseUnspecified")
                : phaseNames[g.phase])
            }
          >
            <h2>
              {g.label ??
                (g.phase === "main"
                  ? t("association.phaseUnspecified")
                  : phaseNames[g.phase])}
              {g.label ? (
                <span className="release-meta">
                  {phaseNames[g.phase]} · {lifecycleName(g.lifecycle)}
                </span>
              ) : null}
            </h2>
            {g.jobs.map((j) => (
              <JobRow
                key={j.job}
                job={j}
                now={now}
                phaseConfirmed={!!g.label}
                source={saved?.job_sources?.[j.job]}
              />
            ))}
          </section>
        ))
      )}
      {rest.length ? (
        <section className="job-group retired">
          <button
            type="button"
            className="link"
            onClick={() => setRetired(!retired)}
          >
            {retired
              ? t("model.collapse")
              : t("model.oldAndDisabledJobs", { v0: rest.length })}
          </button>
          {retired
            ? rest.map((j) => <JobRow key={j.job} job={j} now={now} />)
            : null}
        </section>
      ) : null}
    </div>
  );
}

/** Established mappings only; unknown-phase candidates are offered separately. */
export function jobsFor(branch: string, phase: string, jobs: JobView[] = []) {
  return buildAssociation(branch, phase, jobs).jobs;
}

const stateNames: Record<string, string> = {
  get flagged() {
    return t("model.needsAttention");
  },
  get earlier() {
    return t("model.matchedOverTwoWeeksAgo");
  },
  get tracked() {
    return t("model.tracking");
  },
  get accepted() {
    return t("model.accepted");
  },
  get logged() {
    return t("focus.logOnly");
  },
  get ignored() {
    return t("model.ignored");
  },
  get clean() {
    return t("model.noRulesMatched");
  },
};
const filters = () =>
  [
    ["attention", t("model.needsAttention")],
    ["all", t("model.all")],
    ["logged", t("focus.logOnly")],
    ["ignored", t("model.ignored")],
    ["abandoned", t("model.abandoned")],
  ] as const;
export const inFilter = (l: Listed, f: string) =>
  f === "all" ||
  (f === "abandoned"
    ? l.status.toUpperCase() === "ABANDONED"
    : l.status.toUpperCase() !== "ABANDONED" &&
      (f === "attention"
        ? ["flagged", "tracked"].includes(l.state)
        : l.state === f));

function BranchesTab({
  m,
  saved,
  builds,
  selected,
  onSelect,
  onAssociate,
  onSetJobs,
}: {
  m: ModelSummary;
  saved?: FocusModel;
  builds: BuildsState;
  selected: string | null;
  onSelect: (branch: string) => void;
  onAssociate?: (branch: string, job: string | null) => Promise<unknown>;
  onSetJobs?: (branch: string, jobs: string[]) => Promise<unknown>;
}) {
  useTranslation();
  const groups = branchGroups(m, saved);
  const branch =
    selected &&
    groups.some((group) =>
      group.branches.some((item) => item.branch === selected),
    )
      ? selected
      : (groups[0]?.branches[0]?.branch ?? null);
  const [lineId, setLineId] = useModelPreference(m.code, "release-line", "");
  const selectedGroup =
    groups.find(
      (g) => g.id === lineId && g.branches.some((b) => b.branch === branch),
    ) ?? groups.find((g) => g.branches.some((b) => b.branch === branch));
  const strip = selectedGroup?.branches.find((b) => b.branch === branch);
  const changes = useQuery(
    { ...changesOptions(m.code, branch), enabled: !!branch },
    queryClient,
  );
  const list = changes.data ?? null;
  const failure = changes.error ? failureOf(changes.error) : null;
  // The newest changes are the likeliest to be opened: their files and message arrive early.
  useEffect(() => {
    if (!list?.length) return;
    const timer = setTimeout(
      () => list.slice(0, 3).forEach((l) => prefetchChange(m.code, l.number)),
      800,
    );
    return () => clearTimeout(timer);
  }, [list, m.code]);
  const [filter, setFilter] = useModelPreference(m.code, "filter", "attention");
  useEffect(() => {
    if (
      list &&
      filter === "attention" &&
      !list.some((l) => inFilter(l, "attention"))
    )
      setFilter("all");
  }, [list]);
  const association = buildAssociation(
    branch ?? "",
    strip?.phase ?? "main",
    builds.data?.jobs,
    saved,
    selectedGroup?.jobs,
  );
  const jobs = association.jobs;
  const shown = list?.filter((l) => inFilter(l, filter)) ?? [];
  const [timelineJob, setTimelineJob] = useModelPreference(
    m.code,
    "timeline-job",
    "suggested",
  );
  const allJobs = builds.data?.jobs ?? [];
  const selectedJobs =
    timelineJob === "off"
      ? []
      : timelineJob === "suggested"
        ? jobs
        : allJobs.filter((job) => job.job === timelineJob);
  const routes = sourceRoutes(saved, branch ?? "");
  const historicalJobs = (builds.data?.jobs ?? []).filter(
    (job) =>
      routes.historical.some((r) => r.job === job.job) &&
      (saved?.branch_jobs?.[branch] === undefined ||
        saved.branch_jobs[branch].includes(job.job)),
  );
  const timelineJobs =
    timelineJob === "suggested"
      ? [
          ...new Map(
            [...selectedJobs, ...historicalJobs].map((job) => [job.job, job]),
          ).values(),
        ]
      : selectedJobs;
  const events = branchTimeline(
    shown,
    sourceTimelineJobs(timelineJobs, branch ?? "", saved),
  );

  const timelineOptions = [
    {
      value: "suggested",
      label:
        association.kind === "source" || association.kind === "historical"
          ? t("source.savedRouting")
          : association.kind === "configured"
            ? selectedGroup?.jobs !== undefined
              ? t("release.linkedJobs")
              : t("association.useSaved")
            : t("timeline.suggested"),
    },
    { value: "off", label: t("timeline.hideBuilds") },
    ...allJobs.map((job) => ({
      value: job.job,
      label: `${job.name}${job.disabled ? ` · ${t("timeline.disabled")}` : ""}`,
    })),
  ];
  return (
    <div className="branches-tab">
      <nav className="branch-rail" aria-label={t("model.branches")}>
        {groups.map((g) => (
          <section key={g.id}>
            <h2>
              {g.label ?? phaseNames[g.phase]}
              {g.label ? (
                <span className="release-meta">
                  {phaseNames[g.phase]} · {lifecycleName(g.lifecycle)}
                </span>
              ) : null}
            </h2>
            {g.branches.map((b) => (
              <ContextTarget
                key={b.branch}
                title={b.branch}
                actions={[
                  {
                    id: "view",
                    label: t("context.openBranch"),
                    run: () => {
                      setLineId(g.id);
                      onSelect(b.branch);
                    },
                  },
                  copyContextAction(
                    "copy-branch",
                    t("context.copyBranch"),
                    b.branch,
                  ),
                ]}
              >
                <button
                  type="button"
                  className="branch-pick"
                  aria-current={
                    b.branch === branch && selectedGroup?.id === g.id
                      ? "true"
                      : undefined
                  }
                  onClick={() => {
                    setLineId(g.id);
                    onSelect(b.branch);
                  }}
                >
                  <span className="branch-pick-name">
                    <span>
                      {b.branch}
                      {sourceRoutes(saved, b.branch).current.length ? (
                        <small className="source-badge">
                          {t("source.current")}
                        </small>
                      ) : sourceRoutes(saved, b.branch).historical.length ? (
                        <small className="source-badge">
                          {t("source.historical")}
                        </small>
                      ) : null}
                    </span>
                    {b.flagged ? (
                      <span className="short">{b.flagged}</span>
                    ) : null}
                  </span>
                  <Strip id={`${m.code}:${b.branch}`} days={b.days} />
                </button>
              </ContextTarget>
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
            {selectedGroup?.label ? (
              <p className="quiet">
                {selectedGroup.label} · {lifecycleName(selectedGroup.lifecycle)}
              </p>
            ) : null}
            <h2 className="branch-name">{branch}</h2>
            <p className="quiet">
              {t("model.changesOver12WeeksNeedAttention", {
                v0: strip.total,
                v1: strip.flagged,
              })}
            </p>
          </header>
          <SourceRoutingNote
            branch={branch}
            model={saved}
            sampleUrl={list?.find((l) => l.url)?.url}
            onSelect={onSelect}
          />
          {builds.data ? (
            <BranchBuildAssociation
              key={branch}
              association={association}
              availableJobs={allJobs.filter(
                (job) =>
                  !saved?.ignored_jobs?.includes(job.job) &&
                  !routes.historical.some((route) => route.job === job.job),
              )}
              lockedJobs={routes.current.map((route) => route.job)}
              selectedJobs={[
                ...new Set([
                  ...jobs.map((job) => job.job),
                  ...association.missing,
                ]),
              ]}
              onSet={
                onSetJobs ? (names) => onSetJobs(branch, names) : undefined
              }
              loading={builds.loading}
              onConfirm={
                onAssociate ? (job) => onAssociate(branch, job.job) : undefined
              }
              onReset={
                onAssociate && saved?.branch_jobs?.[branch] !== undefined
                  ? () => onAssociate(branch, null)
                  : undefined
              }
            />
          ) : null}
          {jobs.length ? (
            <div className="branch-jobs">
              {jobs.map((j) => (
                <div
                  key={j.job}
                  className={`branch-job job-${runState(j.builds[0])}`}
                >
                  <strong>
                    <ExternalLink href={jobBrowserUrl(j)}>
                      {j.name}
                    </ExternalLink>
                  </strong>
                  <RunLine job={j} now={builds.now} />
                </div>
              ))}
            </div>
          ) : null}
          <ManifestInputs jobs={jobs} />
          <div className="timeline-controls">
            <span className="timeline-label">{t("timeline.builds")}</span>
            <Select
              value={timelineJob}
              items={timelineOptions}
              onValueChange={(value) =>
                value && setTimelineJob(value as string)
              }
            >
              <SelectTrigger size="sm" aria-label={t("timeline.builds")}>
                <SelectValue>
                  {(value) =>
                    timelineOptions.find((option) => option.value === value)
                      ?.label ?? value
                  }
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                {timelineOptions.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <button
              type="button"
              disabled={builds.loading}
              onClick={builds.refresh}
            >
              <RefreshCw size={14} aria-hidden="true" />
              {t("timeline.refreshBuilds")}
            </button>
            <Tooltip>
              <TooltipTrigger
                render={
                  <button
                    type="button"
                    className="info-hint"
                    aria-label={t("timeline.explanation")}
                  />
                }
              >
                <Info size={14} aria-hidden="true" />
              </TooltipTrigger>
              <TooltipContent>{t("timeline.explanation")}</TooltipContent>
            </Tooltip>
          </div>

          {builds.failure ? <FailureNote failure={builds.failure} /> : null}
          <div
            className="segmented"
            role="radiogroup"
            aria-label={t("model.filter")}
          >
            {filters().map(([key, label]) => (
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
              {events.map((event, index) =>
                event.kind === "build" ? (
                  <BuildMarker
                    key={event.key}
                    code={m.code}
                    branch={branch}
                    job={event.job}
                    run={event.run}
                  />
                ) : (
                  (() => {
                    const l = event.change;
                    return (
                      <Fragment key={event.key}>
                        {event.time === null &&
                        (index === 0 || events[index - 1].time !== null) ? (
                          <li className="quiet timeline-undated">
                            {t("timeline.dateOnly")}
                          </li>
                        ) : null}

                        <ContextTarget
                          key={l.number}
                          title={`Gerrit #${l.number}`}
                          actions={commitContextActions(l)}
                        >
                          <li
                            className={`change state-${l.status === "ABANDONED" ? "abandoned" : l.state} gerrit-${l.status.toLowerCase()}`}
                          >
                            <span
                              className="change-when"
                              title={t("timeline.changeCreated")}
                            >
                              {event.time !== null
                                ? timelineTime(event.time)
                                : l.created.slice(5)}
                            </span>
                            <div>
                              <p>
                                <ExternalLink href={l.url} contextMenu={false}>
                                  {l.subject}
                                </ExternalLink>
                              </p>
                              <span className="flagged-meta">
                                <ChangeStatus status={l.status} />
                                {l.status !== "ABANDONED" && l.state !== "clean"
                                  ? ` ${stateNames[l.state] ?? l.state}`
                                  : ""}
                                {l.rules.length
                                  ? ` · ${l.rules.map((r) => ruleNames[r] ?? r).join("、")}`
                                  : ""}{" "}
                                ·{" "}
                                <ExternalLink
                                  href={gerritSearchUrl(
                                    l.url,
                                    gerritTerm("project", l.project),
                                  )}
                                >
                                  {l.project}
                                </ExternalLink>{" "}
                                ·{" "}
                                <ExternalLink
                                  href={gerritSearchUrl(
                                    l.url,
                                    gerritTerm("owner", l.owner),
                                  )}
                                >
                                  <OwnerChip name={l.owner} />
                                </ExternalLink>{" "}
                                · {l.number}
                                {l.issues.length ? (
                                  <>
                                    {" "}
                                    ·{" "}
                                    <LinkedIssues
                                      ids={l.issues}
                                      issues={m.issue_links ?? []}
                                    />
                                  </>
                                ) : null}
                              </span>
                              <ChangeFiles code={m.code} change={l.number} />
                            </div>
                          </li>
                        </ContextTarget>
                      </Fragment>
                    );
                  })()
                ),
              )}
            </ol>
          )}
        </section>
      ) : (
        <p className="quiet">{t("model.noBranchesFollowedForThisModelYet")}</p>
      )}
    </div>
  );
}

function Overview({
  m,
  saved,
  state,
  builds,
  issues,
  requirements,
  onWork,
  onBranch,
  onBuilds,
}: {
  m: ModelSummary;
  saved?: FocusModel;
  state: FocusState;
  builds: BuildsState;
  issues: WorkItems | null;
  requirements: WorkItems | null;
  onWork: (kind: "issues" | "requirements") => void;
  onBranch: (branch: string) => void;
  onBuilds: () => void;
}) {
  useTranslation();
  const jobs =
    builds.data?.jobs.filter((j) => !j.disabled && j.phase !== "old") ?? [];
  return (
    <div className="model-columns">
      <section className="flagged-list" aria-label={t("model.needsAttention")}>
        <div className="column-head">
          <h2>{t("model.needsAttention")}</h2>
          <span className="quiet">
            {t("model.ruleMatchesLastTwoWeeksAndTrackedChanges")}
          </span>
        </div>
        <AnimatePresence initial={false}>
          {m.flagged.map((f) => (
            <FlaggedRow
              key={f.number}
              item={f}
              issues={m.issue_links ?? []}
              onReview={(a) => state.review(m.code, f.number, a)}
            />
          ))}
        </AnimatePresence>
        {m.flagged.length ? null : (
          <p className="quiet empty-line">
            {t("model.noChangesNeedAttentionFromTheLastTwoWeeks")}
          </p>
        )}
      </section>
      <aside className="model-side">
        <WorkSummary
          code={m.code}
          issues={issues}
          requirements={requirements}
          onOpen={onWork}
        />
        <section aria-label={t("model.builds")}>
          <div className="column-head">
            <h2>{t("model.builds")}</h2>
            <button type="button" className="link" onClick={onBuilds}>
              <ActionIcon name="builds" />
              {t("model.all")}
            </button>
          </div>
          {!builds.data ? (
            builds.failure ? (
              <span className="short">
                {builds.failure.message ?? t("model.couldNotLoadJenkins")}
              </span>
            ) : (
              <span className="quiet loading-line">
                <Spinner aria-hidden="true" /> {t("model.loadingJenkins")}
              </span>
            )
          ) : jobs.length ? (
            jobs.map((j) => (
              <div
                key={j.job}
                className={`job-line job-${runState(j.builds[0])}`}
              >
                <button
                  type="button"
                  className="link job-line-name"
                  onClick={onBuilds}
                >
                  <span className={`phase phase-${j.phase}`}>
                    {phaseNames[j.phase]}
                  </span>
                  {j.name}
                </button>
                <RunLine job={j} now={builds.now} />
              </div>
            ))
          ) : (
            <span className="quiet">
              {builds.data.errors[0] ?? t("model.noBuildJobsFound")}
            </span>
          )}
        </section>
        <section aria-label={t("model.branches")}>
          <div className="column-head">
            <h2>{t("model.branches")}</h2>
            <span className="quiet">{t("model.last12Weeks")}</span>
          </div>
          {branchGroups(m, saved).map((g) => (
            <div key={g.id} className="phase-group">
              <span className={`phase phase-${g.phase}`}>
                {g.label ?? phaseNames[g.phase]}
              </span>
              {g.label ? (
                <span className="release-meta">
                  {phaseNames[g.phase]} · {lifecycleName(g.lifecycle)}
                </span>
              ) : null}
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
                  <Strip id={`${m.code}:${b.branch}`} days={b.days} />
                </button>
              ))}
            </div>
          ))}
        </section>
      </aside>
    </div>
  );
}

const actionOptions = () =>
  Object.entries(actionNames).map(([value, label]) => ({
    value,
    label,
  }));
type Draft = Pick<
  FocusModel,
  | "rules"
  | "freeze"
  | "targets"
  | "release_lines"
  | "ignored_jobs"
  | "branch_jobs"
  | "job_sources"
>;
const draftOf = (m: FocusModel): Draft => ({
  rules: { ...defaultRules, ...m.rules },
  freeze: m.freeze ?? "",
  targets: { ...m.targets },
  release_lines: m.release_lines ?? [],
  ignored_jobs: m.ignored_jobs ?? [],
  branch_jobs: m.branch_jobs ?? {},
  job_sources: m.job_sources ?? {},
});
const bounded = (v: string, max: number) =>
  Math.max(0, Math.min(max, Math.round(Number(v) || 0)));
const targetNames = {
  get severe_pct() {
    return t("model.severeIssueResolutionTarget");
  },
  get overall_pct() {
    return t("model.overallResolutionTarget");
  },
  get fatal_max() {
    return t("model.fatalIssueLimit");
  },
} as const;
const modelOf = (saved: FocusModel, draft: Draft): FocusModel => {
  const { freeze, ...rest } = draft;
  return sourceBranches({ ...saved, ...rest, freeze: freeze || undefined });
};

function Delta({ before, after }: { before: number; after: number }) {
  useTranslation();
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
  useTranslation();
  const b = gate(v, n, before);
  const a = gate(v, n, after);
  return (
    <div className="preview-line">
      <span>{label}</span>
      <span className="delta">
        <span className="quiet">
          {b.ok ? t("focus.targetMet") : t("model.short", { v0: b.gap })}
        </span>{" "}
        →{" "}
        <strong className={a.ok ? "" : "short"}>
          {a.ok
            ? t("focus.targetMet")
            : t("focus.moreToResolve", { v0: a.gap })}
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
  useTranslation();
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
            t("model.severeIssues"),
            m.measures.severe_resolved,
            m.measures.severe_total,
            "severe_pct",
          ],
          [
            t("model.overall"),
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
      aria-label={t("model.beforeAndAfterSaving")}
    >
      <div className="column-head">
        <h2>{dirty ? t("model.afterSaving") : t("model.now")}</h2>
        {busy ? <Spinner aria-hidden="true" /> : null}
      </div>
      {failure ? <FailureNote failure={failure} /> : null}
      {preview ? (
        <>
          <div className="preview-big">
            <span>{t("model.needsAttention")}</span>
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
            <span>{t("model.alerts")}</span>
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
                {t("model.fatalLimit", {
                  v0: saved.targets.fatal_max,
                  v1: draft.targets.fatal_max,
                })}
              </span>
              <strong
                className={
                  m.measures.fatal_open > draft.targets.fatal_max ? "short" : ""
                }
              >
                {m.measures.fatal_open > draft.targets.fatal_max
                  ? t("model.overLimit")
                  : t("focus.targetMet")}
              </strong>
            </div>
          ) : null}
          {dirty && (preview.entering.length || preview.leaving.length) ? (
            <div className="preview-moves">
              {preview.entering.length ? (
                <>
                  <span className="kicker">
                    {t("model.newlyHighlighted", {
                      v0: preview.entering.length,
                    })}
                  </span>
                  {preview.entering.slice(0, 5).map((f) => (
                    <p key={f.number} className="move in">
                      <ExternalLink href={f.url}>{f.subject}</ExternalLink>
                    </p>
                  ))}
                </>
              ) : null}
              {preview.leaving.length ? (
                <>
                  <span className="kicker">
                    {t("model.noLongerHighlighted", {
                      v0: preview.leaving.length,
                    })}
                  </span>
                  {preview.leaving.slice(0, 5).map((f) => (
                    <p key={f.number} className="move out">
                      <ExternalLink href={f.url}>{f.subject}</ExternalLink>
                    </p>
                  ))}
                </>
              ) : null}
            </div>
          ) : null}
          <span className="quiet">
            {t("model.previewBasedOnTheLastChangesLoaded", {
              v0: preview.changes,
            })}
          </span>
        </>
      ) : !failure ? (
        <Skeleton className="bar" />
      ) : null}
    </aside>
  );
}

/** Rules, freeze date and targets, edited in place, previewed, then saved together. */
export function SettingsTab({
  m,
  saved,
  state,
  onEdit,
  builds,
}: {
  m: ModelSummary;
  saved: FocusModel;
  builds: BuildsState;
  state: FocusState;
  onEdit: () => void;
}) {
  useTranslation();
  const [draft, setDraft] = useState<Draft>(() => draftOf(saved));
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  useEffect(() => setDraft(draftOf(saved)), [saved]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(draftOf(saved));
  // SPM refuses a source without a branch; say so on the field instead of at save.
  const sourceMissing = Object.values(draft.job_sources ?? {}).some(
    (source) => !source.branch,
  );
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
  return (
    <div className="settings-tab">
      <div className="settings-form">
        <nav className="settings-nav" aria-label={t("model.settings")}>
          {[
            ["settings-rules", t("focus.rules")],
            ["settings-targets", t("model.targets")],
            ["settings-release", t("release.title")],
            ["settings-sources", t("source.routing")],
            ["settings-manage", t("model.manage")],
          ].map(([id, label]) => (
            <button
              key={id}
              type="button"
              onClick={() =>
                document
                  .getElementById(id)
                  ?.scrollIntoView({ behavior: "smooth", block: "start" })
              }
            >
              {label}
            </button>
          ))}
        </nav>
        <section id="settings-rules" aria-label={t("focus.rules")}>
          <div className="column-head">
            <h2>{t("focus.rules")}</h2>
            <span className="quiet">{t("model.matchesOver13Weeks")}</span>
          </div>
          <div className="settings-card">
            {ruleOrder.map((r) => (
              <div className="rule-block" key={r}>
                <div className="rule-line">
                  <span>
                    {ruleNames[r]}
                    <small className="quiet">{ruleHow[r]}</small>
                  </span>
                  <ToggleGroup
                    variant="outline"
                    size="sm"
                    className="action-choice"
                    data-action={draft.rules[r]}
                    value={[draft.rules[r]]}
                    aria-label={t("model.actionFor", { v0: ruleNames[r] })}
                    onValueChange={(next: string[]) => {
                      if (next[0])
                        setDraft({
                          ...draft,
                          rules: { ...draft.rules, [r]: next[0] },
                        });
                    }}
                  >
                    {actionOptions().map((o) => (
                      <ToggleGroupItem key={o.value} value={o.value}>
                        {o.label}
                      </ToggleGroupItem>
                    ))}
                  </ToggleGroup>
                  <span className="quiet">{m.rule_counts[r] ?? 0}</span>
                </div>
                {r === "feature_after_freeze" ? (
                  <label
                    className={
                      draft.freeze ? "freeze-line" : "freeze-line unset"
                    }
                  >
                    <span>
                      {draft.freeze
                        ? t("focus.mpFreezeDate")
                        : t("model.setAnMPFreezeDateToEnableThisRule")}
                    </span>
                    <Input
                      type="date"
                      aria-label={t("focus.mpFreezeDate")}
                      value={draft.freeze}
                      onChange={(e) =>
                        setDraft({ ...draft, freeze: e.target.value })
                      }
                    />
                  </label>
                ) : null}
              </div>
            ))}
          </div>
        </section>
        <section id="settings-targets" aria-label={t("model.targets")}>
          <div className="column-head">
            <h2>{t("model.targets")}</h2>
          </div>
          <div className="settings-card">
            <div className="target-line">
              <span>{t("model.severeIssueResolutionAtLeast")}</span>
              {target("severe_pct", 100)}
              <span className="quiet">%</span>
            </div>
            <div className="target-line">
              <span>{t("model.overallResolutionAtLeast")}</span>
              {target("overall_pct", 100)}
              <span className="quiet">%</span>
            </div>
            <div className="target-line">
              <span>{t("model.unresolvedFatalIssuesAtMost")}</span>
              {target("fatal_max", 9999)}
              <span className="quiet">{t("model.issues")}</span>
            </div>
          </div>
        </section>
        <ReleaseLinesEditor
          model={saved}
          refreshing={builds.loading}
          lines={draft.release_lines ?? []}
          ignored={draft.ignored_jobs ?? []}
          jobs={builds.data?.jobs ?? []}
          onRefresh={() => void builds.refresh()}
          onChange={(release_lines, ignored_jobs) =>
            setDraft({
              ...draft,
              release_lines,
              ignored_jobs,
              job_sources: Object.fromEntries(
                Object.entries(draft.job_sources ?? {}).filter(
                  ([job]) => !ignored_jobs.includes(job),
                ),
              ),
              branch_jobs: Object.fromEntries(
                Object.entries(draft.branch_jobs ?? {}).map(
                  ([branch, jobs]) => [
                    branch,
                    jobs.filter((job) => !ignored_jobs.includes(job)),
                  ],
                ),
              ),
            })
          }
        />
        <JobSourcesEditor
          jobs={builds.data?.jobs ?? []}
          sources={draft.job_sources ?? {}}
          onChange={(job_sources) => setDraft({ ...draft, job_sources })}
          ignored={draft.ignored_jobs ?? []}
          onRestore={(job) =>
            setDraft({
              ...draft,
              ignored_jobs: (draft.ignored_jobs ?? []).filter((j) => j !== job),
            })
          }
        />
        {Object.keys(draft.branch_jobs ?? {}).length ? (
          <section
            className="branch-binding-settings"
            aria-label={t("association.settings")}
          >
            <h2>{t("association.settings")}</h2>
            <div className="settings-card">
              {Object.entries(draft.branch_jobs ?? {}).map(([branch, jobs]) => (
                <div key={branch} className="binding-row">
                  <p>
                    <code>{branch}</code> ·{" "}
                    {jobs.map((j) => j.split("/").pop()).join(", ") || "—"}
                  </p>
                  <button
                    type="button"
                    className="link"
                    onClick={() => {
                      const branch_jobs = { ...draft.branch_jobs };
                      delete branch_jobs[branch];
                      setDraft({ ...draft, branch_jobs });
                    }}
                  >
                    {t("association.reset")}
                  </button>
                </div>
              ))}
            </div>
          </section>
        ) : null}
        {failure ? <FailureNote failure={failure} /> : null}
        {dirty || saving ? (
          <div className="settings-bar" role="status">
            <span>
              {saving
                ? t("model.savingAndRefreshingWithNewSettings")
                : t("model.unsavedChanges")}
            </span>
            <div>
              <Button
                variant="ghost"
                size="sm"
                className="bar-discard"
                disabled={saving}
                onClick={() => setDraft(draftOf(saved))}
              >
                <ActionIcon name="discard" />
                {t("model.discard")}
              </Button>
              <Button
                size="sm"
                className="bar-save"
                disabled={saving || sourceMissing}
                title={sourceMissing ? t("source.branchRequired") : undefined}
                onClick={() => void save()}
              >
                {saving ? (
                  <Spinner aria-hidden="true" />
                ) : (
                  <ActionIcon name="save" />
                )}
                {t("focus.save")}
              </Button>
            </div>
          </div>
        ) : null}
        <section id="settings-manage" aria-label={t("model.manage")}>
          <div className="column-head">
            <h2>{t("model.manage")}</h2>
          </div>
          <div className="settings-card model-manage">
            <Button variant="ghost" size="sm" onClick={onEdit}>
              <ActionIcon name="sources" />
              {t("model.changeIssueSourcesAndBranches")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="manage-danger"
              onClick={() => state.requestRemove(saved.code)}
            >
              <ActionIcon name="remove" />
              {t("model.unfollow")}
            </Button>
          </div>
        </section>
      </div>
      <PreviewPanel m={m} saved={saved} draft={draft} dirty={dirty} />
    </div>
  );
}

/** 机型: the head with its gates, then 概览, 分支, 构建 and 设置. */
/** What the overview leads with depends on where the model is in its life. */
function PhaseLead({
  code,
  openRequirements,
  onTab,
}: {
  code: string;
  openRequirements: number;
  onTab: (tab: ModelTab) => void;
}) {
  const profiles = useQuery(profilesOptions, queryClient).data;
  const phase = profiles?.find((p) => p.code === code)?.phase;
  if (phase === "ota")
    return <OtaLead code={code} onOpen={() => onTab("ota")} />;
  if (phase === "development" && openRequirements)
    return (
      <button
        type="button"
        className="ota-lead"
        onClick={() => onTab("requirements")}
      >
        {t("profile.openRequirements", { count: openRequirements })}
      </button>
    );
  return null;
}

export function ModelPage({
  state,
  code,
  tab = "overview",
  onTab = () => {},
  onEdit,
}: {
  state: FocusState;
  code: string;
  tab?: ModelTab;
  onTab?: (tab: ModelTab) => void;
  onEdit: () => void;
}) {
  useTranslation();
  const m = state.summary?.models.find((x) => x.code === code);
  const saved = state.models?.find((x) => x.code === code);
  const forecasts = useForecasts(state.summary?.computed_at);
  const sourceBuilds = useBuilds(
    code,
    tab !== "settings" && tab !== "checklist",
    tab === "builds",
  );
  const builds = {
    ...sourceBuilds,
    data: sourceBuilds.data
      ? {
          ...sourceBuilds.data,
          jobs: releaseJobs(sourceBuilds.data.jobs, saved),
        }
      : null,
  };
  const issueWork = useWorkItems(
    code,
    "issues",
    tab === "issues" || tab === "overview",
  );
  const requirementWork = useWorkItems(
    code,
    "requirements",
    tab === "requirements" || tab === "overview",
  );
  const [branch, setBranch] = useModelPreference<string | null>(
    code,
    "branch",
    null,
  );
  const running = useMemo(
    () =>
      builds.data?.jobs.flatMap((j) => j.builds.filter((r) => r.building))
        .length ?? 0,
    [builds.data],
  );
  const [issueFilter, setIssueFilter] = useModelPreference<IssueFilter | null>(
    code,
    "issues-filter",
    null,
  );
  if (!m)
    return (
      <div className="model-page">
        <h1 className="model-code">{saved?.name ?? code}</h1>
        <ModelPlatform code={code} />
        {state.models && !saved ? (
          <p className="quiet">
            {t("model.youAreNotFollowing", { v0: code })}
            <button type="button" className="link" onClick={onEdit}>
              {t("focus.follow", { v0: code })}
            </button>
          </p>
        ) : (
          <p className="quiet loading-line">
            <Spinner aria-hidden="true" /> {t("model.loading")}
          </p>
        )}
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
    <div className={saved?.legacy ? "model-page is-legacy" : "model-page"}>
      {saved?.legacy ? (
        <div className="legacy-note">
          <Archive aria-hidden="true" />
          <span>{t("legacy.note")}</span>
          <Button
            variant="ghost"
            size="xs"
            onClick={() => void state.setLegacy(code, false)}
          >
            {t("legacy.restore")}
          </Button>
        </div>
      ) : null}
      <IssueBrowser
        issues={m.issue_links ?? []}
        open={issueFilter}
        onClose={() => setIssueFilter(null)}
      />
      <section
        className={tab === "overview" ? "model-head" : "model-head compact"}
      >
        <div>
          <ContextTarget
            title={m.code}
            actions={
              m.issue_links?.length
                ? (["all", "unresolved", "severe", "fatal"] as const).map(
                    (filter) => ({
                      id: filter,
                      label: t(`context.issues.${filter}`),
                      run: () => setIssueFilter(filter),
                    }),
                  )
                : []
            }
          >
            <h1 className="model-code">{saved?.name ?? m.code}</h1>
          </ContextTarget>
          <ModelPlatform code={code} />
          <p className="quiet">
            {t("model.gerritBranches", {
              v0: sourceText(m.sources),
              v1: m.branches.length,
              v2: saved?.freeze
                ? t("model.mpFrozenOn", { v0: saved.freeze })
                : t("model.noMPFreezeDateSet"),
            })}
          </p>
        </div>
        <div className="model-figures">
          {sev ? (
            <ContextTarget
              actions={[
                {
                  id: "severe",
                  label: t("context.issues.severe"),
                  disabled: !m.issue_links?.length,
                  run: () => setIssueFilter("severe"),
                },
              ]}
            >
              <button
                type="button"
                className="model-figure"
                disabled={!m.issue_links?.length}
                onClick={() => setIssueFilter("severe")}
              >
                <span>{t("focus.severeIssueResolution")}</span>
                <strong className={sev.ok ? "" : "short"}>
                  <Num value={sev.pct} suffix="%" />
                </strong>
                <small className={sev.ok ? "" : "short"}>
                  {sev.v} / {sev.n} ·{" "}
                  {sev.ok
                    ? t("focus.targetMet")
                    : t("focus.moreToResolve", { v0: sev.gap })}
                </small>
              </button>
            </ContextTarget>
          ) : null}
          {all ? (
            <ContextTarget
              actions={[
                {
                  id: "unresolved",
                  label: t("context.issues.unresolved"),
                  disabled: !m.issue_links?.length,
                  run: () => setIssueFilter("unresolved"),
                },
              ]}
            >
              <button
                type="button"
                className="model-figure"
                disabled={!m.issue_links?.length}
                onClick={() => setIssueFilter("unresolved")}
              >
                <span>{t("focus.overallResolution")}</span>
                <strong className={all.ok ? "" : "short"}>
                  <Num value={all.pct} suffix="%" />
                </strong>
                <small className={all.ok ? "" : "short"}>
                  {all.v} / {all.n} ·{" "}
                  {all.ok
                    ? t("focus.targetMet")
                    : t("focus.moreToResolve", { v0: all.gap })}
                </small>
              </button>
            </ContextTarget>
          ) : null}
          <div>
            <span>{t("focus.thisWeek")}</span>
            <strong>
              <Num value={m.new_issues} prefix="+" />
            </strong>
            <small>
              {t("model.newIssuesChangesMerged", {
                v0: m.week_changes,
                v1: m.week_merged,
              })}
            </small>
          </div>
        </div>
      </section>
      <Tabs.Root
        className="model-tabs-root"
        value={tab}
        onValueChange={(value) => onTab(value as ModelTab)}
      >
        <Tabs.List
          className="model-subnav"
          aria-label={t("model.pages", { v0: m.code })}
        >
          {(Object.keys(tabNames) as ModelTab[]).map((tabKey) => (
            <Tabs.Tab key={tabKey} value={tabKey}>
              <ActionIcon name={tabKey} />
              {tabNames[tabKey]}
              {tabKey === "overview" && m.flagged.length ? (
                <span className="tab-count short">{m.flagged.length}</span>
              ) : null}
              {tabKey === "issues" && issueWork.openCount ? (
                <span className="tab-count">{issueWork.openCount}</span>
              ) : null}
              {tabKey === "requirements" && requirementWork.openCount ? (
                <span className="tab-count">{requirementWork.openCount}</span>
              ) : null}
              {tabKey === "builds" && running ? (
                <span className="tab-count building">
                  {t("model.building2", { count: running })}
                </span>
              ) : null}
              {tabKey === "settings" && !saved?.freeze && !saved?.legacy ? (
                <span
                  className="tab-dot"
                  aria-label={t("model.noFreezeDateSet")}
                />
              ) : null}
            </Tabs.Tab>
          ))}
          <Tabs.Indicator className="tab-indicator" />
        </Tabs.List>
        <Tabs.Panel value={tab} className="model-panel">
          {m.errors.length ? (
            <FailureNote
              failure={{ code: "source_failed", message: m.errors.join("；") }}
            />
          ) : null}
          {tab === "overview" ? (
            <PhaseLead
              code={code}
              openRequirements={requirementWork.openCount ?? 0}
              onTab={onTab}
            />
          ) : null}
          {tab === "overview" ? (
            <ModelForecast
              forecast={
                saved?.legacy
                  ? undefined
                  : forecasts?.find((f) => f.code === code)
              }
              quiet={!m.days.slice(-28).some(Boolean)}
              onSettings={() => onTab("settings")}
              onLegacy={() => void state.setLegacy(code, true)}
            />
          ) : null}
          {tab === "overview" ? (
            <Overview
              m={m}
              saved={saved}
              state={state}
              builds={builds}
              issues={issueWork.data}
              requirements={requirementWork.data}
              onWork={onTab}
              onBranch={(b) => {
                setBranch(b);
                onTab("branches");
              }}
              onBuilds={() => onTab("builds")}
            />
          ) : tab === "ota" ? (
            <ModelOtaTab code={code} />
          ) : tab === "issues" || tab === "requirements" ? (
            <WorkItemsTab code={code} kind={tab} saved={saved} state={state} />
          ) : tab === "branches" ? (
            <BranchesTab
              m={m}
              saved={saved}
              builds={builds}
              selected={branch}
              onSelect={setBranch}
              onSetJobs={
                saved
                  ? (branch, jobs) =>
                      state.save(withBranchJobs(saved, branch, jobs), false)
                  : undefined
              }
              onAssociate={
                saved
                  ? (branch, job) =>
                      state.save(withBranchJob(saved, branch, job), false)
                  : undefined
              }
            />
          ) : tab === "builds" ? (
            <BuildsTab builds={builds} saved={saved} />
          ) : tab === "checklist" ? (
            <ReleaseChecklist
              key={code}
              code={code}
              branches={saved?.branches}
            />
          ) : saved ? (
            <SettingsTab
              builds={sourceBuilds}
              m={m}
              saved={saved}
              state={state}
              onEdit={onEdit}
            />
          ) : null}
        </Tabs.Panel>
      </Tabs.Root>
    </div>
  );
}
