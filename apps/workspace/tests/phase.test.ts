import { expect, it } from "vitest";
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
  const now = Date.parse("2026-11-06T10:00:00");
  expect(daysUntil("2026-11-06", now)).toBe(1);
  expect(daysUntil("2026-11-23", now)).toBe(18);
  expect(daysUntil("2026-11-01", now)).toBeLessThan(0);
});
