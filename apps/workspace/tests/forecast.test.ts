import { describe, expect, it } from "vitest";
import { hardNeeds } from "../src/NeedsYou";
import type { Forecast, GateForecast } from "../src/contracts/setup";
import { forecastPlot, lateWeeks, limitingGate } from "../src/forecast";

const gate = (name: string, gap: number, history: number[]): GateForecast => ({
  gate: name,
  target_pct: 95,
  gap,
  resolved_per_week: 5,
  arrived_per_week: 1,
  state: gap ? "on_pace" : "met",
  likely: gap ? "2026-12-04" : null,
  safe: gap ? "2026-12-18" : null,
  history,
});
const forecast: Forecast = {
  code: "M",
  freeze: "2026-12-01",
  state: "on_pace",
  limiting: "overall",
  likely: "2026-12-04",
  safe: "2026-12-18",
  late_days: 3,
  gates: [gate("severe", 0, [0]), gate("overall", 20, [30, -2, 20])],
};

describe("forecast", () => {
  it("picks the gate that finishes last", () => {
    expect(limitingGate(forecast)?.gate).toBe("overall");
    expect(limitingGate({ ...forecast, limiting: "" })?.gate).toBe("overall");
  });
  it("counts part of a week as a week", () => {
    expect(lateWeeks(1)).toBe(1);
    expect(lateWeeks(14)).toBe(2);
  });
  it("runs the plot to the latest date and keeps a met gap at zero", () => {
    const now = Date.parse("2026-11-06T00:00:00");
    const plot = forecastPlot(forecast, forecast.gates[1], now);
    expect(plot.to).toBe(Date.parse("2026-12-18T00:00:00"));
    expect(plot.history.map((p) => p.gap)).toEqual([30, 0, 20]);
    expect(plot.history[2].at).toBe(now);
    expect(plot.top).toBe(30);
    const noDates = { ...forecast.gates[1], likely: null, safe: null };
    const open = forecastPlot({ ...forecast, freeze: null }, noDates, now);
    expect(open.to).toBe(now);
  });
});

describe("needs badge", () => {
  it("counts what is held up, not what is only quiet", () => {
    const items = ["review", "stale", "build_failed", "stale"].map((kind) => ({
      kind,
      title: kind,
      model: "",
      id: "",
      url: "",
      since: "",
    }));
    expect(hardNeeds(items).map((i) => i.kind)).toEqual([
      "review",
      "build_failed",
    ]);
  });
});
