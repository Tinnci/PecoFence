import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Spinner } from "@/components/ui/spinner";
import { Skeleton } from "@/components/ui/skeleton";
import type { WorkItem } from "./contracts/setup";
import { queryClient } from "./focus-cache";
import {
  sourceFieldsOptions,
  refreshSourceFieldsItem,
} from "./source-fields-cache";
import { FailureNote, failureOf } from "./TemplateImport";
import { Linked } from "./ChangeDiff";
import { readableText } from "./work-text";
import { t } from "./i18n";

export function SourceFields({ code, item }: { code: string; item: WorkItem }) {
  useTranslation();
  const query = useQuery(
    { ...sourceFieldsOptions(code, item), enabled: item.source === "meegle" },
    queryClient,
  );
  const [showEmpty, setShowEmpty] = useState(false);
  const fields = query.data?.fields ?? [];
  const empty = fields.filter((field) => !field.value.trim()).length;
  const visible = fields.filter((field) => showEmpty || field.value.trim());
  return (
    <section aria-label={t("work.sourceFields")}>
      <div className="work-source-head">
        <h3>{t("work.sourceFields")}</h3>
        <button
          type="button"
          className="link"
          disabled={query.isFetching}
          onClick={() =>
            void refreshSourceFieldsItem(code, item).catch(() => undefined)
          }
        >
          {query.isFetching ? (
            <Spinner className="size-3" />
          ) : (
            <RefreshCw size={12} aria-hidden="true" />
          )}
          {t("work.refresh")}
        </button>
      </div>
      {query.error ? <FailureNote failure={failureOf(query.error)} /> : null}
      {!query.data && query.isFetching ? (
        <div className="diff-loading" aria-busy="true">
          {[0, 1, 2].map((n) => (
            <Skeleton key={n} className="diff-loading-line" />
          ))}
        </div>
      ) : null}
      {query.data ? (
        <>
          {visible.length ? (
            <dl className="work-source-fields">
              {visible.map((field) => (
                <div key={field.key}>
                  <dt>{field.name}</dt>
                  <dd className="work-detail-text">
                    {!field.available ? (
                      <span className="quiet">
                        {t("work.sourceNotReturned")}
                      </span>
                    ) : !field.value.trim() ? (
                      <span className="quiet">{t("work.sourceEmpty")}</span>
                    ) : (
                      <Linked
                        text={
                          field.value === "true"
                            ? t("work.sourceYes")
                            : field.value === "false"
                              ? t("work.sourceNo")
                              : readableText(field.value)
                        }
                      />
                    )}
                  </dd>
                </div>
              ))}
            </dl>
          ) : (
            <p className="quiet">{t("work.sourceNoValues")}</p>
          )}
          {empty ? (
            <button
              type="button"
              className="link"
              aria-expanded={showEmpty}
              onClick={() => setShowEmpty(!showEmpty)}
            >
              {t(showEmpty ? "work.sourceHideEmpty" : "work.sourceShowEmpty", {
                count: empty,
              })}
            </button>
          ) : null}
          <p className="quiet work-source-read">
            {t("work.sourceReadAt", {
              time: new Date(query.data.fetched_at).toLocaleString(),
            })}
          </p>
        </>
      ) : null}
    </section>
  );
}
