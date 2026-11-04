import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight } from "lucide-react";
import type { FocusModel, ProjectStatus } from "./contracts/setup";
import { Button } from "./components/ui/button";
import { Spinner } from "./components/ui/spinner";
import { Alert, AlertDescription } from "./components/ui/alert";
import { t } from "./i18n";

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
  async function open() {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    try {
      await onOpen(seed);
    } catch (error) {
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
}: {
  status: ProjectStatus | null;
  onOpen: OpenProjectWorkbench;
}) {
  useTranslation();
  const projects =
    status?.projects.filter(
      (project) => project.check.valid && project.workspace,
    ) ?? [];
  if (!projects.length) return null;
  return (
    <section
      className="project-workbenches"
      aria-label={t("projectSetup.existingProjects")}
    >
      <h3>{t("projectSetup.existingProjects")}</h3>
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
          </div>
        ) : null,
      )}
    </section>
  );
}
