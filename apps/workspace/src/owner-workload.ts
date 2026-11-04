import type { WorkItem } from "./contracts/setup";
import { isDue, isStale, today, updatedAt } from "./work-activity";

/** Display-name buckets stay inside their source instance/space. */
export function ownerScope(item: WorkItem) {
  try {
    const url = new URL(item.url);
    const space =
      item.source === "meegle"
        ? `/${url.pathname.split("/").filter(Boolean)[0] ?? ""}`
        : "";
    return `${item.source}:${url.origin}${space}`;
  } catch {
    return `${item.source}:${item.scope_id}`;
  }
}
export function ownerScopeLabel(scope: string) {
  const address = scope.slice(scope.indexOf(":") + 1);
  try {
    const url = new URL(address);
    return `${url.host}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return address;
  }
}
export type OwnerLoad = {
  key: string;
  owner: string;
  scope: string;
  open: number;
  blocked: number;
  stale: number;
  due: number;
  unknown: number;
};
export type OwnerMetric = "open" | "blocked" | "stale" | "due";
export function uniqueWorkItems(items: WorkItem[]) {
  const unique = new Map<string, WorkItem>();
  for (const item of items) {
    const key = JSON.stringify([ownerScope(item), item.url || item.id]);
    const previous = unique.get(key);
    if (
      !previous ||
      (updatedAt(item.updated) ?? 0) > (updatedAt(previous.updated) ?? 0)
    )
      unique.set(key, item);
  }
  return [...unique.values()];
}
export function ownerWorkload(
  items: WorkItem[],
  now = Date.now(),
): OwnerLoad[] {
  const rows = new Map<string, OwnerLoad>();

  const day = today(now);
  for (const item of uniqueWorkItems(items)) {
    if (["done", "cancelled"].includes(item.group)) continue;
    const scope = ownerScope(item);
    const key = JSON.stringify([scope, item.owner]);
    const row = rows.get(key) ?? {
      key,
      owner: item.owner,
      scope,
      open: 0,
      blocked: 0,
      stale: 0,
      due: 0,
      unknown: 0,
    };
    row.open++;
    if (item.group === "blocked") row.blocked++;
    if (isStale(item, now)) row.stale++;
    if (isDue(item, day)) row.due++;
    if (updatedAt(item.activity.last_activity) === null) row.unknown++;
    rows.set(key, row);
  }
  return [...rows.values()].sort(
    (a, b) =>
      b.blocked - a.blocked ||
      b.stale - a.stale ||
      b.due - a.due ||
      b.open - a.open ||
      a.key.localeCompare(b.key),
  );
}
