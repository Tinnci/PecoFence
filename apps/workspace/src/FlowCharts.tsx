import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useQuery } from "@tanstack/react-query";
import { focus } from "./api";
import { gerritSearchUrl, gerritTerm } from "./browser-links";
import { openContextAction } from "./context-actions";
import { focusKey, queryClient } from "./focus-cache";
import { Strip } from "./Focus";
import type { RepoRow, WorkItem } from "./contracts/setup";
import {
  burndown,
  createdResolved,
  currentIteration,
  FLOW_COLUMNS,
  flowMap,
  iterationStart,
  iterationWindow,
  oldestItems,
  statusAge,
  type FlowMetrics,
  type IterationDates,
} from "./flow";
import { t } from "./i18n";
import { IterationDialog } from "./IterationDialog";

const W = 520;
const H = 200;
const PAD = { top: 12, right: 12, bottom: 22, left: 34 };
const DAY = 86_400_000;
/** The fixed two-week rhythm, used until a named iteration holds today. */
const CADENCE = "\u0000cadence";
const shortDate = (at: number) => {
  const d = new Date(at);
  return `${d.getMonth() + 1}/${d.getDate()}`;
};
const niceTop = (n: number) => {
  const step = 10 ** Math.floor(Math.log10(Math.max(1, n)));
  return Math.ceil(Math.max(1, n) / step) * step;
};

type Series = {
  key: string;
  label: string;
  className: string;
  values: (number | null)[];
};

/** A one-axis line chart with a crosshair and a tooltip for the day under the pointer. */
function LineChart({
  title,
  dateOf,
  series,
  controls,
}: {
  title: string;
  dateOf: (index: number) => number;
  series: Series[];
  controls?: ReactNode;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const days = series[0].values.length;
  const top = niceTop(
    Math.max(...series.flatMap((s) => s.values.map((v) => v ?? 0))),
  );
  const x = (i: number) =>
    PAD.left + (i / (days - 1)) * (W - PAD.left - PAD.right);
  const y = (v: number) => PAD.top + (1 - v / top) * (H - PAD.top - PAD.bottom);
  const path = (values: (number | null)[]) =>
    values.flatMap((v, i) => (v === null ? [] : [`${x(i)},${y(v)}`])).join(" ");
  const ticks = [0, 0.5, 1].map((share) => Math.round(top * share));
  return (
    <figure className="flow-chart">
      <figcaption>
        <strong>{title}</strong>
        {controls}
        <span className="flow-legend">
          {series.map((s) => (
            <span key={s.key}>
              <i className={s.className} />
              {s.label}
            </span>
          ))}
        </span>
      </figcaption>
      <div className="flow-plot">
        <svg
          viewBox={`0 0 ${W} ${H}`}
          role="img"
          aria-label={title}
          onPointerLeave={() => setHover(null)}
          onPointerMove={(e) => {
            const box = e.currentTarget.getBoundingClientRect();
            const at = ((e.clientX - box.left) / box.width) * W;
            const share = (at - PAD.left) / (W - PAD.left - PAD.right);
            setHover(
              Math.max(0, Math.min(days - 1, Math.round(share * (days - 1)))),
            );
          }}
        >
          {ticks.map((tick) => (
            <g key={tick} className="flow-axis">
              <line
                x1={PAD.left}
                x2={W - PAD.right}
                y1={y(tick)}
                y2={y(tick)}
              />
              <text x={PAD.left - 6} y={y(tick) + 4} textAnchor="end">
                {tick}
              </text>
            </g>
          ))}
          {[0, days - 1].map((i) => (
            <text
              key={i}
              className="flow-axis-label"
              x={x(i)}
              y={H - 6}
              textAnchor={i ? "end" : "start"}
            >
              {shortDate(dateOf(i))}
            </text>
          ))}
          {series.map((s) => (
            <polyline
              key={s.key}
              className={s.className}
              points={path(s.values)}
            />
          ))}
          {hover !== null ? (
            <line
              className="flow-cross"
              x1={x(hover)}
              x2={x(hover)}
              y1={PAD.top}
              y2={H - PAD.bottom}
            />
          ) : null}
        </svg>
        {hover !== null ? (
          <div
            className="flow-tip"
            style={{ left: `${(x(hover) / W) * 100}%` }}
            role="status"
          >
            <strong>{shortDate(dateOf(hover))}</strong>
            {series.map((s) => (
              <span key={s.key}>
                {s.label} {s.values[hover] ?? "–"}
              </span>
            ))}
          </div>
        ) : null}
      </div>
    </figure>
  );
}

const MAP_W = 1040;
const MAP_H = 118;
const STAGE_W = 168;
const STAGE_H = 58;
/** Left-to-right order of the work; blocked hangs off it, as nothing passes through it. */
const ROUTE = ["pending", "active", "review", "testing"] as const;

/** The workflow as a phase map: what sits in each stage, one bottleneck marked. */
function FlowMap({
  items,
  flow,
  now,
}: {
  items: WorkItem[];
  flow: FlowMetrics;
  now: number;
}) {
  const { stages, bottleneck } = useMemo(
    () => flowMap(items, now),
    [items, now],
  );
  const gap = (MAP_W - STAGE_W * 5) / 4;
  const left = (i: number) => i * (STAGE_W + gap);
  const stage = (column: string) => stages.find((s) => s.column === column);
  const finished = flow.finished.reduce((sum, n) => sum + n, 0);
  const box = (
    key: string,
    x: number,
    y: number,
    label: string,
    value: number,
    note: string,
    focal: boolean,
  ) => (
    <g key={key} className={focal ? "map-stage is-focal" : "map-stage"}>
      <rect x={x} y={y} width={STAGE_W} height={STAGE_H} rx={6} />
      <text className="map-label" x={x + 12} y={y + 22}>
        {label}
      </text>
      <text
        className="map-value"
        x={x + STAGE_W - 12}
        y={y + 26}
        textAnchor="end"
      >
        {value}
      </text>
      <text className="map-note" x={x + 12} y={y + 44}>
        {note}
      </text>
    </g>
  );
  const age = (days: number | null | undefined) =>
    days === null || days === undefined
      ? ""
      : t("flow.typical", { count: days });
  const blockedX = left(2);
  const blocked = stage("blocked");
  return (
    <figure className="flow-chart flow-map">
      <figcaption>
        <strong>{t("flow.chart.map")}</strong>
        {bottleneck ? (
          <span className="flow-legend">
            {t("flow.bottleneck", {
              stage: t(`work.stage.${bottleneck.column}`),
              count: bottleneck.count,
            })}
          </span>
        ) : null}
      </figcaption>
      <svg
        viewBox={`0 0 ${MAP_W} ${MAP_H + 16}`}
        role="img"
        aria-label={t("flow.chart.map")}
      >
        <defs>
          <marker
            id="map-arrow"
            viewBox="0 0 8 8"
            refX="7"
            refY="4"
            markerWidth="7"
            markerHeight="7"
            orient="auto"
          >
            <path d="M0 0 8 4 0 8z" className="map-head" />
          </marker>
        </defs>
        {[0, 1, 2, 3].map((i) => (
          <line
            key={i}
            className="map-link"
            x1={left(i) + STAGE_W + 4}
            x2={left(i + 1) - 6}
            y1={STAGE_H / 2 + 1}
            y2={STAGE_H / 2 + 1}
            markerEnd="url(#map-arrow)"
          />
        ))}
        <line
          className="map-link is-side"
          x1={blockedX + STAGE_W / 2}
          x2={blockedX + STAGE_W / 2}
          y1={STAGE_H + 2}
          y2={MAP_H - STAGE_H + 12}
        />
        {ROUTE.map((column, i) =>
          box(
            column,
            left(i),
            1,
            t(`work.stage.${column}`),
            stage(column)?.count ?? 0,
            age(stage(column)?.medianAge),
            bottleneck?.column === column,
          ),
        )}
        {box(
          "done",
          left(4),
          1,
          t("work.stage.done"),
          finished,
          t("flow.twelveWeeks"),
          false,
        )}
        {box(
          "blocked",
          blockedX,
          MAP_H - STAGE_H + 15,
          t("work.stage.blocked"),
          blocked?.count ?? 0,
          age(blocked?.medianAge),
          bottleneck?.column === "blocked",
        )}
      </svg>
    </figure>
  );
}

/** Every open item by its column and days in status; the oldest are what to pull next. */
function AgingChart({
  items,
  flow,
  now,
  onDetails,
}: {
  items: WorkItem[];
  flow: FlowMetrics;
  now: number;
  onDetails: (item: WorkItem) => void;
}) {
  const points = useMemo(
    () =>
      items.flatMap((item) => {
        const column = FLOW_COLUMNS.indexOf(
          item.group as (typeof FLOW_COLUMNS)[number],
        );
        const age = statusAge(item, now);
        return column < 0 || age === null ? [] : [{ item, column, age }];
      }),
    [items, now],
  );
  const [hover, setHover] = useState<(typeof points)[number] | null>(null);
  const oldest = useMemo(
    () => new Set(oldestItems(items, now).map((item) => item.id)),
    [items, now],
  );
  const top = niceTop(Math.max(flow.agedAfter, ...points.map((p) => p.age)));
  const band = (W - PAD.left - PAD.right) / FLOW_COLUMNS.length;
  const y = (v: number) => PAD.top + (1 - v / top) * (H - PAD.top - PAD.bottom);
  const x = (p: (typeof points)[number], i: number) =>
    PAD.left +
    band * (p.column + 0.5) +
    (((i * 37) % 100) / 100 - 0.5) * band * 0.7;
  return (
    <figure className="flow-chart">
      <figcaption>
        <strong>{t("flow.chart.aging")}</strong>
        <span className="flow-legend">
          <span>{t("flow.agedLine", { count: flow.agedAfter })}</span>
        </span>
      </figcaption>
      <div className="flow-plot">
        <svg
          viewBox={`0 0 ${W} ${H}`}
          role="img"
          aria-label={t("flow.chart.aging")}
        >
          {[0, Math.round(top / 2), top].map((tick) => (
            <g key={tick} className="flow-axis">
              <line
                x1={PAD.left}
                x2={W - PAD.right}
                y1={y(tick)}
                y2={y(tick)}
              />
              <text x={PAD.left - 6} y={y(tick) + 4} textAnchor="end">
                {tick}
              </text>
            </g>
          ))}
          <line
            className="flow-limit"
            x1={PAD.left}
            x2={W - PAD.right}
            y1={y(flow.agedAfter)}
            y2={y(flow.agedAfter)}
          />
          {FLOW_COLUMNS.map((column, i) => (
            <text
              key={column}
              className="flow-axis-label"
              x={PAD.left + band * (i + 0.5)}
              y={H - 6}
              textAnchor="middle"
            >
              {t(`work.stage.${column}`)}
            </text>
          ))}
          {points.map((p, i) => (
            <circle
              key={`${p.item.source}:${p.item.id}`}
              className={
                oldest.has(p.item.id) ? "flow-dot is-focal" : "flow-dot"
              }
              cx={x(p, i)}
              cy={y(p.age)}
              r={4}
              onPointerEnter={() => setHover(p)}
              onPointerLeave={() => setHover(null)}
              onClick={() => onDetails(p.item)}
            />
          ))}
          {points.flatMap((p, i) =>
            oldest.has(p.item.id)
              ? [
                  <text
                    key={p.item.id}
                    className="flow-dot-label"
                    x={x(p, i) + 7}
                    y={y(p.age) + (i % 2 ? 11 : -3)}
                  >
                    {p.item.display_id}
                  </text>,
                ]
              : [],
          )}
        </svg>
        {hover ? (
          <div
            className="flow-tip"
            style={{
              left: `${(((hover.column + 0.5) * band + PAD.left) / W) * 100}%`,
            }}
            role="status"
          >
            <strong>{hover.item.display_id}</strong>
            <span>{t("flow.days", { count: hover.age })}</span>
            <span>{hover.item.title}</span>
          </div>
        ) : null}
      </div>
    </figure>
  );
}

const REPOS_SHOWN = 8;

/** One row per repository: what changed there, in review, and what needs a look. */
function RepoBreakdown({ code }: { code: string }) {
  const [all, setAll] = useState(false);
  const query = useQuery(
    {
      queryKey: [...focusKey, "model", code, "repos"],
      queryFn: async () => (await focus("repos", { code }))?.repos ?? [],
      staleTime: 60_000,
    },
    queryClient,
  );
  const rows = query.data ?? [];
  if (!rows.length) return null;
  const top = rows[0].changes;
  const open = (row: RepoRow) => {
    const url = gerritSearchUrl(
      row.sample_url,
      gerritTerm("project", row.project),
    );
    void openContextAction(row.project, row.project, url)?.run();
  };
  return (
    <figure className="flow-chart flow-repos">
      <figcaption>
        <strong>{t("flow.chart.repos", { count: rows.length })}</strong>
        <span className="flow-legend">{t("flow.repoHint")}</span>
      </figcaption>
      <div className="repo-head" aria-hidden="true">
        <span />
        <span>{t("flow.repo.changes")}</span>
        <span>{t("flow.repo.merged")}</span>
        <span>{t("flow.repo.open")}</span>
        <span>{t("flow.repo.authors")}</span>
        <span>{t("flow.repo.flagged")}</span>
        <span>{t("flow.repo.weeks")}</span>
      </div>
      <ol>
        {(all ? rows : rows.slice(0, REPOS_SHOWN)).map((row) => (
          <li key={row.project}>
            <button
              type="button"
              className="repo-row"
              title={row.project}
              onClick={() => open(row)}
            >
              <span className="repo-name">
                <span>{row.project}</span>
                <i style={{ width: `${(row.changes / top) * 100}%` }} />
              </span>
              <strong>{row.changes}</strong>
              <span>{row.merged}</span>
              <span>{row.open || ""}</span>
              <span>{row.authors}</span>
              <span className={row.flagged ? "short" : undefined}>
                {row.flagged || ""}
              </span>
              <Strip id={`repo:${code}:${row.project}`} days={row.days} />
            </button>
          </li>
        ))}
      </ol>
      {rows.length > REPOS_SHOWN ? (
        <Button variant="ghost" size="sm" onClick={() => setAll(!all)}>
          {all ? t("mine.less") : t("mine.all", { count: rows.length })}
        </Button>
      ) : null}
    </figure>
  );
}

/** Created against resolved, the current iteration's burndown, and aging open work. */
export function FlowCharts({
  code,
  items,
  flow,
  now,
  iterations,
  onIterations,
  onDetails,
}: {
  code: string;
  items: WorkItem[];
  flow: FlowMetrics;
  now: number;
  iterations: IterationDates[];
  /** Saves the list; absent when the model cannot be edited from here. */
  onIterations?: (next: IterationDates[]) => Promise<void>;
  onDetails: (item: WorkItem) => void;
}) {
  useTranslation();
  const trend = useMemo(() => createdResolved(items, now), [items, now]);
  const [picked, setPicked] = useState<string | null>(null);
  const [dialog, setDialog] = useState<"add" | "edit" | null>(null);
  const chosen =
    iterations.find((i) => i.name === picked) ??
    (picked === CADENCE ? undefined : currentIteration(iterations, now));
  const { start, days } = chosen
    ? iterationWindow(chosen)
    : { start: iterationStart(now), days: 14 };
  const burn = useMemo(
    () => burndown(items, start, days, now),
    [items, start, days, now],
  );
  const options = [
    { value: CADENCE, label: t("iteration.cadence") },
    ...iterations.map((i) => ({ value: i.name, label: i.name })),
  ];
  const others = iterations.filter(
    (i) => dialog !== "edit" || i.name !== chosen?.name,
  );
  return (
    <div className="flow-charts">
      <FlowMap items={items} flow={flow} now={now} />
      <LineChart
        title={t("flow.chart.createdResolved")}
        dateOf={(i) => trend.start + i * DAY}
        series={[
          {
            key: "created",
            label: t("flow.created"),
            className: "flow-line-a",
            values: trend.created,
          },
          {
            key: "resolved",
            label: t("flow.resolved"),
            className: "flow-line-b",
            values: trend.resolved,
          },
        ]}
      />
      <LineChart
        title={t("flow.chart.burndown")}
        dateOf={(i) => start + Math.max(0, i - 1) * DAY}
        controls={
          <span className="iteration-controls">
            <Select
              value={chosen?.name ?? CADENCE}
              items={options}
              onValueChange={(value) => setPicked(value as string)}
            >
              <SelectTrigger size="sm" aria-label={t("iteration.pick")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {options.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {onIterations ? (
              <>
                {chosen ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    aria-label={t("iteration.edit")}
                    title={t("iteration.edit")}
                    onClick={() => setDialog("edit")}
                  >
                    <Pencil aria-hidden="true" />
                  </Button>
                ) : null}
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("iteration.add")}
                  title={t("iteration.add")}
                  onClick={() => setDialog("add")}
                >
                  <Plus aria-hidden="true" />
                </Button>
              </>
            ) : null}
          </span>
        }
        series={[
          {
            key: "scope",
            label: t("flow.scope"),
            className: "flow-line-b",
            values: burn.scope,
          },
          {
            key: "ideal",
            label: t("flow.ideal"),
            className: "flow-line-ideal",
            values: burn.ideal,
          },
          {
            key: "remaining",
            label: t("flow.remaining"),
            className: "flow-line-a",
            values: burn.remaining,
          },
        ]}
      />
      <AgingChart items={items} flow={flow} now={now} onDetails={onDetails} />
      <RepoBreakdown code={code} />
      {dialog && onIterations ? (
        <IterationDialog
          editing={dialog === "edit" ? chosen : undefined}
          others={others}
          onClose={() => setDialog(null)}
          onSave={async (iteration) => {
            await onIterations(
              [...others, ...(iteration ? [iteration] : [])].sort((a, b) =>
                a.start.localeCompare(b.start),
              ),
            );
            setPicked(iteration?.name ?? null);
          }}
        />
      ) : null}
    </div>
  );
}
