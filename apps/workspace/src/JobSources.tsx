import { useState } from "react";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import type { FocusModel, JobSource, JobView } from "./contracts/setup";
import { sourceRoutes } from "./job-sources";
import { ExternalLink } from "./components/external-link";
import { gerritSearchUrl, gerritTerm } from "./browser-links";
export function SourceRoutingNote({
  model,
  branch,
  sampleUrl,
  onSelect,
}: {
  model?: FocusModel;
  branch: string;
  sampleUrl?: string;
  onSelect: (branch: string) => void;
}) {
  useTranslation();
  const routes = sourceRoutes(model, branch);
  const entries = [...routes.current, ...routes.historical];
  if (!entries.length) return null;
  return (
    <section className="source-routing" aria-label={t("source.routing")}>
      <h3>
        {routes.current.length
          ? t("source.currentLocal")
          : t("source.historical")}
      </h3>
      {entries.map(({ job, source }) => (
        <div key={job}>
          <p>
            <strong>{job.split("/").pop()}</strong> · {t("source.local")}
          </p>
          <p className="source-route-path">
            {source.previous.map((old) => (
              <span key={old}>
                <code>{old}</code> →{" "}
              </span>
            ))}
            <ExternalLink
              href={gerritSearchUrl(
                sampleUrl,
                gerritTerm("branch", source.branch),
              )}
            >
              {source.branch}
            </ExternalLink>
          </p>
          {source.branch !== branch ? (
            <button
              type="button"
              className="link"
              onClick={() => onSelect(source.branch)}
            >
              {t("source.viewCurrent")}
            </button>
          ) : null}
          <p className="quiet">
            {source.since_build
              ? t("source.fromBuild", { number: source.since_build })
              : source.history?.length
                ? t("source.onlyAssigned")
                : t("source.cutoverUnknown")}
          </p>
          {source.history?.length ? (
            <ul className="source-history-summary">
              {source.history
                .slice()
                .sort((a, b) => a.first_build - b.first_build)
                .map((range, i) => (
                  <li key={i}>
                    #{range.first_build}–#{range.last_build} ·{" "}
                    <code>{range.branch}</code> · {t("source.manual")}
                    {range.note ? ` · ${range.note}` : ""}
                  </li>
                ))}
            </ul>
          ) : null}
          {source.note ? <p className="quiet">{source.note}</p> : null}
        </div>
      ))}
    </section>
  );
}
export function JobSourcesEditor({
  jobs,
  sources,
  onChange,
}: {
  jobs: JobView[];
  sources: Record<string, JobSource>;
  onChange: (sources: Record<string, JobSource>) => void;
}) {
  useTranslation();
  const names = [
    ...new Set([
      ...jobs.filter((j) => !j.disabled).map((j) => j.job),
      ...Object.keys(sources),
    ]),
  ];
  const [picked, setPicked] = useState("");
  const available = names.filter((job) => !sources[job]);
  const selected = available.includes(picked) ? picked : "";
  const update = (job: string, patch: Partial<JobSource>) =>
    onChange({ ...sources, [job]: { ...sources[job], ...patch } });
  return (
    <section className="job-sources-editor" aria-label={t("source.routing")}>
      <h2>{t("source.routing")}</h2>
      <p className="quiet">{t("source.editorHelp")}</p>
      {Object.keys(sources).map((job) => (
        <div className="source-editor-job" key={job}>
          <h3>{job.split("/").pop()}</h3>
          <fieldset>
            <label>
              {t("source.branch")}
              <input
                value={sources[job].branch}
                maxLength={200}
                onChange={(e) => update(job, { branch: e.target.value.trim() })}
              />
            </label>
            <label>
              {t("source.previous")}
              <input
                value={sources[job].previous.join(", ")}
                onChange={(e) =>
                  update(job, {
                    previous: e.target.value
                      .split(/[,，]/)
                      .map((v) => v.trim()),
                  })
                }
              />
            </label>
            <label>
              {t("source.firstBuild")}
              <input
                type="number"
                min={1}
                step={1}
                placeholder={t("source.unknown")}
                value={sources[job].since_build ?? ""}
                onChange={(e) =>
                  update(job, {
                    since_build: e.target.value
                      ? Math.max(1, Math.round(Number(e.target.value)))
                      : undefined,
                  })
                }
              />
            </label>
            <label>
              {t("source.note")}
              <input
                value={sources[job].note}
                maxLength={1000}
                onChange={(e) => update(job, { note: e.target.value })}
              />
            </label>
            <fieldset className="source-history-editor">
              <legend>{t("source.historyTitle")}</legend>
              {(sources[job].history ?? []).map((range, i) => (
                <div key={i} className="source-history-row">
                  <label>
                    {t("source.rangeStart")}
                    <input
                      type="number"
                      min={1}
                      value={range.first_build || ""}
                      onChange={(e) =>
                        update(job, {
                          history: sources[job].history!.map((r, n) =>
                            n === i
                              ? {
                                  ...r,
                                  first_build: e.target.value
                                    ? Math.max(
                                        1,
                                        Math.round(Number(e.target.value)),
                                      )
                                    : 0,
                                }
                              : r,
                          ),
                        })
                      }
                    />
                  </label>
                  <label>
                    {t("source.rangeEnd")}
                    <input
                      type="number"
                      min={1}
                      value={range.last_build || ""}
                      onChange={(e) =>
                        update(job, {
                          history: sources[job].history!.map((r, n) =>
                            n === i
                              ? {
                                  ...r,
                                  last_build: e.target.value
                                    ? Math.max(
                                        1,
                                        Math.round(Number(e.target.value)),
                                      )
                                    : 0,
                                }
                              : r,
                          ),
                        })
                      }
                    />
                  </label>
                  <label>
                    {t("source.rangeBranch")}
                    <input
                      value={range.branch}
                      onChange={(e) =>
                        update(job, {
                          history: sources[job].history!.map((r, n) =>
                            n === i
                              ? { ...r, branch: e.target.value.trim() }
                              : r,
                          ),
                        })
                      }
                    />
                  </label>
                  <label>
                    {t("source.note")}
                    <input
                      value={range.note}
                      maxLength={1000}
                      onChange={(e) =>
                        update(job, {
                          history: sources[job].history!.map((r, n) =>
                            n === i ? { ...r, note: e.target.value } : r,
                          ),
                        })
                      }
                    />
                  </label>
                  <button
                    type="button"
                    className="link"
                    onClick={() =>
                      update(job, {
                        history: sources[job].history!.filter(
                          (_, n) => n !== i,
                        ),
                      })
                    }
                  >
                    {t("source.removeRange")}
                  </button>
                </div>
              ))}
              <button
                type="button"
                className="link"
                onClick={() => {
                  const next =
                    Math.max(
                      0,
                      ...(sources[job].history ?? []).map((r) => r.last_build),
                    ) + 1;
                  update(job, {
                    history: [
                      ...(sources[job].history ?? []),
                      {
                        branch: "",
                        first_build: next,
                        last_build: next,
                        note: "",
                      },
                    ],
                  });
                }}
              >
                {t("source.addRange")}
              </button>
            </fieldset>
            <button
              type="button"
              className="link"
              onClick={() => {
                const next = { ...sources };
                delete next[job];
                onChange(next);
              }}
            >
              {t("source.remove")}
            </button>
          </fieldset>
        </div>
      ))}
      <div className="source-add-row">
        <select
          aria-label={t("source.chooseJob")}
          value={selected}
          onChange={(e) => setPicked(e.target.value)}
        >
          <option value="">{t("source.chooseJob")}</option>
          {available.map((job) => (
            <option key={job} value={job}>
              {job.split("/").pop()}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="link"
          disabled={!selected}
          onClick={() => {
            onChange({
              ...sources,
              [selected]: { branch: "", previous: [], note: "" },
            });
            setPicked("");
          }}
        >
          {t("source.add")}
        </button>
      </div>
    </section>
  );
}
