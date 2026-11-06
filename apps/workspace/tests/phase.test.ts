import { expect, it } from "vitest";
import { otaLede } from "../src/Focus";
import { daysUntil, layoutOf, layouts } from "../src/phase";

it("keeps every tab reachable in every phase", () => {
  const all = [...layoutOf().tabs].sort();
  for (const layout of Object.values(layouts))
    expect([...layout.tabs, ...layout.more].sort()).toEqual(all);
});

it("leads each phase with its own work", () => {
  expect(layoutOf("development").tabs[1]).toBe("requirements");
  expect(layoutOf("release").gates).toBe(true);
  expect(layoutOf("ota").tabs[1]).toBe("ota");
  expect(layoutOf("ota").forecast).toBe(false);
  expect(layoutOf("").more).toEqual([]);
});

it("counts days to a date", () => {
  const now = new Date("2026-11-06T10:00:00");
  expect(daysUntil("2026-11-06", now)).toBe(0);
  expect(daysUntil("2026-11-17", now)).toBe(11);
  expect(daysUntil("2026-11-01", now)).toBeLessThan(0);
});

it("headlines the soonest OTA with work left", () => {
  const plan = (version: string, pre1: string, merged: number) => ({
    version,
    pre1,
    pre2: "",
    official: "2099-12-01",
    rollout: "",
    state: "upcoming",
    total: 5,
    merged,
    verified: 0,
    late: 0,
    must_open: 0,
    items: [],
  });
  const ota = (code: string, p: ReturnType<typeof plan>) => ({
    code,
    plans: [p],
    undecided: [],
    left_out: [],
  });
  const report = {
    errors: [],
    models: [
      ota("DONE", plan("OTA1", "2099-01-01", 5)),
      ota("LATER", plan("OTA2", "2099-03-01", 1)),
      ota("SOON", plan("OTA1", "2099-02-01", 2)),
    ],
  };
  const models = report.models.map((m) => ({ code: m.code })) as never;
  expect(otaLede(models, report)).toContain("SOON OTA1");
  expect(otaLede([], report)).toBeNull();
});
