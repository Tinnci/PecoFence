import { browserUrl } from "./components/external-link";
import type { BuildRun, JobView } from "./contracts/setup";

function runUrl(run?: BuildRun) {
  const url = browserUrl(run?.url);
  if (!url || !run) return null;
  const parsed = new URL(url);
  // Only derive Jenkins paths from a confirmed /<build number>/ URL.
  if (!parsed.pathname.endsWith(`/${run.number}/`)) return null;
  return parsed;
}
export function jobBrowserUrl(job: JobView): string | null {
  for (const run of job.builds) {
    const url = runUrl(run);
    if (url) {
      url.pathname = url.pathname.slice(0, -`${run.number}/`.length);
      url.search = "";
      url.hash = "";
      return url.href;
    }
  }
  return null;
}
export function consoleBrowserUrl(run?: BuildRun): string | null {
  const url = runUrl(run);
  if (!url) return null;
  url.pathname += "console";
  url.search = "";
  url.hash = "";
  return url.href;
}
