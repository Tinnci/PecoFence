import { beforeEach, describe, expect, it, vi } from "vitest";

describe("theme", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.classList.remove("dark");
  });

  it("starts light and remembers dark", async () => {
    const first = await import("../src/theme");
    expect(first.themePreference()).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    first.setTheme("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    vi.resetModules();
    document.documentElement.classList.remove("dark");
    const next = await import("../src/theme");
    expect(next.themePreference()).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });
});
