import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link2 } from "lucide-react";
import { t } from "./i18n";
import { ExternalLink } from "./components/external-link";
import { jobBrowserUrl } from "./browser-links";
import { FailureNote, failureOf } from "./TemplateImport";
import type { JobView, SetupFailure } from "./contracts/setup";
import type { BuildAssociation } from "./build-association";
export function BranchBuildAssociation({
  association,
  loading,
  onConfirm,
  onReset,
}: {
  association: BuildAssociation;
  loading: boolean;
  onConfirm?: (job: JobView) => Promise<unknown>;
  onReset?: () => Promise<unknown>;
}) {
  useTranslation();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  async function save(action: () => Promise<unknown>) {
    setBusy(true);
    setFailure(null);
    try {
      await action();
    } catch (error) {
      setFailure(failureOf(error));
    } finally {
      setBusy(false);
    }
  }
  if (
    (association.kind === "source" || association.kind === "historical") &&
    !association.missing.length
  )
    return null;
  return (
    <div className="build-association">
      {association.kind === "unconfirmed" ? (
        <>
          <p>{t("association.candidatesFound")}</p>
          <p className="quiet">{t("association.candidateReason")}</p>
          {association.candidates.map((job) => (
            <div className="build-candidate" key={job.job}>
              <ExternalLink href={jobBrowserUrl(job)}>{job.name}</ExternalLink>
              {job.builds[0] ? (
                <ExternalLink href={job.builds[0].url}>
                  #{job.builds[0].number} ·{" "}
                  {job.builds[0].result ?? t("timeline.running")}
                </ExternalLink>
              ) : null}
              {onConfirm ? (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void save(() => onConfirm(job))}
                >
                  <Link2 size={14} aria-hidden="true" />
                  {t("association.confirm")}
                </button>
              ) : null}
            </div>
          ))}
        </>
      ) : association.kind === "none" && !loading ? (
        <p className="quiet">{t("association.notEstablished")}</p>
      ) : null}
      {association.kind === "configured" ? (
        <p className="quiet">
          {t("association.configured")}
          {onReset ? (
            <>
              {" "}
              ·{" "}
              <button
                type="button"
                disabled={busy}
                onClick={() => void save(onReset)}
              >
                {t("association.reset")}
              </button>
            </>
          ) : null}
        </p>
      ) : null}
      {association.kind === "input" ? (
        <p className="quiet">{t("association.input")}</p>
      ) : null}
      {association.kind === "name" ? (
        <p className="quiet">{t("association.name")}</p>
      ) : null}
      {association.missing.length ? (
        <p className="short">
          {t("association.missing", {
            jobs: association.missing
              .map((name) => name.split("/").pop())
              .join(", "),
          })}
        </p>
      ) : null}
      {failure ? <FailureNote failure={failure} /> : null}
    </div>
  );
}
