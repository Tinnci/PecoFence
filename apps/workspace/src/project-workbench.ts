import type { FocusModel, ScopeQuery } from "./contracts/setup";

/** Labels may change; issue scope identity comes from the source's stable IDs. */
function scopeKey(scope: ScopeQuery) {
  return scope.source === "jira"
    ? JSON.stringify([scope.source, scope.project, scope.jql ?? ""])
    : JSON.stringify([
        scope.source,
        scope.space,
        scope.type_key,
        scope.work_item_type,
        scope.filter?.field ?? "",
        scope.filter?.values.map((value) => value.id).sort() ?? [],
      ]);
}

/** Opening a project adds its saved scope without resetting an existing workbench. */
export function projectWorkbench(
  seed: FocusModel,
  existing?: FocusModel,
): FocusModel {
  if (!existing) return seed;
  const scopes = new Map(
    existing.issues.map((scope) => [scopeKey(scope), scope]),
  );
  for (const scope of seed.issues) scopes.set(scopeKey(scope), scope);
  return { ...existing, issues: [...scopes.values()] };
}
