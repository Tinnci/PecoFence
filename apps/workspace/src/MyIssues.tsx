import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { focus } from "./api";
import { openContextAction } from "./context-actions";
import type { Mine, MyIssue } from "./contracts/setup";
import { focusKey, queryClient } from "./focus-cache";
import { t } from "./i18n";

const mineKey = [...focusKey, "mine"];
const SHOWN = 8;
const REFRESH_MS = 30 * 60_000;

/** The last read at once, then a fresh one; a source that fails keeps the other's items. */
function useMine() {
  const cached = useQuery(
    {
      queryKey: [...mineKey, "cached"],
      queryFn: async (): Promise<Mine | null> =>
        (await focus("mine_cached"))?.mine ?? null,
      staleTime: Infinity,
    },
    queryClient,
  );
  const fresh = useQuery(
    {
      queryKey: mineKey,
      queryFn: async (): Promise<Mine | null> =>
        (await focus("mine"))?.mine ?? null,
      staleTime: 5 * 60_000,
      refetchInterval: REFRESH_MS,
    },
    queryClient,
  );
  return { mine: fresh.data ?? cached.data, fresh };
}

function open(item: MyIssue, onModel: (code: string) => void) {
  const source = openContextAction(item.id, item.title, item.url);
  if (source) return void source.run();
  if (item.followed) onModel(item.model);
}

/** 我的问题: Jira and 飞书项目 items under the signed-in user, by urgency score. */
export function MyIssues({ onModel }: { onModel: (code: string) => void }) {
  useTranslation();
  const { mine, fresh } = useMine();
  const [all, setAll] = useState(false);
  const failed = mine?.sources.filter((s) => s.error) ?? [];
  const items = mine?.items ?? [];
  if (!items.length && !failed.length) return null;
  const shown = all ? items : items.slice(0, SHOWN);
  return (
    <section className="my-issues" aria-label={t("mine.title")}>
      <h2>
        {t("mine.title")}
        <span className="quiet"> {items.length}</span>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          disabled={fresh.isFetching}
          aria-label={t("mine.refresh")}
          title={t("mine.refresh")}
          onClick={() => void fresh.refetch()}
        >
          {fresh.isFetching ? (
            <Spinner aria-hidden="true" />
          ) : (
            <RefreshCw aria-hidden="true" />
          )}
        </Button>
      </h2>
      {failed.map((s) => (
        <p key={s.source} className="short" role="alert">
          {t("mine.failed", { source: t(`mine.source.${s.source}`) })}
        </p>
      ))}
      <ol>
        {shown.map((item) => (
          <li key={`${item.source}:${item.id}`}>
            <button
              type="button"
              className="my-issue"
              onClick={() => open(item, onModel)}
            >
              <span className="my-issue-score">{item.score}</span>
              <span className="my-issue-text">
                <strong>{item.title}</strong>
                <span className="quiet">
                  {[item.display_id, item.model, item.status]
                    .filter(Boolean)
                    .join(" · ")}
                </span>
              </span>
              <span className="my-issue-reasons">
                {item.reasons.slice(0, 2).map((r) => (
                  <span key={r.key} className="related-term">
                    {t(`mine.reason.${r.key}`)}
                  </span>
                ))}
              </span>
            </button>
          </li>
        ))}
      </ol>
      {items.length > SHOWN ? (
        <Button variant="ghost" size="sm" onClick={() => setAll(!all)}>
          {all ? t("mine.less") : t("mine.all", { count: items.length })}
        </Button>
      ) : null}
    </section>
  );
}
