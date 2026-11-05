import { useState, type ComponentProps } from "react";
import { Field } from "@base-ui/react/field";
import { Button } from "./components/ui/button";
import { Input } from "./components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./components/ui/select";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import type { FocusModel, JobSource, JobView } from "./contracts/setup";
import { sourceRoutes } from "./job-sources";
import { ExternalLink } from "./components/external-link";
import { gerritSearchUrl, gerritTerm } from "./browser-links";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "./components/ui/collapsible";
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
  const current = new Set(routes.current.map((r) => r.job));
  return (
    <section className="source-routing" aria-label={t("source.routing")}>
      {entries.map(({ job, source }) => (
        <Collapsible key={job} className="source-route">
          <div className="source-route-line">
            <span className="source-route-kind">
              {current.has(job)
                ? t("source.currentLocal")
                : t("source.historical")}
            </span>
            <strong>{job.split("/").pop()}</strong>
            <span className="source-route-path">
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
            </span>
            {source.branch !== branch ? (
              <button
                type="button"
                className="link"
                onClick={() => onSelect(source.branch)}
              >
                {t("source.viewCurrent")}
              </button>
            ) : null}
            <CollapsibleTrigger className="link source-route-more">
              {t("source.details")}
            </CollapsibleTrigger>
          </div>
          <CollapsibleContent className="source-route-details">
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
          </CollapsibleContent>
        </Collapsible>
      ))}
    </section>
  );
}
/** A labelled input in the source editor (Base UI Field ties the label to the control). */
function SourceField({
  label,
  ...props
}: { label: string } & ComponentProps<typeof Input>) {
  return (
    <Field.Root className="source-field">
      <Field.Label>{label}</Field.Label>
      <Input {...props} />
    </Field.Root>
  );
}
export function JobSourcesEditor({
  jobs,
  sources,
  onChange,
  ignored = [],
  onRestore,
}: {
  jobs: JobView[];
  sources: Record<string, JobSource>;
  onChange: (sources: Record<string, JobSource>) => void;
  /** Excluded jobs: SPM reads no builds for them, so they cannot carry a source. */
  ignored?: string[];
  onRestore?: (job: string) => void;
}) {
  useTranslation();
  const [restored, setRestored] = useState<string[]>([]);
  const names = [
    ...new Set([
      ...jobs.filter((j) => !j.disabled).map((j) => j.job),
      ...Object.keys(sources),
      ...restored,
    ]),
  ];
  const [picked, setPicked] = useState("");
  const available = names.filter((job) => !sources[job]);
  const selected = available.includes(picked) ? picked : "";
  const update = (job: string, patch: Partial<JobSource>) =>
    onChange({ ...sources, [job]: { ...sources[job], ...patch } });
  return (
    <section
      id="settings-sources"
      className="job-sources-editor"
      aria-label={t("source.routing")}
    >
      <h2>{t("source.routing")}</h2>
      <p className="quiet">{t("source.editorHelp")}</p>
      {Object.keys(sources).map((job) => (
        <div className="source-editor-job" key={job}>
          <div className="source-editor-head">
            <h3>{job.split("/").pop()}</h3>
            <Button
              type="button"
              variant="ghost"
              size="xs"
              className="manage-danger"
              onClick={() => {
                const next = { ...sources };
                delete next[job];
                onChange(next);
              }}
            >
              {t("source.remove")}
            </Button>
          </div>
          <fieldset>
            <Field.Root className="source-field" invalid={!sources[job].branch}>
              <Field.Label>{t("source.branch")}</Field.Label>
              <Input
                value={sources[job].branch}
                maxLength={200}
                aria-invalid={!sources[job].branch}
                onChange={(e) => update(job, { branch: e.target.value.trim() })}
              />
              {!sources[job].branch ? (
                <span className="source-required">
                  {t("source.branchRequired")}
                </span>
              ) : null}
            </Field.Root>
            <SourceField
              label={t("source.previous")}
              value={sources[job].previous.join(", ")}
              onChange={(e) =>
                update(job, {
                  previous: e.target.value.split(/[,，]/).map((v) => v.trim()),
                })
              }
            />
            <SourceField
              label={t("source.firstBuild")}
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
            <SourceField
              label={t("source.note")}
              value={sources[job].note}
              maxLength={1000}
              onChange={(e) => update(job, { note: e.target.value })}
            />
            <fieldset className="source-history-editor">
              <legend>{t("source.historyTitle")}</legend>
              {(sources[job].history ?? []).map((range, i) => (
                <div key={i} className="source-history-row">
                  <SourceField
                    label={t("source.rangeStart")}
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
                  <SourceField
                    label={t("source.rangeEnd")}
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
                  <SourceField
                    label={t("source.rangeBranch")}
                    value={range.branch}
                    onChange={(e) =>
                      update(job, {
                        history: sources[job].history!.map((r, n) =>
                          n === i ? { ...r, branch: e.target.value.trim() } : r,
                        ),
                      })
                    }
                  />
                  <SourceField
                    label={t("source.note")}
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
          </fieldset>
        </div>
      ))}
      {!names.length && ignored.length ? (
        <div className="source-excluded" role="status">
          <p className="quiet">{t("source.allExcluded")}</p>
          {ignored.map((job) => (
            <Button
              key={job}
              type="button"
              variant="outline"
              size="xs"
              onClick={() => {
                onRestore?.(job);
                setRestored([...restored, job]);
              }}
            >
              {t("source.restoreJob", { job: job.split("/").pop() })}
            </Button>
          ))}
        </div>
      ) : null}
      <div className="source-add-row">
        <Select
          value={selected || null}
          items={available.map((job) => ({
            value: job,
            label: job.split("/").pop() ?? job,
          }))}
          onValueChange={(value) => setPicked((value as string) ?? "")}
        >
          <SelectTrigger size="sm" aria-label={t("source.chooseJob")}>
            <SelectValue placeholder={t("source.chooseJob")} />
          </SelectTrigger>
          <SelectContent>
            {available.map((job) => (
              <SelectItem key={job} value={job}>
                {job.split("/").pop()}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type="button"
          variant="outline"
          size="sm"
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
        </Button>
      </div>
    </section>
  );
}
