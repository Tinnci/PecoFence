import type { Forecast, GateForecast } from "./contracts/setup";

const DAY = 86_400_000;
const WEEK = 7 * DAY;

export function limitingGate(forecast: Forecast): GateForecast | undefined {
  return (
    forecast.gates.find((g) => g.gate === forecast.limiting) ??
    forecast.gates.find((g) => g.gap > 0) ??
    forecast.gates[0]
  );
}

/** Whole weeks from a day count, rounding up: one day late is a week to find. */
export function lateWeeks(days: number) {
  return Math.ceil(Math.abs(days) / 7);
}

export type ForecastPlot = {
  from: number;
  to: number;
  top: number;
  history: { at: number; gap: number }[];
  likely: number | null;
  safe: number | null;
  freeze: number | null;
};

const day = (text: string | null) => {
  const at = text ? Date.parse(`${text}T00:00:00`) : NaN;
  return Number.isNaN(at) ? null : at;
};

/** What the chart draws: the gap so far, then the range it closes in, up to the freeze. */
export function forecastPlot(
  forecast: Forecast,
  gate: GateForecast,
  now = Date.now(),
): ForecastPlot {
  const last = gate.history.length - 1;
  const history = gate.history.map((gap, i) => ({
    at: now - (last - i) * WEEK,
    gap: Math.max(0, gap),
  }));
  const likely = day(gate.likely);
  const safe = day(gate.safe);
  const freeze = day(forecast.freeze);
  const ends = [now, likely, safe, freeze].filter((x) => x !== null);
  return {
    from: history[0]?.at ?? now,
    to: Math.max(...ends),
    top: Math.max(1, ...history.map((h) => h.gap)),
    history,
    likely,
    safe,
    freeze,
  };
}
