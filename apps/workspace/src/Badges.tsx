import { Clock, GitCommitHorizontal, RotateCcw } from "lucide-react";
import { t } from "./i18n";

// Severity wording across Jira and 飞书项目, grouped into four levels, most urgent first.
const levels: string[][] = [
  ["致命", "blocker", "highest", "p0", "fatal"],
  ["严重", "critical", "high", "p1", "severe"],
  ["一般", "major", "medium", "p2", "normal"],
  ["提示", "minor", "low", "p3", "trivial", "lowest", "p4"],
];
/** 0 fatal … 3 minor; null when the wording is unknown. */
export function severityLevel(value: string): number | null {
  const v = value.trim().toLowerCase();
  if (!v) return null;
  const i = levels.findIndex((words) =>
    words.some((w) => v === w || v.startsWith(w) || v.includes(w)),
  );
  return i < 0 ? null : i;
}

/** Severity as a badge: bars for the level, colour for urgency, the source's own word. */
export function SeverityBadge({ value }: { value: string }) {
  if (!value) return null;
  const level = severityLevel(value);
  const bars = level === null ? 0 : 4 - level;
  return (
    <span
      className={`severity-badge severity-${level ?? "unknown"}`}
      title={t("work.sortField.severity")}
    >
      <span className="severity-bars" aria-hidden="true">
        {[1, 2, 3, 4].map((b) => (
          <span key={b} className={b <= bars ? "on" : undefined} />
        ))}
      </span>
      {value}
    </span>
  );
}

function hue(name: string) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.codePointAt(0)!) % 360;
  return h;
}
/** The initial a person is recognised by: the first Han character, else the first letter. */
function initial(name: string) {
  const han = /\p{Script=Han}/u.exec(name);
  return (han?.[0] ?? name.trim()[0] ?? "?").toUpperCase();
}

/** A person with a steady colour, so the same owner reads the same everywhere. */
export function OwnerChip({ name }: { name: string }) {
  if (!name)
    return (
      <span className="owner-chip owner-none">
        <span className="owner-avatar" aria-hidden="true" />
        {t("work.unassigned")}
      </span>
    );
  return (
    <span className="owner-chip" title={name}>
      <span
        className="owner-avatar"
        aria-hidden="true"
        style={{ "--owner-hue": hue(name) } as React.CSSProperties}
      >
        {initial(name)}
      </span>
      <span className="owner-name">{name}</span>
    </span>
  );
}

/** How long something has waited: neutral, then amber at a week, red at two. */
export function AgeChip({
  days,
  children,
}: {
  days: number;
  children: React.ReactNode;
}) {
  const tone = days >= 14 ? "old" : days >= 7 ? "aging" : "fresh";
  return (
    <span className={`age-chip age-${tone}`}>
      <Clock size={11} aria-hidden="true" />
      {children}
    </span>
  );
}

export function ReopenedChip({ count }: { count: number }) {
  return (
    <span className="reopened-chip">
      <RotateCcw size={11} aria-hidden="true" />
      {t("work.reopened", { count })}
    </span>
  );
}

export function CommitsChip({ numbers }: { numbers: number[] }) {
  return (
    <span
      className="commits-chip"
      title={numbers.map((n) => `#${n}`).join(" ")}
    >
      <GitCommitHorizontal size={12} aria-hidden="true" />
      {t("work.commits", { count: numbers.length })}
    </span>
  );
}
