import { useQuery } from "@tanstack/react-query";
import type { Dispatch, SetStateAction } from "react";
import { focusKey, queryClient } from "./focus-cache";

// View state stays in memory with the data; none of it contains translated text.
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
      enabled: false,
      staleTime: Infinity,
      gcTime: Infinity,
    },
    queryClient,
  );
  return [
    (query.data as T) ?? fallback,
    (value) => {
      queryClient.setQueryData<T>(queryKey, (previous) =>
        typeof value === "function"
          ? (value as (old: T) => T)(previous ?? fallback)
          : value,
      );
    },
  ];
}
