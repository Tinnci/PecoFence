import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { SeverityBadge } from "./Badges";
import type { WorkItem } from "./contracts/setup";
import {
  boardColumns,
  FLOW_COLUMNS,
  statusAge,
  type FlowMetrics,
} from "./flow";
import { t } from "./i18n";

const SHOWN = 40;
type Limits = Record<string, number>;

function savedLimits(key: string): Limits {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "{}") as Limits;
  } catch {
    return {};
  }
}

/** Twelve weeks of work finished, with arrivals as a line over it. */
function FlowSpark({ flow }: { flow: FlowMetrics }) {
  const top = Math.max(1, ...flow.finished, ...flow.arrived);
  const y = (n: number) => 24 - (n / top) * 22;
  const line = flow.arrived
    .map((n, i) => `${i * 8 + 3.5},${y(n).toFixed(1)}`)
    .join(" ");
  return (
    <svg className="flow-spark" viewBox="0 0 96 24" aria-hidden="true">
      {flow.finished.map((n, i) => (
        <rect key={i} x={i * 8} y={y(n)} width="7" height={24 - y(n)} rx="1">
          <title>{t("flow.week", { done: n, arrived: flow.arrived[i] })}</title>
        </rect>
      ))}
      <polyline points={line} />
    </svg>
  );
}

/** The measures a Kanban team reads first. */
export function FlowStrip({ flow }: { flow: FlowMetrics }) {
  useTranslation();
  const days = (n: number | null) =>
    n === null ? "–" : t("flow.days", { count: n });
  const stats: [string, string, boolean?][] = [
    [t("flow.wip"), String(flow.wip)],
    [t("flow.blocked"), String(flow.blocked), flow.blocked > 0],
    [t("flow.aged"), String(flow.aged), flow.aged > 0],
    [t("flow.cycle"), days(flow.cycleP50)],
    [t("flow.p85"), days(flow.cycleP85)],
  ];
  return (
    <div className="flow-strip">
      {stats.map(([label, value, alert]) => (
        <span key={label}>
          <strong className={alert ? "short" : undefined}>{value}</strong>
          <small>{label}</small>
        </span>
      ))}
      <span className="flow-trend" title={t("flow.trend")}>
        <FlowSpark flow={flow} />
        <small>{t("flow.trend")}</small>
      </span>
    </div>
  );
}

/** Issues or requirements as columns of work; read-only, as sources own their status. */
export function WorkBoard({
  id,
  items,
  flow,
  now,
  onDetails,
}: {
  id: string;
  items: WorkItem[];
  flow: FlowMetrics;
  now: number;
  onDetails: (item: WorkItem) => void;
}) {
  useTranslation();
  const key = `tinnci.wip.${id}`;
  const [limits, setLimits] = useState(() => savedLimits(key));
  const columns = useMemo(() => boardColumns(items, now), [items, now]);
  function setLimit(column: string, text: string) {
    const value = Number(text);
    const next = { ...limits };
    if (value > 0) next[column] = value;
    else delete next[column];
    setLimits(next);
    try {
      localStorage.setItem(key, JSON.stringify(next));
    } catch {
      // The limit still holds for this session.
    }
  }
  return (
    <div className="work-board">
      {FLOW_COLUMNS.map((column) => {
        const list = columns.get(column) ?? [];
        const limit = limits[column];
        const over = limit !== undefined && list.length > limit;
        return (
          <section
            key={column}
            className={over ? "board-column is-over" : "board-column"}
            aria-label={t(`work.stage.${column}`)}
          >
            <header>
              <strong>{t(`work.stage.${column}`)}</strong>
              <span>{list.length}</span>
              <Input
                className="board-limit"
                inputMode="numeric"
                value={limit ?? ""}
                placeholder="∞"
                aria-label={t("flow.limit", {
                  column: t(`work.stage.${column}`),
                })}
                title={t("flow.limitHint")}
                onChange={(e) => setLimit(column, e.target.value)}
              />
            </header>
            <ol>
              {list.slice(0, SHOWN).map((item) => {
                const age = statusAge(item, now);
                const aged = (age ?? 0) > flow.agedAfter;
                return (
                  <li key={`${item.source}:${item.id}`}>
                    <button
                      type="button"
                      className="board-card"
                      onClick={() => onDetails(item)}
                    >
                      <span className="board-card-title">{item.title}</span>
                      <span className="board-card-foot">
                        <SeverityBadge value={item.severity} />
                        <span className="quiet">{item.display_id}</span>
                        {item.owner ? (
                          <span className="quiet">{item.owner}</span>
                        ) : null}
                        {age !== null ? (
                          <span className={aged ? "short" : "quiet"}>
                            {t("flow.days", { count: age })}
                          </span>
                        ) : null}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ol>
            {list.length > SHOWN ? (
              <span className="quiet board-more">
                {t("work.more", { count: list.length - SHOWN })}
              </span>
            ) : null}
          </section>
        );
      })}
    </div>
  );
}
