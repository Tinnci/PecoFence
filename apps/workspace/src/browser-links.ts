import { browserUrl } from "./browser-urls";
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

/** Derive a Gerrit search only from a canonical change URL, retaining context paths. */
export function gerritSearchUrl(
  sample: string | undefined,
  query: string,
): string | null {
  const safe = browserUrl(sample);
  if (!safe) return null;
  const url = new URL(safe);
  const modern = url.pathname.indexOf("/c/");
  if (modern >= 0) url.pathname = url.pathname.slice(0, modern + 1);
  else if (!/^#\/(?:c\/)?\d+(?:\/|$)/.test(url.hash)) {
    if (!/\/\d+\/?$/.test(url.pathname)) return null;
    url.pathname = url.pathname.replace(/\d+\/?$/, "");
  }
  url.search = "";
  url.hash = "";
  return `${url.href}q/${encodeURIComponent(query)}`;
}
export function gerritTerm(field: string, value: string): string {
  return `${field}:"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

/** The existing engineer tool on the same Jenkins origin/context path. */
export function branchQueryBrowserUrl(run?: { url?: string }) {
  const url = browserUrl(run?.url);
  if (!url) return undefined;
  const parsed = new URL(url);
  const start = parsed.pathname.indexOf("/job/");
  if (start < 0) return undefined;
  parsed.pathname =
    parsed.pathname.slice(0, start) +
    "/job/SCM-Server/job/Engineer-tools/job/Branch_query_tool/";
  parsed.search = "";
  parsed.hash = "";
  return parsed.href;
}
