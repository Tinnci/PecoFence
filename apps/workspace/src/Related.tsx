import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Skeleton } from "@/components/ui/skeleton";
import { ExternalLink } from "@/components/external-link";
import type { RelatedChange, SimilarItem, WorkItem } from "./contracts/setup";
import { focus } from "./api";
import { queryClient } from "./focus-cache";
import { ChangeFiles, ChangeStatus, Linked } from "./ChangeDiff";
import { FailureNote, failureOf } from "./TemplateImport";
import { t } from "./i18n";
import { platformLabel } from "./Platforms";

const relatedKey = (code: string, item: WorkItem) =>
  ["related", code, item.source, item.instance, item.id] as const;

/** Changes bound to the item and items like it with their fixes; everything is local. */
export function Related({ code, item }: { code: string; item: WorkItem }) {
  useTranslation();
  const query = useQuery(
    {
      queryKey: relatedKey(code, item),
      queryFn: async () =>
        (
          await focus("related", {
            code,
            source: item.source,
            item: item.id,
            instance: item.instance,
          })
        ).related ?? null,
      staleTime: 2 * 60_000,
    },
    queryClient,
  );
  const data = query.data;
  // Fixes come from cached 流转记录: read those of similar 飞书项目 items not yet read, once,
  // then ask again so their 原因分析 and 修改方案 appear.
  const warmed = useRef(false);
  useEffect(() => {
    if (!data || warmed.current) return;
    const missing = data.similar.filter(
      (s) => s.item.source === "meegle" && !s.cause && !s.plan,
    );
    if (!missing.length) return;
    warmed.current = true;
    const byModel = new Map<string, SimilarItem[]>();
    for (const s of missing)
      byModel.set(s.model, [...(byModel.get(s.model) ?? []), s]);
    void Promise.allSettled(
      [...byModel].map(([model, list]) =>
        focus("flows", {
          code: model,
          items: list.slice(0, 10).map((s) => s.item.id),
          addresses: list.slice(0, 10).map((s) => s.item.url),
        }),
      ),
    ).then(() =>
      queryClient.invalidateQueries({ queryKey: relatedKey(code, item) }),
    );
  }, [data, code, item]);
  return (
    <>
      <section aria-label={t("related.changes")}>
        <h3>{t("related.changes")}</h3>
        {query.error ? <FailureNote failure={failureOf(query.error)} /> : null}
        {!data && !query.error ? (
          <Skeleton className="diff-loading-line" />
        ) : null}
        {data ? (
          data.changes.length ? (
            <ol className="related-changes">
              {data.changes.map((c) => (
                <ChangeRow key={c.change} change={c} />
              ))}
            </ol>
          ) : (
            <p className="quiet">{t("related.noChanges")}</p>
          )
        ) : null}
      </section>
      <section aria-label={t("related.similar")}>
        <h3>{t("related.similar")}</h3>
        {data?.platform && platformLabel(data.platform) ? (
          <p className="quiet related-platform">
            {t("platform.own", { platform: platformLabel(data.platform) })}
          </p>
        ) : null}
        {!data && !query.error ? (
          <Skeleton className="diff-loading-line" />
        ) : null}
        {data ? (
          data.similar.length ? (
            <ol className="related-similar">
              {data.similar.map((s) => (
                <SimilarRow
                  key={`${s.model}-${s.item.instance}-${s.item.id}`}
                  s={s}
                />
              ))}
            </ol>
          ) : (
            <p className="quiet">{t("related.noSimilar")}</p>
          )
        ) : null}
      </section>
    </>
  );
}

function ChangeRow({ change: c }: { change: RelatedChange }) {
  const kinds = [...new Set(c.evidence.map((e) => e.kind))];
  return (
    <li className="related-change">
      <span className="related-change-head">
        {c.status ? <ChangeStatus status={c.status} /> : null}
        {c.url ? (
          <ExternalLink href={c.url}>#{c.change}</ExternalLink>
        ) : (
          <span>#{c.change}</span>
        )}
        <span className="related-subject">
          {c.subject ?? t("related.unread")}
        </span>
      </span>
      <span className="related-meta">
        {kinds.map((k) => (
          <span
            key={k}
            className="related-evidence"
            title={c.evidence
              .filter((e) => e.kind === k)
              .map((e) => e.detail)
              .join("\n")}
          >
            {t(`related.evidence.${k}`)}
          </span>
        ))}
        {c.branch ? <span className="quiet">{c.branch}</span> : null}
        {c.owner ? <span className="quiet">{c.owner}</span> : null}
      </span>
      {/* Files open only for changes a summary read: SPM serves no others. */}
      {c.model ? <ChangeFiles code={c.model} change={c.change} /> : null}
    </li>
  );
}

function SimilarRow({ s }: { s: SimilarItem }) {
  const closed = ["done", "cancelled"].includes(s.item.group);
  return (
    <li className={`related-item${closed ? " is-closed" : ""}`}>
      <span className="related-change-head">
        <span
          className="related-score"
          title={t("related.score", { score: Math.round(s.score * 100) })}
        >
          <span style={{ width: `${Math.min(100, s.score * 100)}%` }} />
        </span>
        <ExternalLink href={s.item.url}>{s.item.display_id}</ExternalLink>
        <span className={`status-chip stage-${s.item.group}`}>
          {s.item.status}
        </span>
        <span className="quiet">
          {s.model}
          {platformLabel(s.platform) ? ` · ${platformLabel(s.platform)}` : ""}
        </span>
        {s.same_platform.map((k) => (
          <span key={k} className="platform-match">
            {t(`platform.same.${k}`)}
          </span>
        ))}
      </span>
      <span className="related-subject">{s.item.title}</span>
      {s.shared.length ? (
        <span className="related-meta">
          {s.shared.slice(0, 5).map((w) => (
            <span key={w} className="related-term">
              {w}
            </span>
          ))}
        </span>
      ) : null}
      {s.cause || s.plan ? (
        <dl className="related-fix">
          {s.cause ? (
            <div>
              <dt>{t("related.cause")}</dt>
              <dd>
                <Linked text={s.cause} />
              </dd>
            </div>
          ) : null}
          {s.plan ? (
            <div>
              <dt>{t("related.plan")}</dt>
              <dd>
                <Linked text={s.plan} />
              </dd>
            </div>
          ) : null}
        </dl>
      ) : null}
      {s.changes.length ? (
        <span className="related-meta">
          {s.changes.map((c) => (
            <span key={c.change} className="related-evidence">
              {c.url ? (
                <ExternalLink href={c.url}>#{c.change}</ExternalLink>
              ) : (
                `#${c.change}`
              )}
              {c.status
                ? ` ${c.status === "MERGED" ? t("model.merged") : c.status}`
                : ""}
            </span>
          ))}
        </span>
      ) : null}
    </li>
  );
}
