import type { Filters, GroupField } from "./WorkFilters";
import type { SortKey } from "./WorkSort";

export type ViewSettings = {
  filters: Filters;
  group: GroupField;
  sort: SortKey[];
  stage: string;
  attention: string;
  origin: string;
  search: string;
};
export type SavedView = ViewSettings & { id: string; name: string };
type ViewStore = {
  version: 2;
  current: ViewSettings;
  views: SavedView[];
  selected: string | null;
};
const memory = new Map<string, ViewStore>();
const failures = new Set<string>();
const locked = new Set<string>();
const keyOf = (code: string, kind: string) =>
  `tinnci.workviews.v2.${code}.${kind}`;
const groups = ["none", "owner", "severity", "stage", "status"];
const stages = [
  "all",
  "open",
  "pending",
  "active",
  "review",
  "testing",
  "blocked",
  "unknown",
  "done",
  "cancelled",
];
const sortFields = [
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
];
const filterFields = [
  "owner",
  "severity",
  "stage",
  "status",
  "tag",
  "version",
] as const;
const object = (value: unknown): Record<string, unknown> =>
  value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};

export function normalizeView(value: unknown): ViewSettings {
  const input = object(value);
  const filters: Filters = {};
  const rawFilters = object(input.filters);
  for (const field of filterFields) {
    const values = rawFilters[field];
    if (Array.isArray(values)) {
      const valid = [
        ...new Set(
          values.filter(
            (v): v is string => typeof v === "string" && v.length <= 500,
          ),
        ),
      ]
        .slice(0, 100)
        .sort();
      if (valid.length) filters[field] = valid;
    }
  }
  const sort = Array.isArray(input.sort)
    ? input.sort
        .filter((entry): entry is SortKey => {
          const v = object(entry);
          return (
            sortFields.includes(String(v.field)) &&
            ["asc", "desc"].includes(String(v.dir))
          );
        })
        .slice(0, 10)
        .map(({ field, dir }) => ({ field, dir }))
    : [];
  const attention =
    typeof input.attention === "string" ? input.attention.slice(0, 200) : "";
  return {
    filters,
    group: groups.includes(String(input.group))
      ? (input.group as GroupField)
      : "none",
    sort: sort.length ? sort : [{ field: "activity", dir: "desc" }],
    stage: stages.includes(String(input.stage)) ? String(input.stage) : "open",
    attention:
      ["", "blocked", "stale", "due", "corrected", "conflict"].includes(
        attention,
      ) || attention.startsWith("tag:")
        ? attention
        : "",
    origin: ["all", "internet", "customer", "internal"].includes(
      String(input.origin),
    )
      ? String(input.origin)
      : "all",
    search: typeof input.search === "string" ? input.search.slice(0, 200) : "",
  };
}
function normalizeViews(value: unknown): SavedView[] {
  if (!Array.isArray(value)) return [];
  const ids = new Set<string>();
  return value.slice(0, 50).flatMap((entry) => {
    const v = object(entry);
    if (typeof v.name !== "string" || !v.name.trim()) return [];
    let id =
      typeof v.id === "string" && v.id.length > 0 && v.id.length <= 100
        ? v.id
        : crypto.randomUUID();
    if (ids.has(id)) id = crypto.randomUUID();
    ids.add(id);
    return [{ ...normalizeView(v), id, name: v.name.trim().slice(0, 80) }];
  });
}
function write(code: string, kind: string, store: ViewStore) {
  const key = keyOf(code, kind);
  memory.set(key, store);
  try {
    if (locked.has(key)) return false;
    localStorage.setItem(key, JSON.stringify(store));
    failures.delete(key);
    return true;
  } catch {
    failures.add(key);
    return false;
  }
}
export function readViewStore(code: string, kind: string): ViewStore {
  const key = keyOf(code, kind);
  const cached = memory.get(key);
  if (cached) return cached;
  let store: ViewStore = {
    version: 2,
    current: normalizeView({}),
    views: [],
    selected: null,
  };
  try {
    const raw = localStorage.getItem(key);
    if (raw) {
      if (raw.length > 1_000_000) throw new Error("oversized view storage");
      const input = object(JSON.parse(raw));
      if (input.version !== 2) throw new Error("unsupported view storage");
      const views = normalizeViews(input.views);
      store = {
        version: 2,
        current: normalizeView(input.current),
        views,
        selected: views.some((v) => v.id === input.selected)
          ? String(input.selected)
          : null,
      };
    } else {
      const legacy = localStorage.getItem(`tinnci.views.v1.${code}.${kind}`);
      if (legacy && legacy.length <= 1_000_000) {
        store.views = normalizeViews(JSON.parse(legacy));
        write(code, kind, store);
      }
    }
  } catch {
    failures.add(key);
    locked.add(key);
  }
  memory.set(key, store);
  return store;
}
export function saveViewStore(
  code: string,
  kind: string,
  change: Partial<Omit<ViewStore, "version">>,
) {
  return write(code, kind, {
    ...readViewStore(code, kind),
    ...change,
    version: 2,
  });
}
export const viewStorageFailed = (code: string, kind: string) =>
  failures.has(keyOf(code, kind));
export const sameView = (a: unknown, b: unknown) =>
  JSON.stringify(normalizeView(a)) === JSON.stringify(normalizeView(b));

export function preferenceField(name: string) {
  const match =
    /^(issues|requirements)-(stage|attention|origin|search|sort|filters|group)$/.exec(
      name,
    );
  return match
    ? { kind: match[1], field: match[2] as keyof ViewSettings }
    : null;
}
