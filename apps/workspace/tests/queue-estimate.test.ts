import { expect, it } from "vitest";
import { forecastQueue } from "../src/queue-estimate";
import type { BuildCapacity, CapacityTask } from "../src/contracts/setup";
const now = 1000000;
const task = (id: number, job = `job${id}`, label = "fast"): CapacityTask => ({
  id,
  job,
  label,
  since: now - 10000 + id,
  blocked: false,
  buildable: true,
  concurrent: true,
  low: 60000,
  high: 60000,
  samples: 3,
});
const capacity = (): BuildCapacity => ({
  observed_at: new Date(now).toISOString(),
  slots: [20, 50].map((min, i) => ({
    node: `node${i}`,
    labels: ["fast"],
    online: true,
    idle: false,
    job: `running${i}`,
    started: now - 1000,
    low: min * 60000 + 1000,
    high: min * 60000 + 1000,
  })),
  tasks: [task(1), task(2)],
});
it("schedules across parallel slots and includes other queued jobs", () => {
  const c = capacity();
  c.tasks[0].low = c.tasks[0].high = 60 * 60000;
  const estimates = forecastQueue(c, now);
  expect(estimates.get(1)?.startHigh).toBe(now + 20 * 60000);
  expect(estimates.get(2)?.startHigh).toBe(now + 50 * 60000);
});
it("preserves unknown release times instead of claiming an immediate start", () => {
  const c = capacity();
  c.slots = [c.slots[0]];
  c.slots[0].started = now - 200 * 60000;
  expect(forecastQueue(c, now).get(1)).toMatchObject({
    reason: "releaseUnknown",
    startHigh: null,
  });
});
it("respects offline nodes, explicit nonconcurrency and stale snapshots", () => {
  const c = capacity();
  c.slots[0].online = false;
  c.tasks = [task(1, "running1")];
  c.tasks[0].concurrent = false;
  expect(forecastQueue(c, now).get(1)?.startLow).toBe(now + 50 * 60000);
  expect(forecastQueue(c, now + 180000).get(1)?.reason).toBe("stale");
});
it("does not invent constraints for complex labels or missing queue times", () => {
  const c = capacity();
  c.tasks[0].label = "fast || slow";
  expect(forecastQueue(c, now).get(2)?.reason).toBe("constraints");
  c.tasks = [task(1)];
  c.tasks[0].since = null;
  expect(forecastQueue(c, now).get(1)?.reason).toBe("orderUnknown");
});
