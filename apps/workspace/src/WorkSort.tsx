import { ArrowDownUp, Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { WorkItem } from "./contracts/setup";
import { t } from "./i18n";

/** What a list can be ordered by: times and IDs compare as values, the rest by their order. */
export const sortFields = [
  "activity",
  "updated",
  "created",
  "held",
  "due",
  "severity",
  "stage",
  "status",
  "owner",
  "id",
] as const;
export type SortField = (typeof sortFields)[number];
export type SortKey = { field: SortField; dir: "asc" | "desc" };
export const defaultSort: SortKey[] = [{ field: "activity", dir: "desc" }];

const timed = new Set<SortField>([
  "activity",
  "updated",
  "created",
  "held",
  "due",
]);

// Severity from most to least urgent, across Jira and 飞书项目 wording.
const severityOrder = [
  "致命",
  "blocker",
  "highest",
  "p0",
  "严重",
  "critical",
  "high",
  "p1",
  "一般",
  "major",
  "medium",
  "p2",
  "提示",
  "minor",
  "low",
  "p3",
  "trivial",
  "lowest",
  "p4",
];
const stageOrder = [
  "blocked",
  "pending",
  "active",
  "review",
  "testing",
  "done",
  "cancelled",
  "unknown",
];
function rank(list: string[], value: string) {
  const v = value.trim().toLowerCase();
  const i = list.findIndex((x) => v === x || v.startsWith(x));
  return i < 0 ? list.length : i;
}

/** The sort value of an item: a number for times and ranked fields, text otherwise. */
export function sortValue(
  item: WorkItem,
  field: SortField,
  time: (v: string) => number,
  lastActivity: (i: WorkItem) => string,
): number | string {
  switch (field) {
    case "activity":
      return time(lastActivity(item));
    case "updated":
      return time(item.updated);
    case "created":
      return time(item.activity.created);
    case "held":
      return time(item.activity.status_since || item.updated);
    case "due":
      return item.local?.next_check ? time(item.local.next_check) : Infinity;
    case "severity":
      return rank(severityOrder, item.severity);
    case "stage":
      return rank(stageOrder, item.group);
    case "status":
      return item.status;
    case "owner":
      return item.owner || "￿";
    case "id": {
      const n = /(\d+)$/.exec(item.display_id);
      return n ? Number(n[1]) : item.display_id;
    }
  }
}

/** Compares by each key in turn; the first that differs decides. */
export function compareBy(
  keys: SortKey[],
  value: (i: WorkItem, f: SortField) => number | string,
) {
  return (a: WorkItem, b: WorkItem) => {
    for (const k of keys) {
      const x = value(a, k.field);
      const y = value(b, k.field);
      const c =
        typeof x === "number" && typeof y === "number"
          ? x === y
            ? 0
            : x < y
              ? -1
              : 1
          : String(x).localeCompare(String(y), "zh-CN");
      if (c) return k.dir === "asc" ? c : -c;
    }
    return 0;
  };
}

function dirLabel(field: SortField, dir: "asc" | "desc") {
  return t(
    `work.sortDir.${timed.has(field) || field === "id" ? "value" : "order"}.${dir}`,
  );
}

/** Ordered sort conditions, 飞书项目 style: add, change field or direction, remove. */
export function SortBuilder({
  value,
  onChange,
}: {
  value: SortKey[];
  onChange: (next: SortKey[]) => void;
}) {
  const used = new Set(value.map((k) => k.field));
  const free = sortFields.filter((f) => !used.has(f));
  const set = (i: number, key: SortKey) =>
    onChange(value.map((k, j) => (j === i ? key : k)));
  return (
    <Collapsible className="sort-builder">
      <CollapsibleTrigger className="sort-toggle">
        <ArrowDownUp size={14} aria-hidden="true" />
        {value
          .map(
            (k) =>
              `${t(`work.sortField.${k.field}`)} ${dirLabel(k.field, k.dir)}`,
          )
          .join(" · ") || t("work.sort")}
      </CollapsibleTrigger>
      <CollapsibleContent className="sort-panel">
        <p className="sort-title">{t("work.sortConditions")}</p>
        <ol>
          {value.map((k, i) => (
            <li key={k.field}>
              <span className="sort-rank">{i + 1}</span>
              <Select
                value={k.field}
                items={[k.field, ...free].map((f) => ({
                  value: f,
                  label: t(`work.sortField.${f}`),
                }))}
                onValueChange={(f) =>
                  f &&
                  set(i, {
                    field: f as SortField,
                    dir: timed.has(f as SortField) ? "desc" : "asc",
                  })
                }
              >
                <SelectTrigger size="sm" aria-label={t("work.sortField")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {[k.field, ...free].map((f) => (
                    <SelectItem key={f} value={f}>
                      {t(`work.sortField.${f}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <div className="sort-dir" role="group">
                {(["asc", "desc"] as const).map((d) => (
                  <button
                    type="button"
                    key={d}
                    aria-pressed={k.dir === d}
                    onClick={() => set(i, { ...k, dir: d })}
                  >
                    {dirLabel(k.field, d)}
                  </button>
                ))}
              </div>
              <button
                type="button"
                className="sort-remove"
                aria-label={t("work.sortRemove")}
                disabled={value.length === 1}
                onClick={() => onChange(value.filter((_, j) => j !== i))}
              >
                <X size={14} aria-hidden="true" />
              </button>
            </li>
          ))}
        </ol>
        <div className="sort-actions">
          <Button
            size="sm"
            variant="ghost"
            disabled={!free.length}
            onClick={() =>
              onChange([
                ...value,
                { field: free[0], dir: timed.has(free[0]) ? "desc" : "asc" },
              ])
            }
          >
            <Plus size={14} aria-hidden="true" />
            {t("work.sortAdd")}
          </Button>
          <button
            type="button"
            className="link"
            onClick={() => onChange(defaultSort)}
          >
            {t("work.sortReset")}
          </button>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
