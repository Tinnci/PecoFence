import type { WorkItem } from "./contracts/setup";
export const STALE_DAYS = 14;
const closed = new Set(["done", "cancelled"]);

/** Jira writes ISO times with +0800; SPM writes 飞书项目 times as RFC 3339 (+08:00), and older
 * reads kept its zoneless China time ("2026-11-02 13:51:04", "2026-10-30"). */
export function updatedAt(value: string) {
  if (typeof value !== "string") return null;
  if (/^\d{10,}$/.test(value)) return Number(value);
  const china = /^(\d{4}-\d\d-\d\d)(?:[ T](\d\d:\d\d(?::\d\d)?))?$/.exec(
    value.trim(),
  );
  const text = china
    ? `${china[1]}T${china[2] ?? "00:00:00"}+08:00`
    : value.replace(/([+-]\d\d)(\d\d)$/, "$1:$2");
  const parsed = Date.parse(text);
  return Number.isFinite(parsed) ? parsed : null;
}
export function ageDays(value: string, now = Date.now()) {
  const at = updatedAt(value);
  return at === null ? null : Math.floor((now - at) / 86_400_000);
}
/** The last status change or human comment; `updated` moves on any edit, bots included. */
export const lastActivity = (item: WorkItem) =>
  item.activity.last_activity || item.updated;
export const isStale = (item: WorkItem, now = Date.now()) =>
  !closed.has(item.group) &&
  (ageDays(item.activity.last_activity, now) ?? 0) >= STALE_DAYS;
export const today = (now = Date.now()) =>
  new Date(now).toLocaleDateString("sv-SE", { timeZone: "Asia/Shanghai" });
export const isDue = (item: WorkItem, day = today()) =>
  !!item.local?.next_check &&
  /^\d{4}-\d{2}-\d{2}$/.test(item.local.next_check.slice(0, 10)) &&
  item.local.next_check.slice(0, 10) <= day;
