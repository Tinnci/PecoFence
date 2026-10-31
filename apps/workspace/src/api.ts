import { isTauri, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HubReport } from "./contracts/hub";
import type { ImportPlan, ProjectStatus } from "./contracts/setup";
export type Page = "home" | "setup" | "health";
export function isPage(value: unknown): value is Page {
  return value === "home" || value === "setup" || value === "health";
}
export async function loadHub(
  check: boolean,
  component?: string,
): Promise<HubReport> {
  if (isTauri())
    return invoke<HubReport>("load_hub", {
      check,
      component: component ?? null,
    });
  if (import.meta.env.DEV) {
    const query = new URLSearchParams({ check: check ? "1" : "0" });
    if (component) query.set("component", component);
    const response = await fetch(`/__workspace/hub?${query}`);
    if (!response.ok) {
      const failure = await response.json().catch(() => null);
      throw failure ?? new Error("hub_unavailable");
    }
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
// SPM project setup runs only in the desktop shell, through the owner CLI.
function desktopOnly(): never {
  throw { code: "desktop_required" };
}
export async function projectStatus(): Promise<ProjectStatus> {
  return isTauri() ? invoke<ProjectStatus>("project_status") : desktopOnly();
}
/** Opens the native file dialog and returns the owner's plan; nothing is written. */
export async function planTemplate(): Promise<ImportPlan> {
  return isTauri() ? invoke<ImportPlan>("template_plan") : desktopOnly();
}
export async function applyTemplate(
  sha256: string,
  replace: boolean,
): Promise<ImportPlan> {
  return isTauri()
    ? invoke<ImportPlan>("template_apply", { sha256, replace })
    : desktopOnly();
}
export const desktop = isTauri;
