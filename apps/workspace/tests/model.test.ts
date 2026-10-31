import { describe, expect, it } from "vitest";
import { progress, attention, label } from "../src/model";
import type { Component, HubReport } from "../src/contracts/hub";
import { isPage } from "../src/api";
const component = (id: string, lifecycle = "active"): Component => ({
  id,
  lifecycle,
  name: id,
  group: "delivery",
  summary: "",
  host: "wsl",
  installed: true,
  checkable: false,
  depends_on: [],
  issues: [],
  state: "setup_pending",
  setup: [
    {
      id: "token",
      title: "Token",
      why: "Read access",
      needs: [],
      state: "planned",
    },
  ],
});
describe("truthful hub presentation", () => {
  it("keeps planned work outstanding and excludes paused tools from progress", () => {
    const live = component("tool");
    live.setup.push({
      id: "ready",
      title: "Ready",
      why: "",
      needs: [],
      state: "done",
    });
    const report: HubReport = {
      schema_version: 1,
      checked: false,
      observed_at: "",
      unmanifested: [],
      components: [live, component("paused", "paused")],
    };
    expect(progress(report)).toEqual({ done: 1, total: 2 });
    expect(attention(live)).toBe(true);
    expect(label("planned")).toBe("待支持");
    expect(label("unexpected")).toBe("状态未知");
  });
  it("only accepts available navigation targets", () => {
    expect(isPage("setup")).toBe(true);
    expect(isPage("board")).toBe(false);
    expect(isPage("--exec")).toBe(false);
  });
});

import { mergeReport, checkExplanation } from "../src/model";
describe("scoped observations", () => {
  const baseline: HubReport = {
    schema_version: 1,
    observed_at: "initial",
    checked: false,
    components: [
      component("first"),
      { ...component("second"), state: "ready", checked_at: "earlier" },
    ],
    unmanifested: ["other"],
  };
  it("updates one component while preserving other checks and the full-report timestamp", () => {
    const incoming: HubReport = {
      ...baseline,
      scope: "first",
      observed_at: "later",
      checked: true,
      components: [
        { ...component("first"), state: "ready", checked_at: "later" },
      ],
      unmanifested: [],
    };
    const result = mergeReport(baseline, incoming);
    expect(result.components[0].checked_at).toBe("later");
    expect(result.components[1]).toBe(baseline.components[1]);
    expect(result.observed_at).toBe("initial");
    expect(result.checked).toBe(false);
    expect(result.unmanifested).toEqual(["other"]);
  });
  it("rejects an unrelated component response and replaces the inventory only on a full response", () => {
    expect(() =>
      mergeReport(baseline, { ...baseline, scope: "missing", components: [] }),
    ).toThrow();
    expect(
      mergeReport(baseline, { ...baseline, components: [] }).components,
    ).toEqual([]);
  });
  it("explains transport, authentication and timeout failures", () => {
    expect(
      checkExplanation({ state: "action_needed", code: "PROXY_UNAVAILABLE" }),
    ).toContain("网页转发");
    expect(
      checkExplanation({ state: "action_needed", code: "auth_expired" }),
    ).toContain("凭据");
    expect(
      checkExplanation({ state: "unknown", reason: "check_timeout" }),
    ).toContain("超时");
  });
});
