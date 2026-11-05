// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { heatDate, heatDelay, useHeatSweep } from "../src/heat";
import { dismissSplash } from "../src/splash";

describe("heatmap sweep", () => {
  it("runs left to right and takes the same time at any size", () => {
    expect(heatDelay(0, 0, 12)).toBe(0);
    expect(heatDelay(1, 0, 12)).toBeLessThan(heatDelay(2, 0, 12));
    expect(heatDelay(11, 6, 12)).toBeLessThanOrEqual(480);
    expect(heatDelay(83, 0, 84)).toBeLessThanOrEqual(480);
  });

  it("plays once a session for each heatmap", () => {
    expect(renderHook(() => useHeatSweep("a")).result.current).toBe(true);
    expect(renderHook(() => useHeatSweep("a")).result.current).toBe(false);
    expect(renderHook(() => useHeatSweep("b")).result.current).toBe(true);
  });
});

describe("heatmap dates", () => {
  // Thursday of the last week; the grid runs Monday to Sunday.
  const now = new Date(2026, 10, 5, 18);

  it("places the current week last and counts back whole weeks", () => {
    expect(heatDate(83, 84, now)).toBe("2026-11-08");
    expect(heatDate(77, 84, now)).toBe("2026-11-02");
    expect(heatDate(0, 84, now)).toBe("2026-08-17");
  });
});

describe("splash", () => {
  it("fades, then leaves even when no transition fires", () => {
    vi.useFakeTimers();
    document.body.innerHTML = '<div id="splash"></div>';
    dismissSplash();
    expect(document.getElementById("splash")?.className).toBe("done");
    vi.advanceTimersByTime(400);
    expect(document.getElementById("splash")).toBeNull();
    vi.useRealTimers();
    expect(() => dismissSplash()).not.toThrow();
  });
});
