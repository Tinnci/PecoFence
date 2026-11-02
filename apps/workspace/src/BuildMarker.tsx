import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { GitBranch, Package, Search, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { t, locale } from "./i18n";
import { focus } from "./api";
import { focusKey, queryClient } from "./focus-cache";
import { branchInputMatches } from "./branch-timeline";
import { buildContextActions } from "./context-actions";
import { ContextTarget } from "./components/context-menu";
import { ExternalLink } from "./components/external-link";
import { consoleBrowserUrl } from "./browser-links";
import { FailureNote, failureOf } from "./TemplateImport";
import type { BuildRun, JobView } from "./contracts/setup";

export function timelineTime(ms: number) {
  return new Date(ms).toLocaleString(locale(), {
    timeZone: "Asia/Shanghai",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
export function BuildMarker({
  code,
  branch,
  job,
  run,
}: {
  code: string;
  branch: string;
  job: JobView;
  run: BuildRun;
}) {
  useTranslation();
  const [expanded, setExpanded] = useState(false);
  const evidence = useQuery(
    {
      queryKey: [
        ...focusKey,
        "model",
        code,
        "build-evidence",
        job.job,
        run.number,
      ],
      queryFn: async () => {
        const result = (
          await focus("build_evidence", {
            code,
            job: job.job,
            build: run.number,
          })
        ).build_evidence;
        if (!result) throw new Error(t("timeline.noEvidenceResponse"));
        return result;
      },
      enabled: expanded,
      staleTime: run.building ? 60_000 : 30 * 60_000,
    },
    queryClient,
  );
  const data = evidence.data;
  const parameters = run.source_parameters ?? {};
  const software = data?.software_number ?? parameters.BUILD_CONST_BUILD_NUMBER;
  const state = run.building
    ? t("timeline.running")
    : (run.result ?? t("timeline.unknownResult"));
  return (
    <ContextTarget
      title={`${job.name} #${run.number}`}
      actions={buildContextActions(run)}
    >
      <li
        className={`change build-marker build-marker-${run.building ? "building" : run.result?.toLowerCase()}`}
      >
        <time
          className="change-when"
          dateTime={new Date(run.timestamp).toISOString()}
        >
          {timelineTime(run.timestamp)}
        </time>
        <div>
          <p className="build-marker-title">
            <Package size={16} aria-hidden="true" />
            <ExternalLink href={run.url}>
              {job.name} · Jenkins #{run.number}
            </ExternalLink>
            <span className="build-marker-result">{state}</span>
          </p>
          <p className="quiet">
            {t("timeline.started")}
            {!run.building && run.duration > 0
              ? ` · ${t("timeline.finished", { time: timelineTime(run.timestamp + run.duration) })}`
              : ""}{" "}
            · {t("timeline.timeOnly")}
          </p>
          {software ? (
            <p>{t("timeline.softwareNumber", { number: software })}</p>
          ) : null}
          {parameters.BUILD_CONST_BUILD_DISPLAY_ID ? (
            <p>{parameters.BUILD_CONST_BUILD_DISPLAY_ID}</p>
          ) : null}
          <p className="quiet">
            <GitBranch size={13} aria-hidden="true" />{" "}
            {branchInputMatches(run, branch)
              ? t("timeline.branchInputMatch")
              : t("timeline.modelJobAssociation")}
          </p>
          <div className="build-marker-actions">
            <ExternalLink href={consoleBrowserUrl(run)}>
              {t("timeline.console")}
            </ExternalLink>
            <button
              type="button"
              aria-expanded={expanded}
              onClick={() => setExpanded(!expanded)}
            >
              <Search size={14} aria-hidden="true" />
              {expanded ? t("timeline.collapse") : t("timeline.inspect")}
            </button>
          </div>
          {expanded ? (
            <div className="build-evidence">
              {Object.entries(parameters)
                .filter(([name]) => name.startsWith("BRANCH"))
                .map(([name, value]) => (
                  <p key={name}>
                    <code>{name}</code> · {value}
                  </p>
                ))}
              {evidence.isFetching ? (
                <p role="status">{t("timeline.inspecting")}</p>
              ) : null}
              {evidence.error ? (
                <FailureNote failure={failureOf(evidence.error)} />
              ) : null}
              {data ? (
                <>
                  <p>
                    <strong>
                      {data.system_manifest || data.vendor_manifest
                        ? t("timeline.archiveFound")
                        : t("timeline.archiveNotFound")}
                    </strong>
                  </p>
                  {data.system_manifest ? (
                    <p>
                      SYSTEM · <code>{data.system_manifest}</code>
                    </p>
                  ) : null}
                  {data.vendor_manifest ? (
                    <p>
                      VENDOR · <code>{data.vendor_manifest}</code>
                    </p>
                  ) : null}
                  {data.vendor_skipped ? (
                    <p>{t("timeline.vendorSkipped")}</p>
                  ) : null}
                  {data.archive_commit ? (
                    <p>
                      {t("timeline.archiveCommit")} ·{" "}
                      <code>{data.archive_commit}</code>
                    </p>
                  ) : null}
                  {data.archive_repository ? (
                    <p>
                      <code>{data.archive_repository}</code>
                    </p>
                  ) : null}
                  <p className="quiet">
                    {t("timeline.scanned", {
                      size: Math.ceil(
                        data.windows.reduce((n, w) => n + w.bytes, 0) / 1024,
                      ),
                    })}{" "}
                    ·{" "}
                    {data.windows
                      .map((w) =>
                        t("timeline.window", {
                          offset: w.start,
                          size: w.bytes,
                        }),
                      )
                      .join(", ")}
                  </p>
                  <p className="quiet">{t("timeline.verifyNeeded")}</p>
                </>
              ) : null}
              <button
                type="button"
                disabled={evidence.isFetching}
                onClick={() => void evidence.refetch()}
              >
                <RefreshCw size={14} aria-hidden="true" />
                {t("timeline.refreshEvidence")}
              </button>
            </div>
          ) : null}
        </div>
      </li>
    </ContextTarget>
  );
}
