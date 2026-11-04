import type { ItemFlow } from "./contracts/setup";
import { focus } from "./api";

export type FlowResource = {
  code: string;
  source: string;
  scope: string;
  item: string;
  address: string;
  updated: string;
  closed: boolean;
};
export type FlowPriority = "interactive" | "export" | "prefetch";
const rank = { interactive: 0, export: 1, prefetch: 2 } as const;
const resourceKey = (resource: FlowResource) => JSON.stringify(resource);
type Request = {
  resource: FlowResource;
  priority: FlowPriority;
  force: boolean;
  cancelled: boolean;
  resolve: (flow: ItemFlow) => void;
  reject: (error: unknown) => void;
};
const pending: Request[] = [];
const requests = new Map<string, Request>();
let running = false;

export function promoteFlow(resource: FlowResource, priority: FlowPriority) {
  const request = requests.get(resourceKey(resource));
  if (request && rank[priority] < rank[request.priority])
    request.priority = priority;
}
export function isFlowPrefetch(resource: FlowResource) {
  return requests.get(resourceKey(resource))?.priority === "prefetch";
}

// One batch at a time caps source concurrency at ten. Interactive requests move
// ahead of queued exports and speculative reads after the current batch finishes.
async function drain() {
  if (running) return;
  running = true;
  try {
    while (pending.length) {
      pending.sort((a, b) => rank[a.priority] - rank[b.priority]);
      const first = pending[0];
      const batch: Request[] = [];
      for (let i = 0; i < pending.length && batch.length < 10;) {
        const request = pending[i];
        if (
          request.resource.code === first.resource.code &&
          request.force === first.force
        ) {
          batch.push(...pending.splice(i, 1));
        } else i++;
      }
      try {
        const result = await focus(first.force ? "flows_refresh" : "flows", {
          code: first.resource.code,
          items: batch.map((r) => r.resource.item),
          addresses: batch.map((r) => r.resource.address),
        });
        const flows = result.flows ?? (result.flow ? [result.flow] : []);
        for (const request of batch) {
          if (request.cancelled) continue;
          const { item, address } = request.resource;
          const sameId = batch.filter((r) => r.resource.item === item);
          const flow = flows.find(
            (f) =>
              f.item === item &&
              (f.address === address || (!f.address && sameId.length === 1)),
          );
          const error = result.flow_errors?.find(
            (f) => f.item === item && (!f.address || f.address === address),
          );
          if (flow) request.resolve(flow);
          else request.reject({ code: error?.code ?? "spm_invalid_response" });
        }
      } catch (error) {
        for (const request of batch)
          if (!request.cancelled) request.reject(error);
      } finally {
        for (const request of batch) {
          const key = resourceKey(request.resource);
          if (requests.get(key) === request) requests.delete(key);
        }
      }
    }
  } finally {
    running = false;
  }
}

export function readFlow(
  resource: FlowResource,
  priority: FlowPriority,
  force: boolean,
  signal: AbortSignal,
) {
  return new Promise<ItemFlow>((resolve, reject) => {
    if (signal.aborted)
      return reject(new DOMException("Cancelled", "AbortError"));
    const finish = (action: () => void) => {
      signal.removeEventListener("abort", cancel);
      action();
    };
    const request: Request = {
      resource,
      priority,
      force,
      cancelled: false,
      resolve: (flow) => finish(() => resolve(flow)),
      reject: (error) => finish(() => reject(error)),
    };
    const cancel = () => {
      request.cancelled = true;
      const at = pending.indexOf(request);
      if (at >= 0) pending.splice(at, 1);
      if (requests.get(resourceKey(resource)) === request)
        requests.delete(resourceKey(resource));
      request.reject(new DOMException("Cancelled", "AbortError"));
    };
    signal.addEventListener("abort", cancel, { once: true });
    requests.set(resourceKey(resource), request);
    pending.push(request);
    queueMicrotask(() => void drain());
  });
}
