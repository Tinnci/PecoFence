import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { focus } from "./api";
import { queryClient } from "./focus-cache";
import { FailureNote, failureOf } from "./TemplateImport";
import { t } from "./i18n";
import { shortTime } from "./time-format";
import { TracesPanel } from "./Traces";

const levels = ["problems", "all"] as const;

/** What every tool did: one list of events, or a live waterfall per action. */
export function LogsPage({
  view,
  onView,
}: {
  view: "logs" | "traces";
  onView: (view: "logs" | "traces") => void;
}) {
  useTranslation();
  const [search, setSearch] = useState("");
  return (
    <section className="logs-page" aria-label={t(`nav.${view}`)}>
      {view === "traces" ? (
        <TracesPanel
          search={search}
          onSearch={setSearch}
          onShowLog={(runId) => {
            setSearch(runId);
            onView("logs");
          }}
        />
      ) : (
        <EventLog
          search={search}
          onSearch={setSearch}
          onShowRun={(runId) => {
            setSearch(runId);
            onView("traces");
          }}
        />
      )}
    </section>
  );
}

function EventLog({
  search,
  onSearch,
  onShowRun,
}: {
  search: string;
  onSearch: (next: string) => void;
  onShowRun: (runId: string) => void;
}) {
  const [app, setApp] = useState("all");
  // A search for one run is asked to see all of it.
  const [level, setLevel] = useState<(typeof levels)[number]>(
    search ? "all" : "problems",
  );
  const query = useQuery(
    {
      queryKey: ["logs"],
      queryFn: async () => (await focus("logs")).logs ?? null,
      staleTime: 30_000,
    },
    queryClient,
  );
  const log = query.data;
  const entries = useMemo(() => {
    const words = search.trim().toLowerCase();
    return (log?.entries ?? []).filter(
      (e) =>
        (app === "all" || e.app === app) &&
        (level === "all" || e.level === "warn" || e.level === "error") &&
        (!words ||
          `${e.app} ${e.event} ${e.detail} ${e.run_id}`
            .toLowerCase()
            .includes(words)),
    );
  }, [log, app, level, search]);
  const apps = Object.entries(log?.apps ?? {});
  return (
    <>
      <div className="logs-toolbar">
        <ToggleGroup
          variant="outline"
          size="sm"
          className="chips"
          value={[app]}
          onValueChange={(next: string[]) => {
            if (next[0]) setApp(next[0]);
          }}
          aria-label={t("logs.app")}
        >
          <ToggleGroupItem value="all">{t("logs.allApps")}</ToggleGroupItem>
          {apps.map(([name, count]) => (
            <ToggleGroupItem key={name} value={name}>
              {name}
              {count?.problems ? (
                <strong className="logs-problems">{count.problems}</strong>
              ) : null}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <ToggleGroup
          variant="outline"
          size="sm"
          className="chips"
          value={[level]}
          onValueChange={(next: string[]) => {
            if (next[0]) setLevel(next[0] as (typeof levels)[number]);
          }}
          aria-label={t("logs.level")}
        >
          {levels.map((l) => (
            <ToggleGroupItem key={l} value={l}>
              {t(`logs.${l}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <Input
          className="logs-search"
          value={search}
          maxLength={200}
          placeholder={t("logs.search")}
          aria-label={t("logs.search")}
          onChange={(e) => onSearch(e.target.value)}
        />
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          disabled={query.isFetching}
          aria-label={t("logs.refresh")}
          title={t("logs.refresh")}
          onClick={() => void query.refetch()}
        >
          {query.isFetching ? (
            <Spinner aria-hidden="true" />
          ) : (
            <RefreshCw aria-hidden="true" />
          )}
        </Button>
      </div>
      {query.error ? <FailureNote failure={failureOf(query.error)} /> : null}
      {!log && !query.error ? <Skeleton className="diff-loading-line" /> : null}
      {log && !entries.length ? (
        <p className="quiet">{t("logs.empty")}</p>
      ) : null}
      {entries.length ? (
        <ol className="logs-list">
          {entries.map((e, i) => (
            <li key={`${e.ts}-${e.app}-${i}`} className={`log-${e.level}`}>
              <time dateTime={e.ts}>{shortTime(e.ts)}</time>
              <span className="logs-app">{e.app}</span>
              <span className="logs-event">{e.event}</span>
              <span className="logs-detail">
                {e.detail}
                {e.run_id ? (
                  <button
                    type="button"
                    className="logs-run"
                    title={t("logs.showRun")}
                    onClick={() => onShowRun(e.run_id)}
                  >
                    {e.run_id.slice(-8)}
                  </button>
                ) : null}
              </span>
            </li>
          ))}
        </ol>
      ) : null}
      {log?.truncated ? <p className="quiet">{t("logs.truncated")}</p> : null}
    </>
  );
}
