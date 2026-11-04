import { queryOptions } from "@tanstack/react-query";
import type { ItemFlow, WorkItem } from "./contracts/setup";
import { focusKey, queryClient } from "./focus-cache";
import {
  isFlowPrefetch,
  promoteFlow,
  readFlow,
  type FlowPriority,
  type FlowResource,
} from "./flow-queue";

const flowKey = (code: string) =>
  [...focusKey, "model", code, "flows"] as const;
const resourceOf = (code: string, item: WorkItem): FlowResource => ({
  code,
  source: item.source,
  scope: item.scope_id,
  item: item.id,
  address: item.url,
  updated: item.updated,
  closed: ["done", "cancelled"].includes(item.group),
});
const queryKey = (resource: FlowResource) =>
  [...flowKey(resource.code), resource] as const;
const forced = new Set<string>();
const refreshing = new Map<string, Promise<unknown>>();
const speculativeOwners = new Map<string, number>();
const token = (resource: FlowResource) => JSON.stringify(resource);

function options(
  resource: FlowResource,
  priority: FlowPriority = "interactive",
) {
  const lifetime = resource.closed ? 24 * 60 * 60_000 : 5 * 60_000;
  return queryOptions<ItemFlow>({
    queryKey: queryKey(resource),
    meta: { flowResource: resource },
    placeholderData: (previous, previousQuery) => {
      const old = previousQuery?.meta?.flowResource as FlowResource | undefined;
      return old?.address === resource.address &&
        old.source === resource.source &&
        old.scope === resource.scope
        ? previous
        : undefined;
    },
    queryFn: ({ signal }) =>
      readFlow(resource, priority, forced.delete(token(resource)), signal),
    staleTime: (query) => {
      const fetched = Date.parse(query.state.data?.fetched_at ?? "");
      // The disk timestamp retains its original age in memory. Clamp future
      // timestamps so clock adjustments cannot prolong freshness indefinitely.
      return Number.isFinite(fetched)
        ? Math.max(
            0,
            Math.min(fetched, query.state.dataUpdatedAt) +
              lifetime -
              query.state.dataUpdatedAt,
          )
        : lifetime;
    },
  });
}
export function flowOptions(code: string, item: WorkItem) {
  return options(resourceOf(code, item));
}
export function activateFlow(code: string, item: WorkItem) {
  promoteFlow(resourceOf(code, item), "interactive");
}

export async function loadFlows(code: string, items: WorkItem[]) {
  const results = await Promise.allSettled(
    items
      .filter((item) => item.source === "meegle")
      .map((item) => {
        const resource = resourceOf(code, item);
        promoteFlow(resource, "export");
        return queryClient.fetchQuery(options(resource, "export"));
      }),
  );
  const failure = results.find((r) => r.status === "rejected");
  if (failure?.status === "rejected") throw failure.reason;
}

/** Returns a cleanup for abandoned speculative reads. Active rows and promoted
 * export requests retain ownership; cancellation never evicts last-good data. */
export function prefetchFlows(code: string, items: WorkItem[]) {
  const resources = items
    .filter((item) => item.source === "meegle")
    .slice(0, 10)
    .map((item) => resourceOf(code, item));
  for (const resource of resources) {
    const key = token(resource);
    speculativeOwners.set(key, (speculativeOwners.get(key) ?? 0) + 1);
    void queryClient.prefetchQuery(options(resource, "prefetch"));
  }
  let released = false;
  return () => {
    if (released) return;
    released = true;
    for (const resource of resources) {
      const ownerKey = token(resource);
      const remaining = (speculativeOwners.get(ownerKey) ?? 1) - 1;
      if (remaining > 0) {
        speculativeOwners.set(ownerKey, remaining);
        continue;
      }
      speculativeOwners.delete(ownerKey);
      const key = queryKey(resource);
      const query = queryClient
        .getQueryCache()
        .find({ queryKey: key, exact: true });
      if (query && !query.isActive() && isFlowPrefetch(resource)) {
        void queryClient.cancelQueries({ queryKey: key, exact: true });
      }
    }
  };
}

function refreshResource(resource: FlowResource) {
  const key = token(resource);
  const previous = refreshing.get(key);
  if (previous) return previous;
  const task = (async () => {
    // fetchQuery otherwise joins an older in-flight read, defeating force refresh.
    await queryClient.cancelQueries({
      queryKey: queryKey(resource),
      exact: true,
    });
    await queryClient.invalidateQueries({
      queryKey: queryKey(resource),
      exact: true,
      refetchType: "none",
    });
    forced.add(key);
    return queryClient.fetchQuery(options(resource));
  })().finally(() => {
    forced.delete(key);
    refreshing.delete(key);
  });
  refreshing.set(key, task);
  return task;
}

export async function refreshFlows(code: string) {
  await queryClient.invalidateQueries({
    queryKey: flowKey(code),
    refetchType: "none",
  });
  const active = queryClient
    .getQueryCache()
    .findAll({ queryKey: flowKey(code), type: "active" });
  await Promise.allSettled(
    active.map((query) => {
      const resource = query.meta?.flowResource as FlowResource | undefined;
      return resource ? refreshResource(resource) : Promise.resolve();
    }),
  );
}
export function refreshFlow(code: string, item: WorkItem) {
  return refreshResource(resourceOf(code, item));
}
