import { useState } from "react";
import type { CSSProperties } from "react";

/** How long one heatmap takes to sweep in, whatever its size. */
const SWEEP_MS = 480;

/** A cell's place in a left-to-right sweep, each column leading the one after it. */
export function heatDelay(column: number, row: number, columns: number) {
  return Math.round(((column + row / 7) / Math.max(1, columns)) * SWEEP_MS);
}

export function heatCell(
  background: string,
  column: number,
  row: number,
  columns: number,
) {
  return {
    background,
    "--heat-delay": `${heatDelay(column, row, columns)}ms`,
  } as CSSProperties;
}

const swept = new Set<string>();

/** True the first time a heatmap appears this session, so returning to a page stays still. */
export function useHeatSweep(id: string) {
  const [sweep] = useState(() => {
    const first = !swept.has(id);
    swept.add(id);
    return first;
  });
  return sweep;
}
