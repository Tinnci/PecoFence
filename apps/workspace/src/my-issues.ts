import type { MyIssue } from "./contracts/setup";
import { FLOW_COLUMNS } from "./flow";

export const BANDS = ["urgent", "high", "normal"] as const;
export type Band = (typeof BANDS)[number];
export const LENSES = ["rank", "board", "model", "reason"] as const;
export type Lens = (typeof LENSES)[number];

export function bandOf(score: number): Band {
  return score >= 60 ? "urgent" : score >= 40 ? "high" : "normal";
}

/** One piece of work: a change tracked on several branches reads as one row. */
export type Work = { lead: MyIssue; items: MyIssue[] };

/** Keeps the rank order; the highest-scored copy leads. */
export function mergeWork(items: MyIssue[]): Work[] {
  const works = new Map<string, Work>();
  for (const item of items) {
    const key = `${item.source}:${item.title.trim().toLowerCase()}`;
    const work = works.get(key);
    if (work) work.items.push(item);
    else works.set(key, { lead: item, items: [item] });
  }
  return [...works.values()];
}

export function bandCounts(works: Work[]) {
  const counts: Record<Band, number> = { urgent: 0, high: 0, normal: 0 };
  for (const work of works) counts[bandOf(work.lead.score)]++;
  return counts;
}

/** Work by workflow column, most urgent first; a status not yet read counts as pending. */
export function workColumns(works: Work[]) {
  const columns = new Map<string, Work[]>(FLOW_COLUMNS.map((c) => [c, []]));
  for (const work of works)
    (columns.get(work.lead.group) ?? columns.get("pending"))?.push(work);
  return [...columns.entries()].filter(([, list]) => list.length);
}

/** Why an item is here beyond how severe it is. */
function mainReason(item: MyIssue) {
  return (
    item.reasons.find((r) => r.key !== "severity")?.key ??
    item.reasons[0]?.key ??
    "severity"
  );
}

export type WorkGroup = { key: string; works: Work[]; top: number };

/** Groups by model or main reason, the group holding the most urgent work first. */
export function groupWork(works: Work[], lens: Lens): WorkGroup[] {
  if (lens === "rank" || lens === "board") return [];
  const groups = new Map<string, Work[]>();
  for (const work of works) {
    const key = lens === "model" ? work.lead.model : mainReason(work.lead);
    groups.set(key, [...(groups.get(key) ?? []), work]);
  }
  return [...groups.entries()]
    .map(([key, list]) => ({
      key,
      works: list,
      top: Math.max(...list.map((w) => w.lead.score)),
    }))
    .sort((a, b) => b.top - a.top || b.works.length - a.works.length);
}
