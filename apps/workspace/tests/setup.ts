import { beforeEach, vi } from "vitest";
import { setLanguage } from "../src/i18n";
// Node 26 exposes an unavailable localStorage global that shadows jsdom's Storage.
if (typeof window !== "undefined" && !window.localStorage) {
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, String(value));
    },
    removeItem: (key: string) => {
      values.delete(key);
    },
    clear: () => values.clear(),
    key: (index: number) => [...values.keys()][index] ?? null,
    get length() {
      return values.size;
    },
  });
}
beforeEach(async () => {
  await setLanguage("zh-CN");
});
