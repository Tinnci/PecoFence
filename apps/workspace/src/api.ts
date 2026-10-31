import { isTauri, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HubReport } from "./contracts/hub";
export type Page = "home" | "setup" | "health";
export function isPage(value: unknown): value is Page {
  return value === "home" || value === "setup" || value === "health";
}
export async function loadHub(check: boolean): Promise<HubReport> {
  if (isTauri()) return invoke<HubReport>("load_hub", { check });
  if (import.meta.env.DEV) {
    const response = await fetch(`/__workspace/hub?check=${check ? "1" : "0"}`);
    if (!response.ok) throw new Error("hub_unavailable");
    return response.json();
  }
  throw new Error("desktop_required");
}
export async function navigation(onPage: (page: Page) => void) {
  if (!isTauri()) return () => {};
  let updated = false;
  const cleanup = await listen<string>("workspace-target", (event) => {
    if (isPage(event.payload)) {
      updated = true;
      onPage(event.payload);
    }
  });
  try {
    const initial = await invoke<string>("launch_target");
    if (!updated && isPage(initial)) onPage(initial);
  } catch {
    /* Home remains usable if launch arguments cannot be read. */
  }
  return cleanup;
}
