// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

beforeEach(() => {
  localStorage.clear();
  vi.resetModules();
});

describe("versioned work views", () => {
  it("migrates legacy views once and keeps their IDs across restart", async () => {
    localStorage.setItem(
      "tinnci.views.v1.MODEL.issues",
      JSON.stringify([
        { name: "Blocked", stage: "blocked", filters: { owner: ["Alice"] } },
      ]),
    );
    const first = await import("../src/view-storage");
    const migrated = first.readViewStore("MODEL", "issues").views[0];
    expect(migrated).toMatchObject({
      name: "Blocked",
      stage: "blocked",
      filters: { owner: ["Alice"] },
    });
    expect(migrated.id).toBeTruthy();
    vi.resetModules();
    const restarted = await import("../src/view-storage");
    expect(restarted.readViewStore("MODEL", "issues").views[0].id).toBe(
      migrated.id,
    );
    expect(localStorage.getItem("tinnci.views.v1.MODEL.issues")).not.toBeNull();
  });

  it("restores current settings without changing a named view or another list", async () => {
    const storage = await import("../src/view-storage");
    const saved = {
      ...storage.normalizeView({ stage: "blocked" }),
      id: "saved",
      name: "Blocked",
    };
    storage.saveViewStore("MODEL", "issues", {
      views: [saved],
      selected: saved.id,
      current: storage.normalizeView({
        search: "camera",
        group: "owner",
        sort: [{ field: "held", dir: "desc" }],
      }),
    });
    vi.resetModules();
    const restarted = await import("../src/view-storage");
    const record = restarted.readViewStore("MODEL", "issues");
    expect(record.current).toMatchObject({
      search: "camera",
      group: "owner",
      sort: [{ field: "held", dir: "desc" }],
    });
    expect(record.views[0].stage).toBe("blocked");
    expect(record.selected).toBe("saved");
    expect(
      restarted.readViewStore("MODEL", "requirements").current.search,
    ).toBe("");
    expect(restarted.readViewStore("OTHER", "issues").views).toEqual([]);
  });

  it("preserves an unsupported future store while allowing session-only changes", async () => {
    const original = JSON.stringify({ version: 99, private_field: "keep" });
    localStorage.setItem("tinnci.workviews.v2.MODEL.issues", original);
    const storage = await import("../src/view-storage");
    expect(
      storage.saveViewStore("MODEL", "issues", {
        current: storage.normalizeView({ search: "session" }),
      }),
    ).toBe(false);
    expect(storage.readViewStore("MODEL", "issues").current.search).toBe(
      "session",
    );
    expect(localStorage.getItem("tinnci.workviews.v2.MODEL.issues")).toBe(
      original,
    );
    expect(storage.viewStorageFailed("MODEL", "issues")).toBe(true);
  });
});
