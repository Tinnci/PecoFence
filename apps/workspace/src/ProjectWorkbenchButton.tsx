import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, Trash2 } from "lucide-react";
import type {
  FocusModel,
  ImportedProject,
  ProjectStatus,
} from "./contracts/setup";
import { projectChange } from "./api";
import { RemoveModelDialog } from "./RemoveModelDialog";
import { Button } from "./components/ui/button";
import { Spinner } from "./components/ui/spinner";
import { Alert, AlertDescription } from "./components/ui/alert";
import { t } from "./i18n";
import type { SetupFailure } from "./contracts/setup";
import { FailureNote, failureOf } from "./TemplateImport";

export type OpenProjectWorkbench = (seed: FocusModel) => Promise<void>;

export function ProjectWorkbenchButton({
  seed,
  onOpen,
}: {
  seed: FocusModel;
  onOpen: OpenProjectWorkbench;
}) {
  useTranslation();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  // SPM failures carry a code the workspace already has words for.
  const [coded, setCoded] = useState<SetupFailure | null>(null);
  async function open() {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    setCoded(null);
    try {
      await onOpen(seed);
    } catch (error) {
      if (error && typeof error === "object" && "code" in error) {
        setCoded(failureOf(error));
        return;
      }
      const detail =
        error &&
        typeof error === "object" &&
        "message" in error &&
        typeof error.message === "string"
          ? error.message
          : "";
      setFailure(
        detail
          ? `${t("projectSetup.openWorkbenchFailed")}：${detail}`
          : t("projectSetup.openWorkbenchFailed"),
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="project-workbench-action">
      <Button disabled={busy} onClick={() => void open()}>
        {busy ? (
          <Spinner aria-hidden="true" />
        ) : (
          <ArrowRight aria-hidden="true" />
        )}
        {t("projectSetup.openWorkbench")}
      </Button>
      {coded ? <FailureNote failure={coded} /> : null}
      {failure ? (
        <Alert className="alert-warning">
          <AlertDescription>{failure}</AlertDescription>
        </Alert>
      ) : null}
    </div>
  );
}

/** Project entry stays outside advanced import, and survives application restarts. */
export function ProjectWorkbenches({
  status,
  onOpen,
  onChanged,
}: {
  status: ProjectStatus | null;
  onOpen: OpenProjectWorkbench;
  onChanged: () => Promise<void>;
}) {
  useTranslation();
  const [removing, setRemoving] = useState<ImportedProject | null>(null);
  const [removed, setRemoved] = useState<ImportedProject | null>(null);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  async function remove(project: ImportedProject) {
    await projectChange("remove", project.project ?? "", project.scope ?? "");
    setRemoved(project);
    await onChanged();
  }
  async function undo(project: ImportedProject) {
    setRemoved(null);
    setFailure(null);
    try {
      await projectChange(
        "restore",
        project.project ?? "",
        project.scope ?? "",
      );
      await onChanged();
    } catch (error) {
      setFailure(failureOf(error));
    }
  }
  const projects =
    status?.projects.filter(
      (project) => project.check.valid && project.workspace,
    ) ?? [];
  if (!projects.length && !removed && !failure) return null;
  return (
    <section
      className="project-workbenches"
      aria-label={t("projectSetup.existingProjects")}
    >
      <h3>{t("projectSetup.existingProjects")}</h3>
      {removing ? (
        <RemoveModelDialog
          name={removing.project ?? ""}
          title={t("projectSetup.removeTitle", {
            name: `${removing.project} / ${removing.scope}`,
          })}
          description={t("projectSetup.removeHint")}
          onRemove={() => remove(removing)}
          onClose={() => setRemoving(null)}
        />
      ) : null}
      {removed ? (
        <div className="undo-bar" role="status">
          <span>
            {t("projectSetup.removed", {
              name: `${removed.project} / ${removed.scope}`,
            })}
          </span>
          <Button variant="ghost" size="sm" onClick={() => void undo(removed)}>
            {t("model.undo")}
          </Button>
        </div>
      ) : null}
      {failure ? <FailureNote failure={failure} /> : null}
      {projects.map((project) =>
        project.workspace ? (
          <div className="project-workbench-row" key={project.directory}>
            <strong>
              {project.project} <span className="quiet">/ {project.scope}</span>
            </strong>
            {project.workspace.code !== project.project ? (
              <span className="quiet">
                {t("projectSetup.workbenchCode", {
                  code: project.workspace.code,
                })}
              </span>
            ) : null}
            <ProjectWorkbenchButton seed={project.workspace} onOpen={onOpen} />
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              disabled={project.active}
              aria-label={t("projectSetup.remove")}
              title={
                project.active
                  ? t("projectSetup.removeActive")
                  : t("projectSetup.remove")
              }
              onClick={() => setRemoving(project)}
            >
              <Trash2 aria-hidden="true" />
            </Button>
          </div>
        ) : null,
      )}
    </section>
  );
}
