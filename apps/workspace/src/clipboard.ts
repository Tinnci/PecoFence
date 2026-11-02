import { isTauri } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export async function copyText(text: string) {
  if (isTauri()) await writeText(text);
  else await navigator.clipboard.writeText(text);
}
