import type { TraceSpan } from "./contracts/setup";

/** A span's length as people read it: 840 ms, 12.3 s, 2m 05s. */
export function duration(ms: number) {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const seconds = Math.round(ms / 1000);
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, "0")}s`;
}

/** Rows left once folded spans hide what follows them at a greater depth. */
export function visibleRows(spans: TraceSpan[], folded: ReadonlySet<number>) {
  const rows: number[] = [];
  let hideBelow: number | null = null;
  spans.forEach((span, at) => {
    if (hideBelow !== null && span.depth > hideBelow) return;
    hideBelow = folded.has(at) ? span.depth : null;
    rows.push(at);
  });
  return rows;
}

export function hasChildren(spans: TraceSpan[], at: number) {
  return (spans[at + 1]?.depth ?? -1) > spans[at].depth;
}

/** The span with the most time of its own: the one worth looking at first. */
export function slowest(spans: TraceSpan[]) {
  let best = -1;
  spans.forEach((span, at) => {
    if (span.self_ms > 0 && (best < 0 || span.self_ms > spans[best].self_ms))
      best = at;
  });
  return best;
}

/** Evenly spaced round times across a trace. */
export function ticks(total: number, most = 5) {
  if (total <= 0) return [0];
  const rough = total / most;
  const power = 10 ** Math.floor(Math.log10(rough));
  const step = ([1, 2, 5, 10].find((n) => n * power >= rough) ?? 10) * power;
  const marks: number[] = [];
  for (let at = 0; at <= total; at += step) marks.push(at);
  return marks;
}

export function spanWords(span: TraceSpan) {
  return `${span.app} ${span.name} ${span.verb} ${span.status} ${span.code}`.toLowerCase();
}
