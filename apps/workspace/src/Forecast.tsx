import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { focus } from "./api";
import type { Forecast } from "./contracts/setup";
import { forecastPlot, lateWeeks, limitingGate } from "./forecast";
import { focusKey, queryClient } from "./focus-cache";
import { t } from "./i18n";

const W = 640;
const H = 170;
const PAD = { top: 16, right: 12, bottom: 22, left: 34 };

export function useForecasts(version?: string | null) {
  return useQuery(
    {
      queryKey: [...focusKey, "forecasts", version ?? ""],
      queryFn: async (): Promise<Forecast[]> =>
        (await focus("forecast"))?.forecasts ?? [],
      staleTime: 5 * 60_000,
    },
    queryClient,
  ).data;
}

const monthDay = (at: number) => {
  const d = new Date(at);
  return `${d.getMonth() + 1}/${d.getDate()}`;
};
const shortDay = (text: string) => monthDay(Date.parse(`${text}T00:00:00`));

/** The forecast as one line: the date, and what it means against the freeze. */
export function forecastLine(f: Forecast) {
  if (f.state === "met") return t("forecast.met");
  if (f.state === "not_converging") return t("forecast.notConverging");
  if (f.state === "too_little_data") return t("forecast.tooLittle");
  if (!f.likely) return "";
  const date = shortDay(f.likely);
  if (f.late_days === null) return t("forecast.likely", { date });
  return f.late_days > 0
    ? t("forecast.late", { date, weeks: lateWeeks(f.late_days) })
    : t("forecast.early", { date });
}

function Plot({ forecast }: { forecast: Forecast }) {
  const [hover, setHover] = useState<number | null>(null);
  const gate = limitingGate(forecast);
  const plot = useMemo(
    () => (gate ? forecastPlot(forecast, gate) : null),
    [forecast, gate],
  );
  if (!gate || !plot) return null;
  const x = (at: number) =>
    PAD.left +
    ((at - plot.from) / Math.max(1, plot.to - plot.from)) *
      (W - PAD.left - PAD.right);
  const y = (gap: number) =>
    PAD.top + (1 - gap / plot.top) * (H - PAD.top - PAD.bottom);
  const line = plot.history
    .map((p, i) => `${i ? "L" : "M"}${x(p.at)} ${y(p.gap)}`)
    .join(" ");
  const now = plot.history[plot.history.length - 1];
  const shown = hover === null ? null : plot.history[hover];
  return (
    <figure className="forecast-plot">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label={t("forecast.chart", { gap: gate.gap })}
        onMouseLeave={() => setHover(null)}
        onMouseMove={(e) => {
          const box = e.currentTarget.getBoundingClientRect();
          const at = ((e.clientX - box.left) / box.width) * W;
          let best = 0;
          plot.history.forEach((p, i) => {
            if (
              Math.abs(x(p.at) - at) < Math.abs(x(plot.history[best].at) - at)
            )
              best = i;
          });
          setHover(best);
        }}
      >
        <line
          className="chart-grid"
          x1={PAD.left}
          x2={W - PAD.right}
          y1={y(0)}
          y2={y(0)}
        />
        <text
          className="chart-axis"
          x={PAD.left - 6}
          y={y(plot.top) + 4}
          textAnchor="end"
        >
          {plot.top}
        </text>
        <text
          className="chart-axis"
          x={PAD.left - 6}
          y={y(0) + 4}
          textAnchor="end"
        >
          0
        </text>
        <text className="chart-axis" x={PAD.left} y={H - 6}>
          {monthDay(plot.from)}
        </text>
        {plot.likely && plot.safe ? (
          <polygon
            className="forecast-range"
            points={`${x(now.at)},${y(now.gap)} ${x(plot.likely)},${y(0)} ${x(plot.safe)},${y(0)}`}
          />
        ) : null}
        {plot.likely ? (
          <line
            className="forecast-pace"
            x1={x(now.at)}
            y1={y(now.gap)}
            x2={x(plot.likely)}
            y2={y(0)}
          />
        ) : null}
        <path className="forecast-gap" d={line} />
        <circle className="forecast-now" cx={x(now.at)} cy={y(now.gap)} r={4} />
        {plot.freeze ? (
          <g className="forecast-freeze">
            <line
              x1={x(plot.freeze)}
              x2={x(plot.freeze)}
              y1={PAD.top - 8}
              y2={y(0)}
            />
            <text x={x(plot.freeze)} y={H - 6} textAnchor="middle">
              {t("forecast.freeze")} {monthDay(plot.freeze)}
            </text>
          </g>
        ) : null}
        {plot.likely ? (
          <text
            className="chart-axis"
            x={x(plot.likely)}
            y={y(0) - 8}
            textAnchor="end"
          >
            {monthDay(plot.likely)}
          </text>
        ) : null}
        {shown ? (
          <line
            className="chart-grid"
            x1={x(shown.at)}
            x2={x(shown.at)}
            y1={PAD.top}
            y2={y(0)}
          />
        ) : null}
      </svg>
      {shown ? (
        <figcaption className="quiet">
          {monthDay(shown.at)} · {t("forecast.gapThen", { gap: shown.gap })}
        </figcaption>
      ) : null}
    </figure>
  );
}

/** When this model reaches its gates at its recent pace, and whether that beats the freeze. */
export function ModelForecast({
  forecast,
  onSettings,
}: {
  forecast: Forecast | undefined;
  onSettings: () => void;
}) {
  useTranslation();
  if (!forecast || forecast.state === "no_data") return null;
  return (
    <section className="forecast" aria-label={t("forecast.title")}>
      <header>
        <h2>{t("forecast.title")}</h2>
        <strong
          className={
            (forecast.late_days ?? 0) > 0 || forecast.state === "not_converging"
              ? "short"
              : undefined
          }
        >
          {forecastLine(forecast)}
        </strong>
        {!forecast.freeze && forecast.state !== "met" ? (
          <Button variant="outline" size="xs" onClick={onSettings}>
            {t("forecast.setFreeze")}
          </Button>
        ) : null}
      </header>
      {forecast.state === "met" ? null : (
        <>
          <dl className="forecast-gates">
            {forecast.gates.map((g) => (
              <div key={g.gate}>
                <dt>{t(`forecast.gate.${g.gate}`)}</dt>
                <dd>
                  {g.gap
                    ? t("forecast.gap", { gap: g.gap })
                    : t("forecast.reached")}
                </dd>
                <dd className="quiet">
                  {t("forecast.pace", {
                    resolved: g.resolved_per_week.toFixed(1),
                    arrived: g.arrived_per_week.toFixed(1),
                  })}
                </dd>
                <dd className="quiet">
                  {g.likely && g.safe
                    ? t("forecast.range", {
                        likely: shortDay(g.likely),
                        safe: shortDay(g.safe),
                      })
                    : null}
                </dd>
              </div>
            ))}
          </dl>
          <Plot forecast={forecast} />
        </>
      )}
    </section>
  );
}
