export type CheckStatus = "pending" | "passed" | "blocked" | "na";
export type CheckPhase = "plan" | "release";
export type CheckEntry = {
  status: CheckStatus;
  owner: string;
  note: string;
  evidence: string;
  updatedAt: string;
};
export type ReleaseCheck = {
  id: string;
  name: string;
  baseline: string;
  branch: string;
  target: string;
  createdAt: string;
  entries: Record<string, CheckEntry>;
};
export const checkTemplates = [
  { id: "scope", phase: "plan", priority: "P0" },
  { id: "branch", phase: "plan", priority: "P0" },
  { id: "common", phase: "plan", priority: "P0" },
  { id: "certification", phase: "plan", priority: "P0" },
  { id: "dependencies", phase: "plan", priority: "P1" },
  { id: "schedule", phase: "plan", priority: "P1" },
  { id: "commits", phase: "release", priority: "P0" },
  { id: "apps", phase: "release", priority: "P0" },
  { id: "stability", phase: "release", priority: "P1" },
  { id: "upgrade", phase: "release", priority: "P0" },
  { id: "artifacts", phase: "release", priority: "P0" },
  { id: "approval", phase: "release", priority: "P0" },
  { id: "customer", phase: "release", priority: "P0" },
  { id: "handoff", phase: "release", priority: "P1" },
] as const;
export type CheckTemplate = (typeof checkTemplates)[number];
export const emptyEntry = (): CheckEntry => ({
  status: "pending",
  owner: "",
  note: "",
  evidence: "",
  updatedAt: "",
});
export function confirmed(entry?: CheckEntry) {
  return (
    !!entry &&
    !!entry.owner.trim() &&
    ((entry.status === "passed" &&
      !!entry.note.trim() &&
      !!entry.evidence.trim()) ||
      (entry.status === "na" && !!entry.note.trim()))
  );
}
export function checkCounts(release: ReleaseCheck, phase?: CheckPhase) {
  const items = checkTemplates.filter((item) => !phase || item.phase === phase);
  const done = items.filter((item) =>
    confirmed(release.entries[item.id]),
  ).length;
  const blocked = items.filter(
    (item) => release.entries[item.id]?.status === "blocked",
  ).length;
  return {
    total: items.length,
    done,
    blocked,
    pending: items.length - done - blocked,
  };
}
export const releaseStorageKey = (code: string) =>
  `tinnci.release-checklist.v1.${encodeURIComponent(code)}`;
const text = (v: unknown, max = 12000) =>
  typeof v === "string" ? v.slice(0, max) : "";
const object = (v: unknown): Record<string, unknown> => {
  if (!v || typeof v !== "object" || Array.isArray(v))
    throw new Error("invalid_checklist");
  return v as Record<string, unknown>;
};
/** Refuse unknown/corrupt storage rather than replacing the user's saved checks. */
export function readReleaseChecks(code: string): ReleaseCheck[] {
  const raw = localStorage.getItem(releaseStorageKey(code));
  if (!raw) return [];
  const store = object(JSON.parse(raw));
  if (store.version !== 1 || !Array.isArray(store.releases))
    throw new Error("invalid_checklist");
  const ids = new Set<string>();
  return store.releases.map((value) => {
    const r = object(value);
    const id = text(r.id, 100);
    if (!id || ids.has(id) || !text(r.name, 120))
      throw new Error("invalid_checklist");
    ids.add(id);
    const entries: Record<string, CheckEntry> = {};
    const saved = object(r.entries);
    for (const item of checkTemplates) {
      if (!saved[item.id]) continue;
      const e = object(saved[item.id]);
      const status = ["pending", "passed", "blocked", "na"].includes(
        String(e.status),
      )
        ? (e.status as CheckStatus)
        : "pending";
      const entry = {
        status,
        owner: text(e.owner, 120),
        note: text(e.note),
        evidence: text(e.evidence, 2000),
        updatedAt: text(e.updatedAt, 40),
      };
      if ((status === "passed" || status === "na") && !confirmed(entry))
        entry.status = "pending";
      entries[item.id] = entry;
    }
    return {
      id,
      name: text(r.name, 120),
      baseline: text(r.baseline, 500),
      branch: text(r.branch, 500),
      target: text(r.target, 40),
      createdAt: text(r.createdAt, 40),
      entries,
    };
  });
}
export function writeReleaseChecks(code: string, releases: ReleaseCheck[]) {
  localStorage.setItem(
    releaseStorageKey(code),
    JSON.stringify({ version: 1, releases }),
  );
}
