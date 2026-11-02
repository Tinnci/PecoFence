import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./components/ui/select";
import { Plus, Trash2, GitBranch } from "lucide-react";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import { ExternalLink } from "./components/external-link";
import { branchQueryBrowserUrl } from "./browser-links";
import type { FocusModel, JobView, ReleaseLine } from "./contracts/setup";
export function lifecycleName(value?: string) {
  switch (value) {
    case "maintenance":
      return t("release.maintenance");
    case "upgrade":
      return t("release.upgrade");
    case "archived":
      return t("release.archived");
    default:
      return t("release.development");
  }
}
export function ReleaseLinesEditor({
  model,
  lines,
  ignored,
  jobs,
  onChange,
  onRefresh,
}: {
  model: FocusModel;
  lines: ReleaseLine[];
  ignored: string[];
  jobs: JobView[];
  onChange: (lines: ReleaseLine[], ignored: string[]) => void;
  onRefresh?: () => void;
}) {
  useTranslation();
  const names = [
    ...new Set([
      ...jobs.map((j) => j.job),
      ...ignored,
      ...lines.flatMap((l) => l.jobs),
    ]),
  ];
  const update = (index: number, patch: Partial<ReleaseLine>) =>
    onChange(
      lines.map((line, i) => (i === index ? { ...line, ...patch } : line)),
      ignored,
    );
  const toggle = (values: string[], value: string) =>
    values.includes(value)
      ? values.filter((v) => v !== value)
      : [...values, value];
  return (
    <section className="release-editor" aria-label={t("release.title")}>
      <h2>
        <GitBranch size={16} aria-hidden="true" />
        {t("release.title")}
      </h2>
      <p className="quiet">{t("release.help")}</p>
      {onRefresh ? (
        <button type="button" className="link" onClick={onRefresh}>
          {t("release.refreshJobs")}
        </button>
      ) : null}
      {lines.map((line, i) => (
        <fieldset className="release-line-editor" key={line.id}>
          <legend>{line.label || t("release.newLine")}</legend>
          <label>
            {t("release.label")}
            <input
              value={line.label}
              maxLength={100}
              onChange={(e) => update(i, { label: e.target.value })}
            />
          </label>
          <div className="release-selects">
            <label>
              {t("release.lifecycle")}
              <Select
                value={line.lifecycle}
                items={[
                  "development",
                  "maintenance",
                  "upgrade",
                  "archived",
                ].map((value) => ({ value, label: lifecycleName(value) }))}
                onValueChange={(value) =>
                  value && update(i, { lifecycle: value })
                }
              >
                <SelectTrigger size="sm" aria-label={t("release.lifecycle")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {["development", "maintenance", "upgrade", "archived"].map(
                    (value) => (
                      <SelectItem key={value} value={value}>
                        {lifecycleName(value)}
                      </SelectItem>
                    ),
                  )}
                </SelectContent>
              </Select>
            </label>
            <label>
              {t("release.phase")}
              <Select
                value={line.phase}
                items={["main", "ir", "pb", "mp", "old"].map((value) => ({
                  value,
                  label: value.toUpperCase(),
                }))}
                onValueChange={(value) => value && update(i, { phase: value })}
              >
                <SelectTrigger size="sm" aria-label={t("release.phase")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {["main", "ir", "pb", "mp", "old"].map((value) => (
                    <SelectItem key={value} value={value}>
                      {value.toUpperCase()}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
          </div>
          <fieldset className="release-choices">
            <legend>{t("release.branches")}</legend>
            {model.branches.map((branch) => (
              <label key={branch}>
                <input
                  type="checkbox"
                  checked={line.branches.includes(branch)}
                  onChange={() =>
                    update(i, { branches: toggle(line.branches, branch) })
                  }
                />
                {branch}
              </label>
            ))}
          </fieldset>
          <fieldset className="release-choices">
            <legend>{t("release.jobs")}</legend>
            {names
              .filter((name) => !ignored.includes(name))
              .map((job) => (
                <label key={job}>
                  <input
                    type="checkbox"
                    checked={line.jobs.includes(job)}
                    onChange={() => update(i, { jobs: toggle(line.jobs, job) })}
                  />
                  {job.split("/").pop()}
                </label>
              ))}
          </fieldset>
          <button
            type="button"
            className="link danger"
            onClick={() =>
              onChange(
                lines.filter((_, n) => n !== i),
                ignored,
              )
            }
          >
            <Trash2 size={14} aria-hidden="true" />
            {t("release.remove")}
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        className="link"
        onClick={() =>
          onChange(
            [
              ...lines,
              {
                id: `line-${crypto.randomUUID().slice(0, 8)}`,
                label: "",
                phase: "mp",
                lifecycle: "maintenance",
                jobs: [],
                branches: [],
              },
            ],
            ignored,
          )
        }
      >
        <Plus size={14} aria-hidden="true" />
        {t("release.add")}
      </button>
      {names.length ? (
        <fieldset className="release-choices">
          <legend>{t("release.ignoredJobs")}</legend>
          {names.map((job) => (
            <label key={job}>
              <input
                type="checkbox"
                checked={ignored.includes(job)}
                onChange={() =>
                  onChange(
                    ignored.includes(job)
                      ? lines
                      : lines.map((line) => ({
                          ...line,
                          jobs: line.jobs.filter((j) => j !== job),
                        })),
                    toggle(ignored, job),
                  )
                }
              />
              {job.split("/").pop()}
            </label>
          ))}
        </fieldset>
      ) : null}
      <ExternalLink
        href={branchQueryBrowserUrl(
          jobs.flatMap((j) => j.builds).find((r) => r.url),
        )}
      >
        {t("release.branchTool")}
      </ExternalLink>
    </section>
  );
}
