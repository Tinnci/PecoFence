import { describe, expect, it } from "vitest";
import type { WorkItem } from "../src/contracts/setup";
import {
  boardColumns,
  burndown,
  createdResolved,
  currentIteration,
  flowMap,
  flowMetrics,
  iterationProblem,
  iterationStart,
  iterationWindow,
  oldestItems,
  statusAge,
} from "../src/flow";

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

describe("flow charts", () => {
  it("runs totals from zero and leaves out work older than the window", () => {
    const trend = createdResolved(
      [item("done", 10, 5), item("pending", 3, 3), item("done", 200, 150)],
      now,
    );
    expect(trend.created[0]).toBe(0);
    expect(trend.created.at(-1)).toBe(2);
    expect(trend.resolved.at(-1)).toBe(1);
  });

  it("burns down as work closes, grows with added work and stops at today", () => {
    const start = now - 5 * 86_400_000;
    const burn = burndown(
      [item("done", 20, 3), item("active", 20, 20), item("pending", 2, 2)],
      start,
      14,
      now,
    );
    expect(burn.remaining[0]).toBe(2);
    expect(burn.scope[0]).toBe(2);
    expect(burn.remaining[2]).toBe(1);
    expect(burn.remaining[3]).toBe(2);
    expect(burn.scope[5]).toBe(3);
    expect(burn.remaining[7]).toBeNull();
    expect(burn.ideal[14]).toBe(0);
  });

  it("leaves cancelled work and work closed before the iteration out of it", () => {
    const start = now - 86_400_000;
    const burn = burndown(
      [item("cancelled", 9, 0), item("done", 40, 30)],
      start,
      14,
      now,
    );
    expect(burn.scope[0]).toBe(0);
  });

  it("starts iterations on a fixed Monday every two weeks", () => {
    const first = iterationStart(now);
    expect(iterationStart(first + 86_400_000)).toBe(first);
    expect(iterationStart(first + 14 * 86_400_000)).toBe(
      first + 14 * 86_400_000,
    );
    expect(new Date(first + 8 * 3_600_000).getUTCDay()).toBe(1);
  });
});

describe("named iterations", () => {
  const first = { name: "S1", start: "2026-11-02", end: "2026-11-15" };
  const second = { name: "S2", start: "2026-11-16", end: "2026-11-29" };

  it("counts both end dates and finds the one holding today", () => {
    expect(iterationWindow(first).days).toBe(14);
    const noon = new Date(2026, 10, 15, 12).getTime();
    expect(currentIteration([first, second], noon)?.name).toBe("S1");
    expect(currentIteration([second], noon)).toBeUndefined();
  });

  it("names what stops an iteration being saved", () => {
    expect(iterationProblem({ ...first, name: " " }, [])).toBe("name");
    expect(iterationProblem({ ...first, end: first.start }, [])).toBe("dates");
    expect(iterationProblem({ ...second, start: "2026-11-15" }, [first])).toBe(
      "overlap",
    );
    expect(iterationProblem(second, [first])).toBeNull();
  });
});

describe("flow map", () => {
  it("finds the stage where work piles up, not the backlog", () => {
    const { bottleneck, stages } = flowMap(
      [
        item("pending", 9, 9),
        item("pending", 9, 8),
        item("pending", 9, 7),
        item("testing", 9, 4),
        item("testing", 9, 6),
        item("active", 9, 1),
      ],
      now,
    );
    expect(bottleneck?.column).toBe("testing");
    expect(stages.find((s) => s.column === "testing")?.medianAge).toBe(4);
    expect(flowMap([item("pending", 1, 1)], now).bottleneck).toBeNull();
  });

  it("marks only the longest waiting work", () => {
    const list = [1, 9, 4, 7].map((d) => item("active", 20, d));
    const old = oldestItems(list, now, 2);
    expect(old.map((i) => statusAge(i, now))).toEqual([9, 7]);
  });
});
