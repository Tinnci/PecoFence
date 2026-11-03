import { queryOptions } from "@tanstack/react-query";
import type { ItemFlow, WorkItem } from "./contracts/setup";
import { focus } from "./api";
import { focusKey, queryClient } from "./focus-cache";

const flowKey = (code: string) =>
  [...focusKey, "model", code, "flows"] as const;
const ttl = (item: WorkItem) =>
  ["done", "cancelled"].includes(item.group) ? 24 * 60 * 60_000 : 5 * 60_000;

type Request = {
  code: string;
  item: string;
  force: boolean;
  resolve: (flow: ItemFlow) => void;
  reject: (error: unknown) => void;
};
const pending: Request[] = [];
let running = false;

// All consumers share one queue. At most ten source reads are in flight, even
// when hovering, opening rows and exporting overlap.
async function drain() {
  if (running) return;
  running = true;
  try {
    while (pending.length) {
      const first = pending[0];
      const batch: Request[] = [];
      for (let i = 0; i < pending.length && batch.length < 10;) {
        const request = pending[i];
        if (request.code === first.code && request.force === first.force) {
          batch.push(...pending.splice(i, 1));
        } else i++;
      }
      try {
        const result = await focus(first.force ? "flows_refresh" : "flows", {
          code: first.code,
          items: [...new Set(batch.map((r) => r.item))],
        });
        // The CLI uses `flow` for a one-item batch and `flows` for several.
        const flows = result.flows ?? (result.flow ? [result.flow] : []);
        for (const request of batch) {
          const flow = flows.find((f) => f.item === request.item);
          if (flow) request.resolve(flow);
          else request.reject({ code: "spm_invalid_response" });
        }
      } catch (error) {
        for (const request of batch) request.reject(error);
      }
    }
  } finally {
    running = false;
  }
}

function read(code: string, item: string, force = false) {
  return new Promise<ItemFlow>((resolve, reject) => {
    pending.push({ code, item, force, resolve, reject });
    queueMicrotask(() => void drain());
  });
}

export function flowOptions(code: string, item: WorkItem) {
  return queryOptions({
    queryKey: [...flowKey(code), item.scope_id, item.id, item.updated],
    queryFn: () => read(code, item.id),
    // A disk-cache hit must not acquire another full memory-cache lifetime.
    staleTime: (query) => {
      const fetched = Date.parse(query.state.data?.fetched_at ?? "");
      return Number.isFinite(fetched)
        ? Math.max(0, fetched + ttl(item) - query.state.dataUpdatedAt)
        : ttl(item);
    },
  });
}

export async function loadFlows(code: string, items: WorkItem[]) {
  // fetchQuery handles freshness, in-flight deduplication and error state for
  // each row; the queue handles batching. Export waits for every requested row.
  const results = await Promise.allSettled(
    items
      .filter((item) => item.source === "meegle")
      .map((item) => queryClient.fetchQuery(flowOptions(code, item))),
  );
  const failure = results.find((r) => r.status === "rejected");
  if (failure?.status === "rejected") throw failure.reason;
}

export function prefetchFlows(code: string, items: WorkItem[]) {
  void loadFlows(code, items.slice(0, 10)).catch(() => undefined);
}

export async function refreshFlows(code: string) {
  await queryClient.invalidateQueries({
    queryKey: flowKey(code),
    refetchType: "none",
  });
  // Refresh only visible/open histories; closed rows stay invalidated until used.
  const active = queryClient
    .getQueryCache()
    .findAll({ queryKey: flowKey(code), type: "active" });
  await Promise.allSettled(
    active.map((query) =>
      queryClient.fetchQuery({
        queryKey: query.queryKey,
        staleTime: 0,
        queryFn: () => read(code, String(query.queryKey.at(-2)), true),
      }),
    ),
  );
}

export function refreshFlow(code: string, item: WorkItem) {
  return queryClient.fetchQuery({
    ...flowOptions(code, item),
    staleTime: 0,
    queryFn: () => read(code, item.id, true),
  });
}
