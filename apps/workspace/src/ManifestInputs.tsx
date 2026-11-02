import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import { ExternalLink } from "./components/external-link";
import { branchQueryBrowserUrl, jobBrowserUrl } from "./browser-links";
import type { JobView } from "./contracts/setup";
/** Raw repo inputs stay separate from source repository branches and layer assignments. */
export function manifestInputs(job: JobView) {
  return ["T", "V", ""].flatMap((slot) => {
    const suffix = slot ? `_${slot}` : "";
    const repository = job.parameters[`REPO_URL${suffix}`];
    const ref = job.parameters[`BRANCH${suffix}`];
    return typeof repository === "string" &&
      typeof ref === "string" &&
      repository.trim() &&
      ref.trim()
      ? [
          {
            slot: slot || "REPO",
            repository: repository.trim(),
            ref: ref.trim(),
          },
        ]
      : [];
  });
}
export function ManifestInputs({ jobs }: { jobs: JobView[] }) {
  useTranslation();
  const entries = jobs
    .map((job) => ({ job, inputs: manifestInputs(job) }))
    .filter((e) => e.inputs.length);
  if (!entries.length) return null;
  return (
    <details className="manifest-inputs">
      <summary>{t("release.manifestInputs")}</summary>
      <p className="quiet">{t("release.manifestExplanation")}</p>
      {entries.map(({ job, inputs }) => (
        <section key={job.job}>
          <h3>
            <ExternalLink href={jobBrowserUrl(job)}>{job.name}</ExternalLink>
          </h3>
          <dl>
            {inputs.map((input) => (
              <div key={input.slot}>
                <dt>
                  {input.slot}
                  <span className="quiet">{t("release.layerUnverified")}</span>
                </dt>
                <dd>
                  <code>{input.ref}</code>
                  <small>{input.repository}</small>
                </dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
      <ExternalLink
        href={branchQueryBrowserUrl(
          jobs.flatMap((j) => j.builds).find((r) => r.url),
        )}
      >
        {t("release.branchTool")}
      </ExternalLink>
    </details>
  );
}
