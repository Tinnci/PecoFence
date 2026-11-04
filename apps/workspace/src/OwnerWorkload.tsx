import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { WorkItems } from "./contracts/setup";
import type { WorkKind } from "./focus-cache";
import { OwnerChip } from "./Badges";
import { applyWorkView } from "./model-navigation";
import { normalizeView } from "./view-storage";
import {
  ownerScopeLabel,
  ownerWorkload,
  type OwnerLoad,
  type OwnerMetric,
} from "./owner-workload";
import { t } from "./i18n";

export function OwnerWorkload({
  code,
  issues,
  requirements,
  onOpen,
}: {
  code: string;
  issues: WorkItems | null;
  requirements: WorkItems | null;
  onOpen: (kind: WorkKind) => void;
}) {
  useTranslation();
  const [kind, setKind] = useState<WorkKind>("issues");
  const [shown, setShown] = useState(8);
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const tick = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(tick);
  }, []);
  const data = kind === "issues" ? issues : requirements;
  const rows = useMemo(
    () => ownerWorkload(data?.items ?? [], now),
    [data, now],
  );
  const names = new Map<string, number>();
  for (const row of rows) names.set(row.owner, (names.get(row.owner) ?? 0) + 1);
  const metrics: { key: OwnerMetric; label: string }[] = [
    { key: "open", label: t("work.open") },
    { key: "blocked", label: t("work.stage.blocked") },
    { key: "stale", label: t("work.ownerInactive") },
    { key: "due", label: t("work.ownerDue") },
  ];
  const open = (row: OwnerLoad, metric: OwnerMetric) => {
    applyWorkView(
      code,
      kind,
      normalizeView({
        stage: "open",
        attention: metric === "open" ? "" : metric,
        filters: { owner: [row.owner || "—"], source: [row.scope] },
        group: "none",
      }),
    );
    onOpen(kind);
  };
  const unknown = rows.reduce((sum, row) => sum + row.unknown, 0);
  if (!issues && !requirements) return null;
  return (
    <section className="owner-workload" aria-label={t("work.owners")}>
      <div className="column-head">
        <h3>{t("work.owners")}</h3>
        <div className="owner-kind" aria-label={t("work.ownerKind")}>
          {(["issues", "requirements"] as const).map((value) => (
            <button
              key={value}
              type="button"
              className="link"
              aria-pressed={kind === value}
              onClick={() => {
                setKind(value);
                setShown(8);
              }}
            >
              {t(`model.${value}`)}
            </button>
          ))}
        </div>
      </div>
      {data ? (
        rows.length ? (
          <>
            <table className="owner-table">
              <thead>
                <tr>
                  <th scope="col">{t("work.filterField.owner")}</th>
                  {metrics.map((metric) => (
                    <th scope="col" key={metric.key}>
                      {metric.label}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.slice(0, shown).map((row) => (
                  <tr key={row.key}>
                    <th scope="row">
                      <button
                        type="button"
                        className="link owner-person"
                        onClick={() => open(row, "open")}
                      >
                        <OwnerChip name={row.owner} />
                        {(names.get(row.owner) ?? 0) > 1 ? (
                          <small title={ownerScopeLabel(row.scope)}>
                            {ownerScopeLabel(row.scope)}
                          </small>
                        ) : null}
                      </button>
                    </th>
                    {metrics.map((metric) => (
                      <td key={metric.key}>
                        {row[metric.key] ? (
                          <button
                            type="button"
                            className={`link owner-count${metric.key === "blocked" || metric.key === "due" ? " short" : ""}`}
                            aria-label={t("work.ownerMetric", {
                              owner: row.owner || t("work.unassigned"),
                              metric: metric.label,
                              count: row[metric.key],
                            })}
                            onClick={() => open(row, metric.key)}
                          >
                            {row[metric.key]}
                          </button>
                        ) : (
                          <span className="quiet">0</span>
                        )}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
            {rows.length > 8 ? (
              <button
                type="button"
                className="link"
                onClick={() => setShown(shown === 8 ? rows.length : 8)}
              >
                {t(shown === 8 ? "work.ownerMore" : "work.ownerLess", {
                  count: rows.length - 8,
                })}
              </button>
            ) : null}
            {unknown ? (
              <p className="quiet owner-note">
                {t("work.ownerUnknown", { count: unknown })}
              </p>
            ) : null}
            {data.sources.some((source) => source.stale || source.error) ? (
              <p className="quiet owner-note">{t("work.ownerCached")}</p>
            ) : null}
          </>
        ) : (
          <p className="quiet empty-line">{t("work.ownerEmpty")}</p>
        )
      ) : (
        <p className="quiet empty-line">{t("work.ownerLoading")}</p>
      )}
    </section>
  );
}
