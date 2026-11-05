import { locale } from "./i18n";

const shortParts: Intl.DateTimeFormatOptions = {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
};

/** One way to show a moment across the workspace: month, day and minutes in the current
 * language, with the year only when it is not this one. Unreadable values show as "—". */
export function shortTime(at: string | number | null | undefined) {
  if (at === null || at === undefined || at === "") return "—";
  const ms = typeof at === "number" ? at : Date.parse(at);
  if (!Number.isFinite(ms)) return typeof at === "string" ? at : "—";
  const when = new Date(ms);
  return when.toLocaleString(
    locale(),
    when.getFullYear() === new Date().getFullYear()
      ? shortParts
      : { ...shortParts, year: "numeric" },
  );
}
