import { QueryClient, queryOptions } from "@tanstack/react-query";
import { focus } from "./api";
import type {
  Builds,
  FocusModel,
  FocusSummary,
  WorkItems,
} from "./contracts/setup";

// One desktop window uses the default SPM configuration root. Keep the scope in
// every key so a future profile picker can create an isolated cache.
export const focusKey = ["spm", "default"] as const;
export const calendarKey = [...focusKey, "calendar"];
export const profilesKey = ["profiles"] as const;
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      networkMode: "always",
      retry: false,
      staleTime: 120_000,
      gcTime: 30 * 60_000,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
    },
  },
});
export const modelsKey = [...focusKey, "models"];
export const summaryKey = [...focusKey, "summary"];
export const modelsOptions = queryOptions({
  queryKey: modelsKey,
  queryFn: async () => (await focus("list")).models ?? [],
});
export const summaryOptions = queryOptions({
  queryKey: summaryKey,
  queryFn: async () => (await focus("cached")).summary ?? { models: [] },
  staleTime: Infinity,
});
const revisions = new Map<string, number>();
export function invalidateModel(code: string) {
  revisions.set(code, (revisions.get(code) ?? 0) + 1);
  void queryClient.invalidateQueries({
    queryKey: [...focusKey, "model", code],
    refetchType: "none",
  });
}
export function updateSummary(
  update: (previous: FocusSummary) => FocusSummary,
) {
  queryClient.setQueryData<FocusSummary>(summaryKey, (previous) =>
    update(previous ?? { models: [] }),
  );
}

// Refresh one model at a time: the CLI merges and rewrites a shared disk cache.
// Publishing each result lets the UI update without waiting for all models.
let refreshQueue: Promise<unknown> = Promise.resolve();
export async function refreshModel(code: string, force = false) {
  const revision = revisions.get(code) ?? 0;
  const model = await queryClient.fetchQuery({
    queryKey: [...focusKey, "model", code, "summary", revision],
    staleTime: force ? 0 : 120_000,
    queryFn: () => {
      const task = refreshQueue
        .catch(() => {})
        .then(async () => {
          if ((revisions.get(code) ?? 0) !== revision) return null;
          const result = await focus("refresh", { code });
          return (
            result.summary?.models.find((model) => model.code === code) ?? null
          );
        });
      refreshQueue = task;
      return task;
    },
  });
  if (!model || (revisions.get(code) ?? 0) !== revision) return;
  const followed = queryClient.getQueryData<FocusModel[]>(modelsKey);
  if (!followed?.some((item) => item.code === code)) return;
  updateSummary((previous) => {
    const reads = followed.map(
      (item) =>
        queryClient.getQueryState([
          ...focusKey,
          "model",
          item.code,
          "summary",
          revisions.get(item.code) ?? 0,
        ])?.dataUpdatedAt ?? Date.parse(previous.computed_at ?? ""),
    );
    const oldest = Math.min(...reads);
    return {
      ...previous,
      ...(Number.isFinite(oldest)
        ? { computed_at: new Date(oldest).toISOString() }
        : {}),
      models: [
        ...previous.models.filter(
          (item) =>
            item.code !== code &&
            followed.some((saved) => saved.code === item.code),
        ),
        model,
      ],
    };
  });
  await queryClient.invalidateQueries({
    queryKey: [...focusKey, "model", code, "changes"],
  });
}

export function buildsOptions(code: string) {
  return queryOptions({
    queryKey: [...focusKey, "model", code, "builds"],
    staleTime: 45_000,
    queryFn: async () => {
      const result = await focus("builds", { code });
      if (!result.builds || result.builds.code !== code)
        throw { code: "spm_invalid_response" };
      return result.builds;
    },
  });
}
export async function prefetchBuilds(code: string) {
  const options = buildsOptions(code);
  if (!queryClient.getQueryData(options.queryKey)) {
    try {
      const cached = await queryClient.fetchQuery({
        queryKey: [...focusKey, "model", code, "builds-disk"],
        queryFn: async () =>
          (await focus("builds_cached", { code })).builds ?? null,
      });
      if (
        cached?.code === code &&
        !queryClient.getQueryData(options.queryKey)
      ) {
        queryClient.setQueryData<Builds>(options.queryKey, cached, {
          updatedAt: Number.isFinite(Date.parse(cached.computed_at))
            ? Math.min(Date.now(), Date.parse(cached.computed_at))
            : 0,
        });
      }
    } catch {
      /* A fresh read below supplies the actionable error. */
    }
  }
  await queryClient.prefetchQuery(options);
}
export function changesOptions(code: string, branch: string | null) {
  return queryOptions({
    queryKey: [...focusKey, "model", code, "changes", branch],
    queryFn: async () =>
      (await focus("changes", { code, branch: branch! })).changes ?? [],
  });
}

export function optimisticReview(
  summary: FocusSummary,
  code: string,
  change: number,
  action: string,
): FocusSummary {
  return {
    ...summary,
    models: summary.models.map((model) =>
      model.code !== code
        ? model
        : {
            ...model,
            flagged:
              action === "accept"
                ? model.flagged.filter((item) => item.number !== change)
                : model.flagged.map((item) =>
                    item.number === change
                      ? { ...item, tracked: action === "track" }
                      : item,
                  ),
          },
    ),
  };
}

export type WorkKind = "issues" | "requirements";
/** Issues and requirements: the disk cache at once, a fresh read when older than 30 minutes. */
export function workItemsOptions(code: string, kind: WorkKind) {
  return queryOptions({
    queryKey: [...focusKey, "model", code, kind],
    staleTime: 30 * 60_000,
    queryFn: async () => {
      const result = await focus(kind, { code });
      if (!result.work_items || result.work_items.code !== code)
        throw { code: "spm_invalid_response" };
      return result.work_items;
    },
  });
}
export async function prefetchWorkItems(code: string, kind: WorkKind) {
  const options = workItemsOptions(code, kind);
  if (!queryClient.getQueryData(options.queryKey)) {
    try {
      const cached = (await focus(`${kind}_cached`, { code })).work_items;
      if (cached?.code === code && !queryClient.getQueryData(options.queryKey))
        queryClient.setQueryData<WorkItems>(options.queryKey, cached, {
          updatedAt: Number.isFinite(Date.parse(cached.checked_at))
            ? Math.min(Date.now(), Date.parse(cached.checked_at))
            : 0,
        });
    } catch {
      /* The fresh read reports the actionable error. */
    }
  }
  await queryClient.prefetchQuery(options);
}
