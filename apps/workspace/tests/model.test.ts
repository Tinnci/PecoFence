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
