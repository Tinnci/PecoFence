import { describe, expect, it } from "vitest";
import { runtimeEstimate } from "../src/build-estimate";
import type { JobView } from "../src/contracts/setup";
const job = (durations: number[]): JobView => ({
  job: "demo",
  name: "demo",
  phase: "mp",
  disabled: false,
  parameters: {},
  changes: [],
  builds: [
    { number: 9, building: true, timestamp: 1000, duration: 0, estimated: 100 },
    ...durations.map((duration, i) => ({
      number: i,
      building: false,
      result: "SUCCESS",
      timestamp: 1,
      duration,
      estimated: 0,
    })),
  ],
});
describe("build time estimates", () => {
  it("uses successful history quartiles and stops counting down after that range", () => {
    const j = job([100, 200, 300, 400, 500]);
    expect(runtimeEstimate(j, 1100)).toMatchObject({
      source: "history",
      low: 200,
      high: 400,
      remainingLow: 100,
      remainingHigh: 300,
    });
    expect(runtimeEstimate(j, 1500)).toMatchObject({
      overdue: true,
      progress: null,
    });
  });
  it("excludes failures and uses Jenkins with too few successful samples", () => {
    const j = job([100, 200, 9999]);
    j.builds[3].result = "FAILURE";
    expect(runtimeEstimate(j, 1050)).toMatchObject({
      source: "jenkins",
      expected: 100,
      history: { count: 2 },
    });
  });
  it("keeps unknown start times and missing estimates indeterminate", () => {
    const j = job([]);
    j.builds[0].timestamp = 0;
    j.builds[0].estimated = 0;
    expect(runtimeEstimate(j, 1000)).toMatchObject({
      source: "unknown",
      elapsed: null,
      progress: null,
      remainingHigh: null,
    });
  });
});
