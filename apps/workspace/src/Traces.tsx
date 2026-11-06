import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ChevronRight, CircleX, Radio, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Toggle } from "@/components/ui/toggle";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { Trace, TraceSpan } from "./contracts/setup";
import { focus } from "./api";
import { queryClient } from "./focus-cache";
import { FailureNote, failureOf } from "./TemplateImport";
import { t } from "./i18n";
import { shortTime } from "./time-format";
import {
  duration,
  hasChildren,
  slowest,
  spanWords,
  ticks,
  visibleRows,
} from "./trace";

const FAST_MS = 1000;
const IDLE_MS = 3000;
const filters = ["all", "failed"] as const;

function traceMatches(trace: Trace, words: string) {
  return (
    !words ||
    trace.run_id.toLowerCase().includes(words) ||
    trace.name.toLowerCase().includes(words) ||
    trace.spans.some((s) => spanWords(s).includes(words))
  );
}

/** Every action on one timeline, live: where its time went and what it waited for. */
export function TracesPanel({
  search,
  onSearch,
  onShowLog,
}: {
  search: string;
  onSearch: (next: string) => void;
  onShowLog: (runId: string) => void;
}) {
  useTranslation();
  const [live, setLive] = useState(true);
  const [filter, setFilter] = useState<(typeof filters)[number]>("all");
  const [pinned, setPinned] = useState<string | null>(null);
  const query = useQuery(
    {
      queryKey: ["traces"],
      queryFn: async () => (await focus("traces")).traces ?? null,
      refetchInterval: (q) =>
        !live
          ? false
          : q.state.data?.traces.some((x) => x.running)
            ? FAST_MS
            : IDLE_MS,
    },
    queryClient,
  );
  const words = search.trim().toLowerCase();
  const traces = useMemo(
    () =>
      (query.data?.traces ?? []).filter(
        (x) => traceMatches(x, words) && (filter === "all" || x.failed > 0),
      ),
    [query.data, words, filter],
  );
  // Live follows what is running until a trace is picked.
  const selected =
    traces.find((x) => x.id === pinned) ??
    (live ? traces.find((x) => x.running) : undefined) ??
    traces[0];
  const running = Boolean(selected?.running);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    const timer = window.setInterval(() => setNow(Date.now()), 100);
    return () => window.clearInterval(timer);
  }, [running]);
  const grown = running && live ? Math.max(0, now - query.dataUpdatedAt) : 0;
  const longest = Math.max(1, ...traces.map((x) => x.ms));
  return (
    <>
      <div className="logs-toolbar">
        <ToggleGroup
          variant="outline"
          size="sm"
          className="chips"
          value={[filter]}
          onValueChange={(next: string[]) => {
            if (next[0]) setFilter(next[0] as (typeof filters)[number]);
          }}
          aria-label={t("traces.filter")}
        >
          {filters.map((f) => (
            <ToggleGroupItem key={f} value={f}>
              {t(`traces.${f}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <Input
          className="logs-search"
          value={search}
          maxLength={200}
          placeholder={t("traces.search")}
          aria-label={t("traces.search")}
          onChange={(e) => onSearch(e.target.value)}
        />
        <Toggle
          variant="outline"
          size="sm"
          pressed={live}
          onPressedChange={(next) => {
            setLive(next);
            if (next) setPinned(null);
          }}
        >
          <Radio aria-hidden="true" className={live ? "trace-live" : ""} />
          {t("traces.live")}
        </Toggle>
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
      {!query.data && !query.error ? (
        <Skeleton className="diff-loading-line" />
      ) : null}
      {query.data && !traces.length ? (
        <p className="quiet">{t("traces.empty")}</p>
      ) : null}
      {selected ? (
        <div className="trace-split">
          <ol className="trace-list" aria-label={t("traces.list")}>
            {traces.map((x) => (
              <li key={x.id}>
                <button
                  type="button"
                  aria-current={x.id === selected.id}
                  onClick={() => setPinned(x.id)}
                >
                  <span className="trace-list-name">
                    {x.running ? <Spinner aria-hidden="true" /> : null}
                    {x.failed ? (
                      <CircleX aria-hidden="true" className="trace-failed" />
                    ) : null}
                    {x.name}
                  </span>
                  <span className="trace-list-ms">{duration(x.ms)}</span>
                  <time dateTime={x.started}>{shortTime(x.started)}</time>
                  <span className="trace-list-app">{x.app}</span>
                  <span
                    className="trace-list-bar"
                    style={{ width: `${(x.ms / longest) * 100}%` }}
                  />
                </button>
              </li>
            ))}
          </ol>
          <Waterfall
            key={selected.id}
            trace={selected}
            grown={grown}
            words={words}
            onShowLog={onShowLog}
          />
        </div>
      ) : null}
      {query.data?.truncated ? (
        <p className="quiet">{t("traces.truncated")}</p>
      ) : null}
    </>
  );
}

function spanLength(span: TraceSpan, grown: number) {
  return span.status === "running" ? span.ms + grown : span.ms;
}

function Waterfall({
  trace,
  grown,
  words,
  onShowLog,
}: {
  trace: Trace;
  grown: number;
  words: string;
  onShowLog: (runId: string) => void;
}) {
  const [folded, setFolded] = useState<ReadonlySet<number>>(new Set());
  const [picked, setPicked] = useState<number | null>(null);
  const total = Math.max(1, trace.ms + (trace.running ? grown : 0));
  const worst = useMemo(() => slowest(trace.spans), [trace.spans]);
  const place = (from: number, length: number) => ({
    left: `${(from / total) * 100}%`,
    width: `${(length / total) * 100}%`,
  });
  const anyMatch =
    words && trace.spans.some((s) => spanWords(s).includes(words));
  const detail = picked === null ? undefined : trace.spans[picked];
  return (
    <section className="trace-view" aria-label={trace.name}>
      <header className="trace-head">
        <strong>{trace.name}</strong>
        <span>{duration(total)}</span>
        <time dateTime={trace.started}>{shortTime(trace.started)}</time>
        <Button
          type="button"
          variant="ghost"
          size="xs"
          onClick={() => onShowLog(trace.run_id)}
        >
          {t("traces.showLog")}
        </Button>
      </header>
      <div className="trace-ruler" aria-hidden="true">
        <span />
        <div>
          {ticks(total).map((at) => (
            <span key={at} style={{ left: `${(at / total) * 100}%` }}>
              {duration(at)}
            </span>
          ))}
        </div>
      </div>
      <ol className="trace-rows">
        {visibleRows(trace.spans, folded).map((at) => {
          const span = trace.spans[at];
          const length = spanLength(span, trace.running ? grown : 0);
          const opens = hasChildren(trace.spans, at);
          const isFolded = folded.has(at);
          const past = (span.offset_ms + length) / total > 0.82;
          const dim = anyMatch && !spanWords(span).includes(words);
          return (
            <li
              key={at}
              className={`trace-row is-${span.status}${dim ? " is-dim" : ""}${at === picked ? " is-picked" : ""}`}
            >
              <div
                className="trace-label"
                style={{ paddingLeft: span.depth * 14 }}
              >
                {opens ? (
                  <button
                    type="button"
                    className="trace-fold"
                    aria-expanded={!isFolded}
                    aria-label={t(isFolded ? "traces.unfold" : "traces.fold", {
                      name: span.name,
                    })}
                    onClick={() => {
                      const next = new Set(folded);
                      if (isFolded) next.delete(at);
                      else next.add(at);
                      setFolded(next);
                    }}
                  >
                    <ChevronRight aria-hidden="true" />
                  </button>
                ) : (
                  <span className="trace-fold" />
                )}
                <button
                  type="button"
                  className="trace-name"
                  onClick={() => setPicked(at === picked ? null : at)}
                >
                  {span.status === "failed" ? (
                    <CircleX aria-hidden="true" className="trace-failed" />
                  ) : null}
                  {span.name}
                  {span.verb ? <em>{span.verb}</em> : null}
                </button>
              </div>
              <div className="trace-track">
                <span
                  className="trace-bar"
                  style={place(span.offset_ms, length)}
                />
                {span.critical.map(([from, to]) => (
                  <span
                    key={from}
                    className="trace-critical"
                    style={place(from, to - from)}
                  />
                ))}
                <span
                  className={`trace-ms${at === worst ? " is-worst" : ""}`}
                  style={
                    past
                      ? {
                          right: `${100 - (span.offset_ms / total) * 100}%`,
                        }
                      : {
                          left: `${((span.offset_ms + length) / total) * 100}%`,
                        }
                  }
                >
                  {duration(length)}
                </span>
              </div>
            </li>
          );
        })}
      </ol>
      {detail ? (
        <dl className="trace-detail">
          <div>
            <dt>{t("traces.tool")}</dt>
            <dd>{detail.app}</dd>
          </div>
          <div>
            <dt>{t("traces.offset")}</dt>
            <dd>+{duration(detail.offset_ms)}</dd>
          </div>
          <div>
            <dt>{t("traces.self")}</dt>
            <dd>{duration(detail.self_ms)}</dd>
          </div>
          <div>
            <dt>{t("traces.status")}</dt>
            <dd>
              {t(`traces.state.${detail.status}`)}
              {detail.code ? ` · ${detail.code}` : ""}
            </dd>
          </div>
        </dl>
      ) : null}
    </section>
  );
}
