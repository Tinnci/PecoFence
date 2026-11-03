import { ListFilter, Rows3, X } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { WorkItem } from "./contracts/setup";
import { OwnerChip, SeverityBadge, severityLevel } from "./Badges";
import { t } from "./i18n";

// Filters as Linear shows them: one chip per field with the values chosen, values within a
// field are alternatives, fields together narrow. Every list offers only values present.
export const filterFields = [
  "owner",
  "severity",
  "stage",
  "status",
  "tag",
  "version",
] as const;
export type FilterField = (typeof filterFields)[number];
export type Filters = Partial<Record<FilterField, string[]>>;
export const NONE = "—";

export function valuesOf(item: WorkItem, field: FilterField): string[] {
  switch (field) {
    case "owner":
      return [item.owner || NONE];
    case "severity":
      return [item.severity || NONE];
    case "stage":
      return [item.group];
    case "status":
      return [item.status || NONE];
    case "tag":
      return item.local?.tags.length ? item.local.tags : [NONE];
    case "version":
      return item.activity.found_versions.length
        ? item.activity.found_versions
        : [NONE];
  }
}

export function matches(item: WorkItem, filters: Filters) {
  return filterFields.every((f) => {
    const chosen = filters[f];
    return !chosen?.length || valuesOf(item, f).some((v) => chosen.includes(v));
  });
}

/** Each value of a field with how many items carry it, most common first. */
function tally(items: WorkItem[], field: FilterField) {
  const counts = new Map<string, number>();
  for (const i of items)
    for (const v of new Set(valuesOf(i, field)))
      counts.set(v, (counts.get(v) ?? 0) + 1);
  const list = [...counts];
  // Severity and stage read in their own order; the rest by how common they are.
  if (field === "severity")
    return list.sort(
      (a, b) => (severityLevel(a[0]) ?? 9) - (severityLevel(b[0]) ?? 9),
    );
  return list.sort((a, b) => b[1] - a[1]);
}

export function valueLabel(field: FilterField, v: string) {
  if (v === NONE)
    return field === "owner" ? t("work.unassigned") : t("work.filterEmpty");
  return field === "stage" ? t(`work.stage.${v}`) : v;
}

function ValueMark({ field, value }: { field: FilterField; value: string }) {
  if (field === "owner" && value !== NONE) return <OwnerChip name={value} />;
  if (field === "severity" && value !== NONE)
    return <SeverityBadge value={value} />;
  if (field === "stage")
    return (
      <span className={`status-chip stage-${value}`}>
        {valueLabel(field, value)}
      </span>
    );
  return <span>{valueLabel(field, value)}</span>;
}

function Checklist({
  items,
  field,
  value,
  onChange,
}: {
  items: WorkItem[];
  field: FilterField;
  value: Filters;
  onChange: (next: Filters) => void;
}) {
  const chosen = value[field] ?? [];
  const toggle = (v: string, on: boolean) => {
    const next = on ? [...chosen, v] : chosen.filter((x) => x !== v);
    onChange({ ...value, [field]: next.length ? next : undefined });
  };
  return (
    <>
      {tally(items, field)
        .slice(0, 40)
        .map(([v, n]) => (
          <DropdownMenuCheckboxItem
            key={v}
            checked={chosen.includes(v)}
            closeOnClick={false}
            onCheckedChange={(on) => toggle(v, on)}
          >
            <span className="filter-option">
              <ValueMark field={field} value={v} />
              <span className="filter-count">{n}</span>
            </span>
          </DropdownMenuCheckboxItem>
        ))}
    </>
  );
}

type BarProps = {
  items: WorkItem[];
  value: Filters;
  onChange: (next: Filters) => void;
};

/** The filter button: a field, then its values; it stays where it is as chips appear. */
export function FilterAdd({ items, value, onChange }: BarProps) {
  const active = filterFields.filter((f) => value[f]?.length);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger className="filter-add">
        <ListFilter size={14} aria-hidden="true" />
        {active.length ? null : t("work.filter")}
      </DropdownMenuTrigger>
      <DropdownMenuContent className="filter-menu" align="start">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("work.filterBy")}</DropdownMenuLabel>
          {filterFields.map((f) => (
            <DropdownMenuSub key={f}>
              <DropdownMenuSubTrigger>
                {t(`work.filterField.${f}`)}
                {value[f]?.length ? (
                  <span className="filter-count">{value[f]!.length}</span>
                ) : null}
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="filter-menu">
                <Checklist
                  items={items}
                  field={f}
                  value={value}
                  onChange={onChange}
                />
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          ))}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Active filters as chips on their own row, each reopening its own list. */
export function FilterChips({ items, value, onChange }: BarProps) {
  const active = filterFields.filter((f) => value[f]?.length);
  if (!active.length) return null;
  return (
    <div className="filter-bar">
      {active.map((f) => {
        const chosen = value[f]!;
        return (
          <span key={f} className="filter-chip">
            <DropdownMenu>
              <DropdownMenuTrigger className="filter-chip-body">
                <span className="filter-field">
                  {t(`work.filterField.${f}`)}
                </span>
                <span className="filter-op">
                  {chosen.length > 1
                    ? t("work.filterAnyOf")
                    : t("work.filterIs")}
                </span>
                <span className="filter-values">
                  {chosen
                    .slice(0, 2)
                    .map((v) => valueLabel(f, v))
                    .join("、")}
                  {chosen.length > 2 ? ` +${chosen.length - 2}` : ""}
                </span>
              </DropdownMenuTrigger>
              <DropdownMenuContent className="filter-menu" align="start">
                <Checklist
                  items={items}
                  field={f}
                  value={value}
                  onChange={onChange}
                />
              </DropdownMenuContent>
            </DropdownMenu>
            <button
              type="button"
              className="filter-chip-remove"
              aria-label={t("work.filterRemove", {
                field: t(`work.filterField.${f}`),
              })}
              onClick={() => onChange({ ...value, [f]: undefined })}
            >
              <X size={12} aria-hidden="true" />
            </button>
          </span>
        );
      })}
      {active.length ? (
        <button type="button" className="link" onClick={() => onChange({})}>
          {t("work.filterClear")}
        </button>
      ) : null}
    </div>
  );
}

export const groupFields = [
  "none",
  "owner",
  "severity",
  "stage",
  "status",
] as const;
export type GroupField = (typeof groupFields)[number];

const stageOrder = [
  "blocked",
  "pending",
  "active",
  "review",
  "testing",
  "unknown",
  "done",
  "cancelled",
];

/** Items in groups, each group in the list's order; groups by their field's own order. */
export function groupItems(items: WorkItem[], field: GroupField) {
  if (field === "none") return [{ key: "", items }];
  const map = new Map<string, WorkItem[]>();
  for (const i of items) {
    const k = valuesOf(i, field)[0];
    map.set(k, [...(map.get(k) ?? []), i]);
  }
  const groups = [...map].map(([key, list]) => ({ key, items: list }));
  const rank = (k: string) =>
    field === "severity"
      ? (severityLevel(k) ?? 9)
      : field === "stage"
        ? stageOrder.indexOf(k)
        : 0;
  return groups.sort((a, b) =>
    field === "severity" || field === "stage"
      ? rank(a.key) - rank(b.key)
      : a.key === NONE
        ? 1
        : b.key === NONE
          ? -1
          : b.items.length - a.items.length,
  );
}

export function GroupHeader({
  field,
  value,
  count,
  collapsed,
  onToggle,
}: {
  field: GroupField;
  value: string;
  count: number;
  collapsed: boolean;
  onToggle: () => void;
}) {
  if (field === "none") return null;
  return (
    <li className="work-group">
      <button type="button" aria-expanded={!collapsed} onClick={onToggle}>
        <span className="chevron" data-open={!collapsed} aria-hidden="true">
          ▸
        </span>
        <ValueMark field={field} value={value} />
        <span className="filter-count">{count}</span>
      </button>
    </li>
  );
}

/** How the list is laid out: grouping now, ordering beside it. */
export function DisplayMenu({
  group,
  onGroup,
}: {
  group: GroupField;
  onGroup: (g: GroupField) => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger className="filter-add">
        <Rows3 size={14} aria-hidden="true" />
        {group === "none"
          ? t("work.group")
          : t("work.groupedBy", { field: t(`work.groupField.${group}`) })}
      </DropdownMenuTrigger>
      <DropdownMenuContent className="filter-menu" align="start">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("work.group")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={group}
            onValueChange={(g) => onGroup(g as GroupField)}
          >
            {groupFields.map((g) => (
              <DropdownMenuRadioItem key={g} value={g}>
                {t(`work.groupField.${g}`)}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
