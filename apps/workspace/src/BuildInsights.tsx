import { useTranslation } from "react-i18next";
import { Clock3, Hourglass, CircleAlert, Timer, Activity } from "lucide-react";
import { Progress } from "@/components/ui/progress";
import { ExternalLink } from "@/components/external-link";
import type { Builds, JobView } from "./contracts/setup";
import { forecastQueue } from "./queue-estimate";
import { durationHistory, runtimeEstimate } from "./build-estimate";
import { t } from "./i18n";
import { shortTime } from "./time-format";
import { jobBrowserUrl } from "./browser-links";
import "./build-insights.css";

export function buildDuration(ms: number) {
  const minutes = Math.round(Math.max(0, ms) / 60000);
  if (minutes < 1) return t("buildInsight.lessMinute");
  const hours = Math.floor(minutes / 60);
  return hours
    ? t("buildInsight.hoursMinutes", { hours, minutes: minutes % 60 })
    : t("buildInsight.minutes", { count: minutes });
}
function range(low: number, high: number) {
  return Math.round(low / 60000) === Math.round(high / 60000)
    ? buildDuration(high)
    : `${buildDuration(low)} – ${buildDuration(high)}`;
}
const time = (ms: number) => shortTime(ms);
export function RunningEstimate({ job, now }: { job: JobView; now: number }) {
  useTranslation();
  const run = job.builds[0];
  const e = runtimeEstimate(job, now);
  if (!run?.building) return null;
  return (
    <div className={`build-runtime ${e.overdue ? "overdue" : ""}`}>
      <div className="build-runtime-line">
        <ExternalLink href={run.url} className="build-run-link">
          <Activity size={14} />#{run.number} · {t("buildInsight.running")}
        </ExternalLink>
        <span>
          {e.elapsed === null
            ? t("buildInsight.startUnknown")
            : t("buildInsight.elapsed", { duration: buildDuration(e.elapsed) })}
        </span>
      </div>
      <Progress
        value={e.progress}
        aria-label={t("buildInsight.timeProgress")}
      />
      <div className="build-runtime-line build-runtime-eta">
        <strong>
          {e.overdue
            ? t("buildInsight.overdue", {
                duration: buildDuration(e.elapsed! - e.high!),
              })
            : e.remainingHigh !== null
              ? t("buildInsight.remaining", {
                  duration: range(e.remainingLow!, e.remainingHigh),
                })
              : t("buildInsight.finishUnknown")}
        </strong>
        {!e.overdue &&
        e.elapsed !== null &&
        e.finishLow !== null &&
        e.finishHigh !== null ? (
          <span>
            {t("buildInsight.finishAt", {
              time:
                e.finishLow === e.finishHigh
                  ? time(e.finishHigh)
                  : `${time(e.finishLow)} – ${time(e.finishHigh)}`,
            })}
          </span>
        ) : null}
      </div>
      <p className="build-estimate-basis">
        {e.source === "history"
          ? t("buildInsight.historyBasis", { count: e.history.count })
          : t(
              e.source === "jenkins"
                ? "buildInsight.jenkinsBasis"
                : "buildInsight.noEstimate",
            )}
        {e.source === "history" && e.history.sameNode
          ? ` · ${t("buildInsight.sameNode")}`
          : ""}
        {e.source === "history" && e.history.matched
          ? ` · ${t("buildInsight.sameBranch")}`
          : ""}
      </p>
    </div>
  );
}
export function DurationSummary({ job }: { job: JobView }) {
  useTranslation();
  const h = durationHistory(job);
  return (
    <span className="build-duration-summary">
      <Timer size={13} />
      {h.low !== null && h.high !== null
        ? t("buildInsight.typical", {
            duration: range(h.low, h.high),
            count: h.count,
          })
        : t("buildInsight.fewSamples", { count: h.count })}
    </span>
  );
}
function queueReason(reason: string | null, blocked: boolean) {
  const label = reason?.match(
    /Waiting for next available executor on [‘'“\"](.+?)[’'”\"]/i,
  )?.[1];
  return label
    ? t("buildInsight.waitExecutor", { label })
    : reason ||
        t(
          blocked
            ? "buildInsight.blockedReason"
            : "buildInsight.waitingScheduler",
        );
}
export function BuildActivity({ data, now }: { data: Builds; now: number }) {
  useTranslation();
  const running = data.jobs.flatMap((job) =>
    job.builds.filter((run) => run.building),
  );
  const visibleJobs = new Set(data.jobs.map((job) => job.job));
  const queue =
    data.queue?.items.filter((item) => visibleJobs.has(item.job)) ?? [];
  const capacity = data.queue?.capacity;
  const forecasts = capacity ? forecastQueue(capacity, now) : null;
  const observed = Date.parse(data.queue?.observed_at ?? data.computed_at);
  const stale = !Number.isFinite(observed) || now - observed > 120000;
  return (
    <section className="build-activity" aria-label={t("buildInsight.activity")}>
      <header className="build-activity-summary">
        <div>
          <span>
            <Activity size={15} />
            {t("buildInsight.runningCount", { count: running.length })}
          </span>
          <span>
            <Hourglass size={15} />
            {data.queue
              ? t("buildInsight.queuedCount", { count: queue.length })
              : t("buildInsight.queueUnknown")}
          </span>
        </div>
        <span className={stale ? "build-stale" : "build-estimate-basis"}>
          {stale ? t("buildInsight.stale") : t("buildInsight.autoRefresh")}
        </span>
      </header>
      {!data.queue ? (
        <p className="build-queue-gap">
          <CircleAlert size={15} />
          {t(
            data.queue_error
              ? "buildInsight.queueFailed"
              : "buildInsight.queueUnsupported",
          )}
        </p>
      ) : queue.length ? (
        <div className="build-queue">
          <div className="build-queue-heading">
            <h3>{t("buildInsight.queueTitle")}</h3>
            <span>
              {t("buildInsight.serverQueue", {
                count: data.queue.server_count,
              })}{" "}
              · {t("buildInsight.waitOrder")}
            </span>
          </div>
          <p className="build-queue-method">
            {capacity
              ? t("queueEstimate.method")
              : t(
                  data.queue?.capacity_error
                    ? "queueEstimate.capacityFailed"
                    : "queueEstimate.capacityMissing",
                )}
          </p>
          {queue.map((item) => {
            const job = data.jobs.find((job) => job.job === item.job)!;
            const count = queue.filter((q) => q.job === item.job).length;
            const h = durationHistory(job);
            const prediction = forecasts?.get(item.id);
            const predicted =
              prediction?.reason === "estimated" &&
              prediction.startLow !== null &&
              prediction.startHigh !== null &&
              prediction.startHigh > now;
            const forecastTask = capacity?.tasks.find(
              (task) => task.id === item.id,
            );
            return (
              <article key={item.id} className="build-queue-row">
                <Hourglass size={16} aria-hidden="true" />
                <div className="build-queue-copy">
                  <div>
                    <ExternalLink href={jobBrowserUrl(job)}>
                      {job.name}
                    </ExternalLink>
                    <span className="build-queue-id">Q{item.id}</span>
                    {count > 1 ? (
                      <span
                        className="build-queue-repeat"
                        title={t("buildInsight.checkParameters")}
                      >
                        {t("buildInsight.sameTask", { count })}
                      </span>
                    ) : null}
                  </div>
                  <p>{queueReason(item.reason, item.blocked)}</p>
                  {prediction ? (
                    <small>
                      {t("queueEstimate.resources", {
                        online: prediction.online,
                        idle: prediction.idle,
                        offline: prediction.offline,
                        ahead: prediction.ahead,
                      })}
                    </small>
                  ) : null}
                  {forecastTask?.label ? (
                    <small>
                      {t("queueEstimate.pool", { label: forecastTask.label })}
                    </small>
                  ) : null}
                  {h.low !== null && h.high !== null ? (
                    <small>
                      {t("buildInsight.afterStart", {
                        duration: range(h.low, h.high),
                      })}
                    </small>
                  ) : null}
                </div>
                <div className="build-queue-wait">
                  <strong>
                    <Clock3 size={13} />
                    {item.since && item.since <= now
                      ? t("buildInsight.waited", {
                          duration: buildDuration(now - item.since),
                        })
                      : t("buildInsight.waitUnknown")}
                  </strong>
                  {predicted ? (
                    <>
                      <span className="build-queue-prediction">
                        {t("queueEstimate.remaining", {
                          duration: range(
                            Math.max(0, prediction!.startLow! - now),
                            Math.max(0, prediction!.startHigh! - now),
                          ),
                        })}
                      </span>
                      <span>
                        {t("queueEstimate.start", {
                          time: `${time(Math.max(now, prediction!.startLow!))} – ${time(prediction!.startHigh!)}`,
                        })}
                      </span>
                    </>
                  ) : (
                    <span>
                      {t(
                        prediction?.reason === "estimated"
                          ? "queueEstimate.readyPending"
                          : prediction
                            ? `queueEstimate.reason.${prediction.reason}`
                            : "buildInsight.startPending",
                      )}
                    </span>
                  )}
                </div>
              </article>
            );
          })}
        </div>
      ) : (
        <p className="build-queue-clear">{t("buildInsight.queueEmpty")}</p>
      )}
    </section>
  );
}
