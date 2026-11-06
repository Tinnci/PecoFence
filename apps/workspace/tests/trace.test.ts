import { describe, expect, it } from "vitest";
import type { TraceSpan } from "../src/contracts/setup";
import {
  duration,
  hasChildren,
  slowest,
  ticks,
  visibleRows,
} from "../src/trace";

const span = (depth: number, self_ms = 0): TraceSpan => ({
  app: "spm",
  name: "x",
  verb: "",
  depth,
  offset_ms: 0,
  ms: 10,
  self_ms,
  status: "ok",
  code: "",
  critical: [],
});

describe("trace rows", () => {
  const tree = [span(0, 1), span(1, 2), span(2, 90), span(1, 5), span(0, 3)];
  it("folding hides only what sits under the span", () => {
    expect(visibleRows(tree, new Set())).toEqual([0, 1, 2, 3, 4]);
    expect(visibleRows(tree, new Set([1]))).toEqual([0, 1, 3, 4]);
    expect(visibleRows(tree, new Set([0]))).toEqual([0, 4]);
  });
  it("knows which spans open and which took longest", () => {
    expect(hasChildren(tree, 0)).toBe(true);
    expect(hasChildren(tree, 2)).toBe(false);
    expect(hasChildren(tree, 4)).toBe(false);
    expect(slowest(tree)).toBe(2);
    expect(slowest([span(0)])).toBe(-1);
  });
  it("reads times and rounds the ruler", () => {
    expect(duration(840)).toBe("840 ms");
    expect(duration(12_340)).toBe("12.3 s");
    expect(duration(125_000)).toBe("2m 05s");
    expect(ticks(960)).toEqual([0, 200, 400, 600, 800]);
    expect(ticks(0)).toEqual([0]);
  });
});
