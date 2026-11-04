// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { WorkItem, WorkItems } from "../src/contracts/setup";
import { ownerScope, ownerWorkload } from "../src/owner-workload";
import { OwnerWorkload } from "../src/OwnerWorkload";
import { matches } from "../src/WorkFilters";
import { queryClient } from "../src/focus-cache";
import {
  normalizeView,
  readViewStore,
  saveViewStore,
} from "../src/view-storage";

const now = Date.parse("2026-11-04T01:00:00+08:00");
function item(id: string, patch: Partial<WorkItem> = {}): WorkItem {
  return {
    id,
    source: "meegle",
    scope_id: "scope",
    url: `https://project.example/space/issue/detail/${id}`,
    owner: "Alice",
    group: "active",
    updated: "2026-11-01T00:00:00+08:00",
    activity: { last_activity: "2026-10-01T00:00:00+08:00" },
    ...patch,
  } as WorkItem;
}
const data = (items: WorkItem[]): WorkItems => ({
  code: "OWNERS",
  kind: "issues",
  checked_at: "",
  sources: [],
  items,
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});

describe("owner follow-up", () => {
  it("deduplicates overlaps, isolates source spaces, and separates unknown activity", () => {
    const blocked = item("1", {
      group: "blocked",
      local: { next_check: "2026-11-04" } as WorkItem["local"],
    });
    const unknown = item("2", {
      owner: "",
      activity: { last_activity: "" } as WorkItem["activity"],
    });
    const other = item("3", {
      url: "https://project.example/another/issue/detail/3",
    });
    const rows = ownerWorkload(
      [blocked, blocked, unknown, other, item("4", { group: "done" })],
      now,
    );
    expect(rows).toHaveLength(3);
    expect(
      rows.find((r) => r.scope === ownerScope(blocked) && r.owner === "Alice"),
    ).toMatchObject({ open: 1, blocked: 1, stale: 1, due: 1 });
    expect(rows.find((r) => r.owner === "")).toMatchObject({
      open: 1,
      stale: 0,
      unknown: 1,
    });
    expect(rows.filter((r) => r.owner === "Alice")).toHaveLength(2);
  });

  it("replaces hidden old filters with the exact owner, source and metric", async () => {
    const user = userEvent.setup();
    const blocked = item("1", { group: "blocked" });
    const otherSpace = item("2", {
      group: "blocked",
      url: "https://elsewhere.example/space/issue/detail/2",
    });
    saveViewStore("OWNERS", "issues", {
      current: normalizeView({
        search: "hidden",
        filters: { owner: ["Other"] },
      }),
    });
    const open = vi.fn();
    render(
      <OwnerWorkload
        code="OWNERS"
        issues={data([blocked, otherSpace])}
        requirements={null}
        onOpen={open}
      />,
    );
    const rows = screen.getAllByRole("row");
    const selected = rows.find((row) =>
      row.textContent?.includes("project.example"),
    )!;
    await user.click(
      within(selected).getByRole("button", { name: /Alice.*阻塞.*1/ }),
    );
    const settings = readViewStore("OWNERS", "issues").current;
    expect(open).toHaveBeenCalledWith("issues");
    expect(settings).toMatchObject({
      search: "",
      stage: "open",
      attention: "blocked",
      filters: { owner: ["Alice"], source: [ownerScope(blocked)] },
    });
    expect(
      [blocked, otherSpace].filter((entry) => matches(entry, settings.filters)),
    ).toEqual([blocked]);
  });

  it("opens the requirements list from its own owner statistics", async () => {
    const user = userEvent.setup();
    const open = vi.fn();
    render(
      <OwnerWorkload
        code="OWNERS-R"
        issues={data([])}
        requirements={data([item("R", { owner: "Bob" })])}
        onOpen={open}
      />,
    );
    await user.click(screen.getByRole("button", { name: "需求" }));
    await user.click(screen.getByRole("button", { name: "Bob" }));
    expect(open).toHaveBeenCalledWith("requirements");
    expect(
      readViewStore("OWNERS-R", "requirements").current.filters.owner,
    ).toEqual(["Bob"]);
  });
});
