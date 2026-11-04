import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Tag } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import { ExternalLink } from "@/components/external-link";
import { OwnerChip, SeverityBadge, CommitsChip } from "./Badges";
import type { WorkItem } from "./contracts/setup";
import { ItemFlow } from "./Flow";
import { Linked } from "./ChangeDiff";
import { readableText } from "./work-text";
import { isDue, updatedAt } from "./work-activity";
import { ownerScope, ownerScopeLabel } from "./owner-workload";
import { t } from "./i18n";

export const workItemKey = (item: WorkItem) =>
  JSON.stringify([item.source, item.scope_id, item.url, item.id]);
function date(value: string) {
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value;
  const time = updatedAt(value);
  return time === null ? "—" : new Date(time).toLocaleString();
}
function Text({ value }: { value: string }) {
  return (
    <div className="work-detail-text">
      <Linked text={readableText(value)} />
    </div>
  );
}
export function WorkItemDetails({
  code,
  item,
  onClose,
  onEdit,
  history,
}: {
  code: string;
  item: WorkItem;
  onClose: () => void;
  onEdit: () => void;
  history: ReactNode;
}) {
  useTranslation();
  const activity = item.activity;
  const local = item.local;
  const comment = activity.last_comment;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent className="work-detail-dialog">
        <header className="work-detail-header">
          <DialogTitle>
            {item.display_id} · {t("work.details")}
          </DialogTitle>
          <DialogDescription className="work-detail-title">
            {item.title}
          </DialogDescription>
          <div className="work-meta">
            <SeverityBadge value={item.severity} />
            <span className={`status-chip stage-${item.group}`}>
              {item.status}
            </span>
            <OwnerChip name={item.owner} />
            {item.commits.length ? (
              <CommitsChip numbers={item.commits} />
            ) : null}
          </div>
          <div className="work-detail-actions">
            <ExternalLink href={item.url}>{t("work.openSource")}</ExternalLink>
            <button type="button" className="link" onClick={onEdit}>
              <Tag size={13} aria-hidden="true" />
              {t("work.mark")}
            </button>
          </div>
        </header>
        <div className="work-detail-body">
          <section aria-label={t("work.detailInfo")}>
            <h3>{t("work.detailInfo")}</h3>
            <dl className="work-detail-fields">
              <dt>{t("work.filterField.source")}</dt>
              <dd>{ownerScopeLabel(ownerScope(item))}</dd>
              <dt>{t("work.field.found_version")}</dt>
              <dd>{activity.found_versions.join("、") || "—"}</dd>
              <dt>{t("work.field.target_version")}</dt>
              <dd>{activity.target_versions.join("、") || "—"}</dd>
              <dt>{t("work.detailCreated")}</dt>
              <dd>{date(activity.created)}</dd>
              <dt>{t("work.detailUpdated")}</dt>
              <dd>{date(item.updated)}</dd>
              <dt>{t("work.detailActivity")}</dt>
              <dd>{date(activity.last_activity)}</dd>
              <dt>{t("work.detailStatusSince")}</dt>
              <dd>
                {date(activity.status_since)}
                {activity.status_since ? (
                  <small className="quiet">
                    {" "}
                    ·{" "}
                    {t(
                      `work.since.${activity.status_since_source || "unknown"}`,
                    )}
                  </small>
                ) : null}
              </dd>
              {item.origin ? (
                <>
                  <dt>{t("work.field.origin")}</dt>
                  <dd>
                    {t(`work.origin.${item.origin}`)}
                    {item.origin_evidence ? (
                      <Text value={item.origin_evidence} />
                    ) : null}
                  </dd>
                </>
              ) : null}
            </dl>
          </section>
          {activity.progress ? (
            <section aria-label={t("work.progress")}>
              <h3>{t("work.progress")}</h3>
              <Text value={activity.progress} />
            </section>
          ) : null}
          {comment ? (
            <section aria-label={t("work.latestComment")}>
              <h3>{t("work.latestComment")}</h3>
              <div className="work-meta">
                <OwnerChip name={comment.by} />
                <time dateTime={comment.at}>{date(comment.at)}</time>
              </div>
              <Text value={comment.text} />
            </section>
          ) : null}
          <section aria-label={t("work.localTitle")}>
            <h3>{t("work.localTitle")}</h3>
            {local?.tags.length ? (
              <div className="local-tags">
                {local.tags.map((tag) => (
                  <span key={tag} className="local-tag">
                    {tag}
                  </span>
                ))}
              </div>
            ) : null}
            {local?.next_check ? (
              <p className={isDue(item) ? "short" : "quiet"}>
                {t("work.nextCheck", { date: local.next_check })}
              </p>
            ) : null}
            {local?.corrections.length ? (
              <dl className="work-detail-corrections">
                {local.corrections.map((correction) => (
                  <div key={correction.field}>
                    <dt>
                      {t(`work.field.${correction.field}`)}{" "}
                      <span className="quiet">
                        {t(`work.correction.${correction.state}`)}
                      </span>
                    </dt>
                    <dd>
                      {correction.field === "stage"
                        ? t(`work.stage.${correction.value}`)
                        : correction.field === "origin"
                          ? t(`work.origin.${correction.value}`)
                          : correction.value}
                      {correction.reason ? (
                        <Text value={correction.reason} />
                      ) : null}
                    </dd>
                  </div>
                ))}
              </dl>
            ) : null}
            {local?.notes.length ? (
              <ol className="work-detail-notes">
                {local.notes.map((note, index) => (
                  <li key={`${note.at}:${index}`}>
                    <time className="quiet" dateTime={note.at}>
                      {date(note.at)}
                    </time>
                    <Text value={note.text} />
                  </li>
                ))}
              </ol>
            ) : null}
            {!local?.tags.length &&
            !local?.notes.length &&
            !local?.corrections.length &&
            !local?.next_check ? (
              <p className="quiet">{t("work.noLocalDetails")}</p>
            ) : null}
          </section>
          {item.source === "meegle" ? (
            <section aria-label={t("flow.title")}>
              <ItemFlow code={code} item={item} initiallyOpen />
            </section>
          ) : null}
          {history}
        </div>
      </DialogContent>
    </Dialog>
  );
}
