import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
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
  refreshing = false,
}: {
  model: FocusModel;
  lines: ReleaseLine[];
  ignored: string[];
  jobs: JobView[];
  onChange: (lines: ReleaseLine[], ignored: string[]) => void;
  onRefresh?: () => void;
  refreshing?: boolean;
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
    <section
      id="settings-release"
      className="release-editor"
      aria-label={t("release.title")}
    >
      <div className="column-head">
        <h2>{t("release.title")}</h2>
        {onRefresh ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={refreshing}
            onClick={onRefresh}
          >
            {refreshing ? (
              <Spinner aria-hidden="true" />
            ) : (
              <RefreshCw aria-hidden="true" />
            )}
            {t("release.refreshJobs")}
          </Button>
        ) : null}
      </div>
      <p className="quiet">{t("release.help")}</p>
      {lines.map((line, i) => (
        <fieldset className="release-line-editor" key={line.id}>
          <legend>{line.label || t("release.newLine")}</legend>
          <div className="release-line-head">
            <label className="release-name">
              <span>{t("release.label")}</span>
              <Input
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
                  onValueChange={(value) =>
                    value && update(i, { phase: value })
                  }
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
          </div>
          <div className="release-pick">
            <fieldset className="release-choices">
              <legend>{t("release.branches")}</legend>
              {model.branches.map((branch) => (
                <label key={branch}>
                  <Checkbox
                    checked={line.branches.includes(branch)}
                    onCheckedChange={() =>
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
                    <Checkbox
                      checked={line.jobs.includes(job)}
                      onCheckedChange={() =>
                        update(i, { jobs: toggle(line.jobs, job) })
                      }
                    />
                    {job.split("/").pop()}
                  </label>
                ))}
            </fieldset>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="xs"
            className="manage-danger release-remove"
            onClick={() =>
              onChange(
                lines.filter((_, n) => n !== i),
                ignored,
              )
            }
          >
            <Trash2 aria-hidden="true" />
            {t("release.remove")}
          </Button>
        </fieldset>
      ))}
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="release-add"
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
      </Button>
      {names.length ? (
        <fieldset className="release-choices">
          <legend>{t("release.ignoredJobs")}</legend>
          {names.map((job) => (
            <label key={job}>
              <Checkbox
                checked={ignored.includes(job)}
                onCheckedChange={() =>
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
        className="release-tool"
        href={branchQueryBrowserUrl(
          jobs.flatMap((j) => j.builds).find((r) => r.url),
        )}
      >
        {t("release.branchTool")}
      </ExternalLink>
    </section>
  );
}
