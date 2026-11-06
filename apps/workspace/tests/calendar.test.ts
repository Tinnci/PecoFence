import { describe, expect, it } from "vitest";
import type { CalEvent } from "../src/contracts/setup";
import {
  monthWeeks,
  shiftMonth,
  spanDays,
  visibleEvents,
  weekBars,
} from "../src/calendar";

const event = (over: Partial<CalEvent>): CalEvent => ({
  id: "e",
  source: "sheet",
  key: "pre1",
  stage: "build",
  milestone: true,
  project: "P",
  model: "",
  legacy: false,
  start: "2026-11-23",
  end: "2026-11-23",
  status: "",
  version: "",
  customer: "",
  owner: "",
  note: "",
  origins: [],
  ...over,
});
const week = [
  "2026-11-23",
  "2026-11-24",
  "2026-11-25",
  "2026-11-26",
  "2026-11-27",
  "2026-11-28",
  "2026-11-29",
];

describe("calendar", () => {
  it("lays a month out from Monday", () => {
    const weeks = monthWeeks(2026, 10);
    expect(weeks[0][6]).toBe("2026-11-01");
    expect(weeks[0][0]).toBe("2026-10-26");
    expect(weeks.at(-1)?.[0]).toBe("2026-11-30");
    expect(weeks.every((w) => w.length === 7)).toBe(true);
  });
  it("clips a stage to the week and stacks only what overlaps", () => {
    const bars = weekBars(week, [
      event({ id: "a", start: "2026-11-20", end: "2026-11-24" }),
      event({ id: "b", start: "2026-11-24", end: "2026-11-30" }),
      event({ id: "c", start: "2026-11-25", end: "2026-11-25" }),
      event({ id: "d", start: "2026-12-01", end: "2026-12-02" }),
    ]);
    const of = (id: string) => bars.find((b) => b.event.id === id);
    expect(of("a")).toMatchObject({ column: 0, span: 2, lane: 0 });
    expect(of("b")).toMatchObject({ column: 1, span: 6, lane: 1 });
    expect(of("c")).toMatchObject({ lane: 0 });
    expect(of("d")).toBeUndefined();
  });
  it("knows a stage's length and which weeks it runs past", () => {
    const stage = event({ start: "2026-11-20", end: "2026-12-02" });
    expect(spanDays(stage)).toBe(13);
    expect(spanDays(event({}))).toBe(1);
    expect(weekBars(week, [stage])[0]).toMatchObject({
      before: true,
      after: true,
    });
    expect(weekBars(week, [event({})])[0]).toMatchObject({
      before: false,
      after: false,
    });
  });
  it("puts milestones first so a fold never hides them", () => {
    const bars = weekBars(week, [
      event({ id: "w", milestone: false, end: "2026-11-29" }),
      event({ id: "m", start: "2026-11-27", end: "2026-11-27" }),
      event({ id: "n", start: "2026-11-23", end: "2026-11-23" }),
    ]);
    const lane = (id: string) => bars.find((b) => b.event.id === id)?.lane;
    expect([lane("m"), lane("n"), lane("w")]).toEqual([0, 0, 1]);
  });
  it("keeps replaced rows and legacy models out unless asked", () => {
    const list = [
      event({ id: "a", model: "M" }),
      event({ id: "b", model: "M", status: "已延期" }),
      event({ id: "c", model: "L", legacy: true }),
      event({ id: "d", project: "X", customer: "Nova" }),
    ];
    const ids = (v: Parameters<typeof visibleEvents>[1]) =>
      visibleEvents(list, v).map((e) => e.id);
    expect(ids({ scope: "followed", delayed: false, words: "" })).toEqual([
      "a",
    ]);
    expect(ids({ scope: "all", delayed: true, words: "" })).toHaveLength(4);
    expect(ids({ scope: "all", delayed: false, words: "NOVA" })).toEqual(["d"]);
  });
  it("crosses a year", () => {
    expect(shiftMonth(2026, 11, 1)).toEqual({ year: 2027, month: 0 });
  });
});
