import { useQuery } from "@tanstack/react-query";
import type { Dispatch, SetStateAction } from "react";
import { focusKey, queryClient } from "./focus-cache";
import {
  normalizeView,
  preferenceField,
  readViewStore,
  saveViewStore,
  type ViewSettings,
} from "./view-storage";

// Work-list preferences have a versioned disk representation; Query notifies subscribers.
export function useModelPreference<T>(
  code: string,
  name: string,
  fallback: T,
): [T, Dispatch<SetStateAction<T>>] {
  const queryKey = [...focusKey, "view", code, name];
  const query = useQuery(
    {
      queryKey,
      queryFn: () => fallback,
      initialData: () => {
        const setting = preferenceField(name);
        return setting
          ? (readViewStore(code, setting.kind).current[setting.field] as T)
          : fallback;
      },
      enabled: false,
      staleTime: Infinity,
      gcTime: Infinity,
    },
    queryClient,
  );
  return [
    (query.data as T) ?? fallback,
    (value) => {
      queryClient.setQueryData<T>(queryKey, (previous) => {
        const next =
          typeof value === "function"
            ? (value as (old: T) => T)(previous ?? fallback)
            : value;
        const setting = preferenceField(name);
        if (!setting) return next;
        const current = normalizeView({
          ...readViewStore(code, setting.kind).current,
          [setting.field]: next,
        });
        saveViewStore(code, setting.kind, { current });
        return current[setting.field] as T;
      });
    },
  ];
}

/** Apply one validated snapshot to disk before publishing its fields to the UI. */
export function applyWorkView(code: string, kind: string, input: ViewSettings) {
  const current = normalizeView(input);
  saveViewStore(code, kind, { current });
  for (const [field, value] of Object.entries(current)) {
    queryClient.setQueryData(
      [...focusKey, "view", code, `${kind}-${field}`],
      value,
    );
  }
}
