import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { t } from "./i18n";
import { shortTime } from "./time-format";
import { failureText } from "./TemplateImport";

export type ReadSource = {
  label: string;
  updatedAt?: string | null;
  stale?: boolean;
  error?: string | null;
};

/** A cache read or local edit never establishes that a source was refreshed. */
export function sourceHealth(sources: ReadSource[] = []) {
  const failed = sources.filter((s) => s.stale || s.error);
  const succeeded = sources.filter(
    (s) => !s.stale && !s.error && validTime(s.updatedAt) !== null,
  );
  const times = sources.map((s) => validTime(s.updatedAt));
  return {
    outcome: failed.length
      ? succeeded.length
        ? "partial"
        : "failed"
      : sources.length && succeeded.length === sources.length
        ? "success"
        : "unknown",
    updatedAt:
      times.length && times.every((at) => at !== null)
        ? Math.min(...times.filter((at) => at !== null))
        : undefined,
    issues: failed.map((s) =>
      [
        s.label,
        s.stale && validTime(s.updatedAt) !== null
          ? t("work.staleSource") + (s.error || t("refresh.sourceUnavailable"))
          : s.error || t("refresh.sourceUnavailable"),
      ]
        .filter(Boolean)
        .join("："),
    ),
  } as const;
}

function validTime(value: string | null | undefined) {
  const at = Date.parse(value ?? "");
  return Number.isFinite(at) ? at : null;
}

/** Shared, persistent feedback for panels that retain their last source result. */
export function RefreshStatus({
  reading,
  error,
  hasData,
  sources,
  issues = [],
  label,
  onRefresh,
  scope,
}: {
  reading: boolean;
  error?: unknown;
  hasData: boolean;
  sources?: ReadSource[];
  issues?: string[];
  label: string;
  scope?: string;
  onRefresh: () => unknown;
}) {
  useTranslation();
  const health = sourceHealth(sources);
  const messages = [
    ...new Set([
      ...health.issues,
      ...issues,
      ...(error ? [failureText(error)] : []),
    ]),
  ];
  const outcome = error
    ? "failed"
    : messages.length && health.outcome === "success"
      ? "partial"
      : messages.length && health.outcome === "unknown"
        ? "failed"
        : health.outcome;
  const failed = outcome === "failed" || outcome === "partial";
  const text = reading
    ? t("refresh.reading")
    : outcome === "success"
      ? t("refresh.updated", { time: shortTime(health.updatedAt) })
      : outcome === "partial"
        ? t("refresh.partial")
        : outcome === "failed"
          ? t(hasData ? "refresh.failedCached" : "refresh.failed")
          : t("refresh.unconfirmed");
  return (
    <div className="refresh-feedback">
      <div className="refresh-control">
        <span
          className={failed && !reading ? "short" : "quiet"}
          role="status"
          aria-atomic="true"
        >
          {scope ? `${scope} · ` : ""}
          {text}
          {(reading || outcome !== "success") &&
          health.updatedAt !== undefined ? (
            <>
              {" "}
              ·{" "}
              {t("refresh.lastUpdated", { time: shortTime(health.updatedAt) })}
            </>
          ) : null}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          disabled={reading}
          aria-label={label}
          title={label}
          onClick={() => void onRefresh()}
        >
          {reading ? (
            <Spinner aria-hidden="true" />
          ) : (
            <RefreshCw aria-hidden="true" />
          )}
        </Button>
      </div>
      {failed && !reading ? (
        <div className="refresh-errors short" role="alert">
          {messages.map((message) => (
            <p key={message}>{message}</p>
          ))}
        </div>
      ) : null}
    </div>
  );
}
