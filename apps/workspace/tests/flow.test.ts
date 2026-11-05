import { describe, expect, it } from "vitest";
import type { WorkItem } from "../src/contracts/setup";
import { boardColumns, flowMetrics, statusAge } from "../src/flow";

const now = Date.parse("2026-11-05T00:00:00Z");
const daysAgo = (n: number) => new Date(now - n * 86_400_000).toISOString();

function item(
  group: string,
  created: number,
  since: number,
  source = "history",
) {
  return {
    id: `${group}-${created}-${since}`,
    source: "jira",
    group,
    activity: {
      created: daysAgo(created),
      status_since: daysAgo(since),
      status_since_source: source,
    },
  } as unknown as WorkItem;
}

describe("flow", () => {
  it("ages work from when it entered its status", () => {
    expect(statusAge(item("active", 30, 4), now)).toBe(4);
  });

  it("measures cycle time from finished work and buckets by week", () => {
    const done = [10, 20, 30, 40, 50].map((d) => item("done", d + 5, 5));
    const flow = flowMetrics([...done, item("active", 3, 3)], now);
    expect(flow.cycleP50).toBe(30);
    expect(flow.cycleP85).toBe(50);
    expect(flow.finished[11]).toBe(5);
    expect(flow.arrived[11]).toBe(1);
    expect(flow.wip).toBe(1);
  });

  it("does not take an unknown status date for a finish", () => {
    const flow = flowMetrics([item("done", 20, 1, "observed")], now);
    expect(flow.finished.every((n) => n === 0)).toBe(true);
    expect(flow.cycleP85).toBeNull();
  });

  it("flags work older than the usual cycle, and falls back on few finishes", () => {
    const few = flowMetrics(
      [item("testing", 40, 20), item("testing", 5, 5)],
      now,
    );
    expect(few.agedAfter).toBe(14);
    expect(few.aged).toBe(1);
    const done = [3, 3, 3, 3, 3].map((d, i) => item("done", d + 2 + i * 0, 2));
    const many = flowMetrics([...done, item("active", 9, 9)], now);
    expect(many.agedAfter).toBe(3);
    expect(many.aged).toBe(1);
  });

  it("leaves pending work out of work in progress and finished work off the board", () => {
    const flow = flowMetrics(
      [item("pending", 2, 2), item("blocked", 2, 2)],
      now,
    );
    expect([flow.wip, flow.blocked]).toEqual([1, 1]);
    const columns = boardColumns(
      [item("testing", 9, 2), item("testing", 9, 8), item("done", 9, 1)],
      now,
    );
    expect(columns.get("testing")?.map((i) => statusAge(i, now))).toEqual([
      8, 2,
    ]);
    expect(columns.has("done")).toBe(false);
  });
});
