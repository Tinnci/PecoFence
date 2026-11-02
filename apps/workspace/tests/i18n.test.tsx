// @vitest-environment jsdom
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath, URL as NodeURL } from "node:url";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  focus: vi.fn(),
  loadHub: vi.fn(),
  desktop: () => true,
  navigation: async () => () => {},
  projectStatus: async () => ({ projects: [] }),
  loadConnections: async () => ({ connections: [] }),
}));
vi.mock("../src/api", () => mocks);
// NumberFlow's web component is registered only in browser builds. Its actual
// rendering is checked in the browser; jsdom exercises navigation and locale state.
vi.mock("@number-flow/react", () => ({
  default: ({
    value,
    prefix = "",
    suffix = "",
  }: {
    value: number;
    prefix?: string;
    suffix?: string;
  }) => (
    <span>
      {prefix}
      {value}
      {suffix}
    </span>
  ),
}));
import App from "../src/App";
import { i18n, locale, resolveLanguage, setLanguage, t } from "../src/i18n";
import { queryClient } from "../src/focus-cache";
import zh from "../src/locales/zh-CN.json";
import en from "../src/locales/en.json";
import { span } from "../src/Model";

const target = { severe_pct: 95, overall_pct: 85, fatal_max: 0 };
const model = (code: string) => ({
  code,
  targets: target,
  issues: 0,
  sources: [],
  new_issues: 0,
  days: [],
  days_from: "2026-01-01",
  week_changes: 0,
  week_merged: 0,
  branches: [],
  flagged: [],
  rule_counts: {},
  rules: {},
  errors: [],
});
beforeEach(() => {
  queryClient.clear();
  mocks.loadHub.mockResolvedValue({
    schema_version: 1,
    observed_at: "2026-11-02T00:00:00Z",
    checked: false,
    unmanifested: [],
    components: [],
  });
  mocks.focus.mockReset().mockImplementation(async (action, args) => {
    if (action === "list")
      return {
        models: ["A", "B"].map((code) => ({
          code,
          freeze: "2026-10-12",
          targets: target,
          issues: [],
          branches: [],
          rules: {},
          accepted: [],
          tracked: [],
        })),
      };
    if (action === "cached" || action === "refresh")
      return { summary: { models: [model("A"), model("B")] } };
    if (action === "builds" || action === "builds_cached")
      return {
        builds: {
          code: args.code,
          jobs: [],
          computed_at: new Date().toISOString(),
          errors: [],
        },
      };
    return {};
  });
});
afterEach(async () => {
  cleanup();
  await queryClient.cancelQueries();
  queryClient.clear();
  window.localStorage.clear();
});

describe("workspace languages", () => {
  it("keeps catalogs, interpolation fields and plural variants in sync", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort());
    for (const key of Object.keys(zh) as (keyof typeof zh)[]) {
      const fields = (message: string) =>
        [...message.matchAll(/\{\{(.*?)\}\}/g)].map((match) => match[1]).sort();
      expect(fields(en[key]), key).toEqual(fields(zh[key]));
      if (!key.startsWith("format.")) expect(en[key].trim(), key).not.toBe("");
    }
  });

  it("has a bundled translation for every application message key", () => {
    const root = fileURLToPath(new NodeURL("../src", import.meta.url));
    for (const file of readdirSync(root, { recursive: true })) {
      if (!/\.tsx?$/.test(String(file))) continue;
      const source = readFileSync(`${root}/${file}`, "utf8");
      for (const match of source.matchAll(/\bt\("([^"]+)"/g)) {
        expect(Object.hasOwn(zh, match[1]), `${file}: ${match[1]}`).toBe(true);
      }
    }
  });

  it("resolves system languages and formats English counts and durations", async () => {
    expect(resolveLanguage("system", ["fr-FR", "zh-Hant-TW"])).toBe("zh-CN");
    expect(resolveLanguage("system", ["en-GB", "zh-CN"])).toBe("en");
    expect(resolveLanguage("system", ["ja-JP"])).toBe("en");
    expect(resolveLanguage("zh-CN", ["en-US"])).toBe("zh-CN");
    await setLanguage("en");
    expect(t("focus.matches", { count: 1 })).toBe("1 match");
    expect(t("focus.matches", { count: 2 })).toBe("2 matches");
    expect(span(2 * 3600000 + 43 * 60000)).toBe("2 hr 43 min");
    expect(document.documentElement.lang).toBe("en");
    expect(document.documentElement.dir).toBe("ltr");
    expect(locale()).toBe("en");
    expect(window.localStorage.getItem("spm.language.v1")).toBe("en");
    await setLanguage("zh-CN");
    expect(span(2 * 3600000 + 43 * 60000)).toBe("2 小时 43 分");
  });

  it("switches language in place while preserving the model tab, scroll and cache", async () => {
    const user = userEvent.setup();
    render(<App />);
    const nav = await screen.findByRole("navigation", { name: "主导航" });
    await user.click(await within(nav).findByRole("button", { name: "A" }));
    await user.click(await screen.findByRole("tab", { name: "设置" }));
    const scroll = document.querySelector<HTMLElement>(".window-scroll")!;
    scroll.scrollTop = 280;
    await user.click(within(nav).getByRole("button", { name: "B" }));
    expect(
      screen.getByRole("tab", { name: "概览" }).getAttribute("aria-selected"),
    ).toBe("true");
    await user.click(within(nav).getByRole("button", { name: "A" }));
    expect(
      screen.getByRole("tab", { name: "设置" }).getAttribute("aria-selected"),
    ).toBe("true");
    expect(scroll.scrollTop).toBe(280);
    await waitFor(() =>
      expect(
        mocks.focus.mock.calls.filter((call) => call[0] === "refresh"),
      ).toHaveLength(2),
    );
    // Settings previews are debounced. Finish that initial read before measuring locale changes.
    await waitFor(() =>
      expect(
        document.querySelector('.preview-panel [data-slot="spinner"]'),
      ).toBeNull(),
    );
    const calls = mocks.focus.mock.calls.length;
    await user.click(screen.getByRole("button", { name: "更多" }));
    await user.click(
      await screen.findByRole("menuitemradio", { name: "English" }),
    );
    await screen.findByRole("tab", { name: "Settings" });
    expect(
      screen
        .getByRole("tab", { name: "Settings" })
        .getAttribute("aria-selected"),
    ).toBe("true");
    expect(screen.getByRole("button", { name: "More" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Rules" }).textContent).toContain(
      "Features entering MP after freeze",
    );
    expect(scroll.scrollTop).toBe(280);
    expect(mocks.focus.mock.calls).toHaveLength(calls);
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    expect(
      screen.getByRole("tab", { name: "设置" }).getAttribute("aria-selected"),
    ).toBe("true");
  });
});
