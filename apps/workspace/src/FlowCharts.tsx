import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { WorkItem } from "./contracts/setup";
import {
  burndown,
  createdResolved,
  FLOW_COLUMNS,
  iterationStart,
  statusAge,
  type FlowMetrics,
} from "./flow";
import { t } from "./i18n";

const W = 520;
const H = 200;
const PAD = { top: 12, right: 12, bottom: 22, left: 34 };
const DAY = 86_400_000;
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
  start,
  series,
}: {
  title: string;
  start: number;
  series: Series[];
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
              {shortDate(start + i * DAY)}
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
            <strong>{shortDate(start + hover * DAY)}</strong>
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
                p.age > flow.agedAfter ? "flow-dot is-aged" : "flow-dot"
              }
              cx={x(p, i)}
              cy={y(p.age)}
              r={4}
              onPointerEnter={() => setHover(p)}
              onPointerLeave={() => setHover(null)}
              onClick={() => onDetails(p.item)}
            />
          ))}
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

/** Created against resolved, the current iteration's burndown, and aging open work. */
export function FlowCharts({
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
  useTranslation();
  const trend = useMemo(() => createdResolved(items, now), [items, now]);
  const start = iterationStart(now);
  const burn = useMemo(
    () => burndown(items, start, 14, now),
    [items, start, now],
  );
  return (
    <div className="flow-charts">
      <LineChart
        title={t("flow.chart.createdResolved")}
        start={trend.start}
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
        start={start}
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
    </div>
  );
}
