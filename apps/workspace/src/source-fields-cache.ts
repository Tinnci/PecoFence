import { queryOptions } from "@tanstack/react-query";
import { focus } from "./api";
import { focusKey, queryClient } from "./focus-cache";
import type { SourceFields, WorkItem } from "./contracts/setup";

type Resource = {
  code: string;
  item: string;
  address: string;
  scope: string;
  updated: string;
};
const prefix = (code: string) =>
  [...focusKey, "model", code, "source-fields"] as const;
const resourceOf = (code: string, item: WorkItem): Resource => ({
  code,
  item: item.id,
  address: item.url,
  scope: item.scope_id,
  updated: item.updated,
});
const keyOf = (resource: Resource) =>
  [...prefix(resource.code), resource] as const;
const forced = new Set<string>();
const refreshing = new Map<string, Promise<unknown>>();
function options(resource: Resource) {
  return queryOptions<SourceFields>({
    queryKey: keyOf(resource),
    meta: { sourceFieldResource: resource },
    queryFn: async ({ signal }) => {
      const refresh = forced.delete(JSON.stringify(resource));
      const result = await focus(
        refresh ? "source_fields_refresh" : "source_fields",
        {
          code: resource.code,
          item: resource.item,
          addresses: [resource.address],
        },
      );
      signal.throwIfAborted();
      if (
        !result.source_fields ||
        result.source_fields.item !== resource.item ||
        result.source_fields.address !== resource.address
      )
        throw { code: "spm_invalid_response" };
      return result.source_fields;
    },
    placeholderData: (previous, query) => {
      const old = query?.meta?.sourceFieldResource as Resource | undefined;
      return old?.address === resource.address && old.scope === resource.scope
        ? previous
        : undefined;
    },
    staleTime: (query) => {
      const fetched = Date.parse(query.state.data?.fetched_at ?? "");
      return Number.isFinite(fetched)
        ? Math.max(
            0,
            Math.min(fetched, query.state.dataUpdatedAt) +
              300_000 -
              query.state.dataUpdatedAt,
          )
        : 0;
    },
  });
}
export const sourceFieldsOptions = (code: string, item: WorkItem) =>
  options(resourceOf(code, item));
function refreshResource(resource: Resource) {
  const token = JSON.stringify(resource);
  const existing = refreshing.get(token);
  if (existing) return existing;
  const task = (async () => {
    await queryClient.cancelQueries({ queryKey: keyOf(resource), exact: true });
    await queryClient.invalidateQueries({
      queryKey: keyOf(resource),
      exact: true,
      refetchType: "none",
    });
    forced.add(token);
    return queryClient.fetchQuery(options(resource));
  })().finally(() => {
    forced.delete(token);
    refreshing.delete(token);
  });
  refreshing.set(token, task);
  return task;
}
export const refreshSourceFieldsItem = (code: string, item: WorkItem) =>
  refreshResource(resourceOf(code, item));
export async function refreshSourceFields(code: string) {
  await queryClient.invalidateQueries({
    queryKey: prefix(code),
    refetchType: "none",
  });
  const active = queryClient
    .getQueryCache()
    .findAll({ queryKey: prefix(code), type: "active" });
  await Promise.allSettled(
    active.map((query) => {
      const resource = query.meta?.sourceFieldResource as Resource | undefined;
      return resource ? refreshResource(resource) : Promise.resolve();
    }),
  );
}
