import type { ModelTab } from "./api";

export type Phase = "development" | "release" | "ota" | "maintenance";
/** Where attention goes first; maintenance folds away on its own. */
export const RUNNING: Phase[] = ["ota", "release", "development"];

/** What a stage of a product's life puts in front of you. */
type Layout = {
  /** The work of this phase, in order. */
  tabs: ModelTab[];
  /** Always reachable, set back. */
  more: ModelTab[];
  /** Release gates lead the header; other phases have their own figures. */
  gates: boolean;
  forecast: boolean;
};

export const layouts: Record<Phase, Layout> = {
  development: {
    tabs: ["overview", "requirements", "issues", "branches", "builds"],
    more: ["checklist", "ota", "settings"],
    gates: false,
    forecast: true,
  },
  release: {
    tabs: ["overview", "issues", "checklist", "builds", "branches"],
    more: ["requirements", "ota", "settings"],
    gates: true,
    forecast: true,
  },
  ota: {
    tabs: ["overview", "ota", "issues", "builds"],
    more: ["branches", "requirements", "checklist", "settings"],
    gates: false,
    forecast: false,
  },
  maintenance: {
    tabs: ["overview", "issues", "builds"],
    more: ["ota", "branches", "requirements", "checklist", "settings"],
    gates: false,
    forecast: false,
  },
};

/** Until a phase is known the page shows everything evenly. */
const undecided: Layout = {
  tabs: [
    "overview",
    "issues",
    "requirements",
    "ota",
    "branches",
    "builds",
    "checklist",
    "settings",
  ],
  more: [],
  gates: true,
  forecast: true,
};

export const layoutOf = (phase?: string): Layout =>
  layouts[phase as Phase] ?? undecided;

/** Whole days until a YYYY-MM-DD day; negative once it has passed. */
export const daysUntil = (day: string, now = new Date()) => {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.round(
    (Date.parse(`${day}T00:00:00`) - today.getTime()) / 86_400_000,
  );
};
