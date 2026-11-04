import { describe, expect, it } from "vitest";
import { projectWorkbench } from "../src/project-workbench";
import type { FocusModel, ScopeQuery } from "../src/contracts/setup";

const scope = (ids: string[]): ScopeQuery => ({
  source: "meegle",
  space: "space",
  type_key: "issue",
  work_item_type: "issue",
  type_name: "问题",
  filter: {
    field: "version",
    field_name: "影响版本",
    values: ids.map((id) => ({ id, label: `build-${id}` })),
  },
});
const seed: FocusModel = {
  code: "MODEL-A16",
  issues: [scope(["1", "2"])],
  branches: [],
  jobs: [],
  rules: {},
  targets: { severe_pct: 95, overall_pct: 85, fatal_max: 0 },
  accepted: [],
  tracked: [],
};

describe("project to workbench handoff", () => {
  it("uses the saved scope and targets for a new workbench", () => {
    expect(projectWorkbench(seed)).toEqual(seed);
  });
  it("keeps existing build routing, rules and targets while adding a scope", () => {
    const existing = {
      ...seed,
      issues: [scope(["3"])],
      branches: ["MP"],
      jobs: ["daily"],
      targets: { severe_pct: 100, overall_pct: 100, fatal_max: 0 },
      accepted: [123],
    };
    const merged = projectWorkbench(seed, existing);
    expect(merged).toEqual({
      ...existing,
      issues: [...existing.issues, ...seed.issues],
    });
    expect(existing.issues).toHaveLength(1);
  });
  it("opening repeatedly deduplicates reordered IDs and refreshes labels", () => {
    const existing = { ...seed, issues: [scope(["2", "1"])] };
    const next = projectWorkbench(seed, existing);
    expect(next.issues).toEqual(seed.issues);
    expect(projectWorkbench(seed, next)).toEqual(next);
  });
});
