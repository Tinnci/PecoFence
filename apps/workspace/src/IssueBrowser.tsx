import { ActionIcon } from "./components/action-icon";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { t } from "./i18n";
import type { IssueLink } from "./contracts/setup";
import { ContextTarget } from "./components/context-menu";
import { copyContextAction, linkContextActions } from "./context-actions";
import { ExternalLink } from "./components/external-link";
import { Button } from "./components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "./components/ui/dialog";
import { Input } from "./components/ui/input";

export type IssueFilter = "all" | "unresolved" | "severe" | "fatal";
export function filterIssues(
  issues: IssueLink[],
  filter: IssueFilter,
  query = "",
) {
  const search = query.trim().toLowerCase();
  return issues.filter(
    (i) =>
      i.id.toLowerCase().includes(search) &&
      (filter === "all" ||
        (filter === "unresolved"
          ? !i.resolved
          : filter === "severe"
            ? i.severe && !i.resolved
            : i.fatal)),
  );
}
export function IssueBrowser({
  issues,
  open,
  onClose,
}: {
  issues: IssueLink[];
  open: IssueFilter | null;
  onClose: () => void;
}) {
  useTranslation();
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(100);
  const [filter, setFilter] = useState<IssueFilter | null>(null);
  const active = filter ?? open ?? "all";
  const shown = filterIssues(issues, active, query);
  return (
    <Dialog
      open={open !== null}
      onOpenChange={(value) => {
        if (!value) {
          setQuery("");
          setLimit(100);
          setFilter(null);
          onClose();
        }
      }}
    >
      <DialogContent className="issue-browser">
        <DialogTitle>{t("context.modelIssues")}</DialogTitle>
        <DialogDescription>{t("context.issueSnapshot")}</DialogDescription>
        <div className="issue-filters">
          {(["all", "unresolved", "severe", "fatal"] as const).map((f) => (
            <Button
              key={f}
              size="sm"
              variant={active === f ? "secondary" : "outline"}
              aria-pressed={active === f}
              onClick={() => {
                setFilter(f);
                setLimit(100);
              }}
            >
              <ActionIcon name={f} />
              {t(`context.issues.${f}`)}
            </Button>
          ))}
        </div>
        <Input
          aria-label={t("context.searchIssues")}
          placeholder={t("context.searchIssues")}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setLimit(100);
          }}
        />
        <p className="quiet">
          {t("context.issueCount", { count: shown.length })}
        </p>
        <ul className="issue-destinations">
          {shown.slice(0, limit).map((issue, index) => (
            <li key={`${issue.url}-${index}`}>
              <ExternalLink
                href={issue.url}
                actions={[
                  ...linkContextActions(issue.url),
                  copyContextAction(
                    "copy-issue",
                    t("context.copyIssueId"),
                    issue.id,
                  ),
                ]}
              >
                {issue.id}
              </ExternalLink>
              <span className="quiet">
                {t(
                  issue.resolved
                    ? "context.resolved"
                    : "context.issues.unresolved",
                )}
              </span>
            </li>
          ))}
        </ul>
        {shown.length > limit ? (
          <Button variant="outline" onClick={() => setLimit((n) => n + 100)}>
            <ActionIcon name="expand" />
            {t("context.moreIssues")}
          </Button>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
export function LinkedIssues({
  ids,
  issues,
}: {
  ids: string[];
  issues: IssueLink[];
}) {
  useTranslation();
  return (
    <>
      {ids.map((id, index) => {
        // Numeric "Bug #123" references can match a native Meegle record ID.
        const native = id.replace(/^(?:Bug\s*)?#\s*/i, "");
        const matches = issues.filter(
          (issue) => issue.id === native || issue.id === id,
        );
        const destinations = [...new Set(matches.map((i) => i.url))];
        const url = destinations.length === 1 ? destinations[0] : undefined;
        return (
          <span key={id}>
            {index ? " · " : ""}
            <ContextTarget
              title={id}
              actions={[
                ...linkContextActions(url),
                copyContextAction("copy-issue", t("context.copyIssueId"), id),
              ]}
            >
              <span>
                {url ? (
                  <ExternalLink href={url} contextMenu={false}>
                    {id}
                  </ExternalLink>
                ) : (
                  id
                )}
              </span>
            </ContextTarget>
          </span>
        );
      })}
    </>
  );
}
