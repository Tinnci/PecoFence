import { describe, expect, it } from "vitest";
import type { MyIssue } from "../src/contracts/setup";
import { bandCounts, bandOf, groupWork, mergeWork } from "../src/my-issues";

function issue(
  id: string,
  score: number,
  more: Partial<MyIssue> = {},
): MyIssue {
  return {
    source: "jira",
    id,
    display_id: id,
    title: id,
    url: "",
    status: "Open",
    severity: "",
    model: "A",
    followed: true,
    score,
    reasons: [],
    ...more,
  };
}

describe("my issues", () => {
  it("bands scores at 60 and 40", () => {
    expect([65, 60, 59, 40, 39].map(bandOf)).toEqual([
      "urgent",
      "urgent",
      "high",
      "high",
      "normal",
    ]);
  });

  it("merges one title tracked twice and counts it once", () => {
    const works = mergeWork([
      issue("1", 70, { title: "Same" }),
      issue("2", 45),
      issue("3", 30, { title: " same " }),
    ]);
    expect(works.map((w) => w.items.length)).toEqual([2, 1]);
    expect(bandCounts(works)).toEqual({ urgent: 1, high: 1, normal: 0 });
  });

  it("keeps the same title apart across sources", () => {
    const works = mergeWork([
      issue("1", 30, { title: "x" }),
      issue("2", 30, { title: "x", source: "meegle" }),
    ]);
    expect(works).toHaveLength(2);
  });

  it("groups by model with the most urgent group first", () => {
    const works = mergeWork([
      issue("1", 30, { model: "B" }),
      issue("2", 65, { model: "A" }),
      issue("3", 25, { model: "B" }),
    ]);
    const groups = groupWork(works, "model");
    expect(groups.map((g) => [g.key, g.works.length])).toEqual([
      ["A", 1],
      ["B", 2],
    ]);
  });

  it("groups by the reason beyond severity", () => {
    const reasons = [
      { key: "severity", points: 30 },
      { key: "customer", points: 10 },
    ];
    const works = mergeWork([
      issue("1", 40, { reasons }),
      issue("2", 15, { reasons: [{ key: "severity", points: 15 }] }),
    ]);
    expect(groupWork(works, "reason").map((g) => g.key)).toEqual([
      "customer",
      "severity",
    ]);
    expect(groupWork(works, "rank")).toEqual([]);
  });
});
