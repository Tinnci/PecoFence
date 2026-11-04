import type { WorkItem } from "./contracts/setup";
import { readableText } from "./work-text";

export const normalizeSearch = (value: string) =>
  value.normalize("NFKC").toLowerCase();

/** Spaces combine keywords; quotes keep a phrase together. */
export function searchTerms(query: string): string[] {
  return (normalizeSearch(query).match(/"[^"]+"|\S+/g) ?? []).map((term) =>
    term.replace(/^"|"$/g, ""),
  );
}

/** Index list data once per snapshot, without fetching every item's detail/history. */
export function workSearchText(item: WorkItem): string {
  const a = item.activity;
  return normalizeSearch(
    [
      item.id,
      item.display_id,
      item.title,
      item.url,
      item.owner,
      item.status,
      item.severity,
      item.resolution,
      item.origin_evidence,
      ...a.found_versions,
      ...a.target_versions,
      readableText(a.progress),
      a.last_comment?.by,
      readableText(a.last_comment?.text ?? ""),
      ...(item.local?.tags ?? []),
      ...(item.local?.notes.map((note) => readableText(note.text)) ?? []),
      ...item.commits.map(String),
    ]
      .filter(Boolean)
      .join(" "),
  );
}

export function matchesSearch(text: string, terms: string[]): boolean {
  return terms.every((term) => text.includes(term));
}
