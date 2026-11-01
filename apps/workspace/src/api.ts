import { isTauri, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HubReport } from "./contracts/hub";
import type {
  Activation,
  ConnectionReport,
  DiscoverArgs,
  Discovery,
  Draft,
  DraftChoices,
  ImportPlan,
  ProjectStatus,
  ServiceHealth,
} from "./contracts/setup";
export type Page = "home" | "setup" | "health" | "project" | "activity";
export function isPage(value: unknown): value is Page {
  return (
    value === "home" ||
    value === "setup" ||
    value === "health" ||
    value === "project" ||
    value === "activity"
  );
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
/** Each source tool's own login, observed by `spm connections`. */
export async function loadConnections(): Promise<ConnectionReport> {
  return isTauri() ? invoke<ConnectionReport>("connections") : desktopOnly();
}
/** Runs the tool's browser login (Meegle) and returns the rechecked row. */
export async function signIn(source: string): Promise<ConnectionReport> {
  return isTauri()
    ? invoke<ConnectionReport>("connection_sign_in", { source })
    : desktopOnly();
}
/** One `spm discover` list. */
export async function discover(
  list:
    | "spaces"
    | "types"
    | "groups"
    | "items"
    | "jira_projects"
    | "jira_versions"
    | "jira_filters"
    | "gerrit_projects"
    | "gerrit_activity",
  args: DiscoverArgs = {},
): Promise<Discovery> {
  return isTauri()
    ? invoke<Discovery>("discover", { list, args })
    : desktopOnly();
}
/** Switches the background service to the project; resolves on its first snapshot. */
export async function activate(
  project: string,
  scope: string,
): Promise<Activation> {
  return isTauri()
    ? invoke<Activation>("activate", { project, scope })
    : desktopOnly();
}
/** `spm setup draft`; with the confirmed sha256 it creates the project. */
export async function draft(
  choices: DraftChoices,
  sha256?: string,
): Promise<Draft> {
  return isTauri()
    ? invoke<Draft>("draft", { choices, sha256: sha256 ?? null })
    : desktopOnly();
}
/** The background service's health: project, last snapshot's gates and unmapped values. */
export async function serviceStatus(): Promise<ServiceHealth> {
  return isTauri() ? invoke<ServiceHealth>("service_status") : desktopOnly();
}
/** Adds stage mappings for values that appeared after the project was created. */
export async function remap(
  project: string,
  scope: string,
  stages: Record<string, string>,
): Promise<ImportPlan> {
  return isTauri()
    ? invoke<ImportPlan>("remap", { project, scope, stages })
    : desktopOnly();
}
