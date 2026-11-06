import type { Phase } from "../phase";

const disc = [
  [1, 2, 3],
  [0, 1, 2, 3, 4],
  [0, 1, 2, 3, 4],
  [0, 1, 2, 3, 4],
  [1, 2, 3],
];
/** The product's life as cells of the mark: filling in 研发, whole at 封板, emptying after. */
const lit: Record<Phase, number[]> = {
  development: [3, 4],
  release: [0, 1, 2, 3, 4],
  ota: [0, 1, 2],
  maintenance: [0],
};

export function Lune({ phase, size = 13 }: { phase: Phase; size?: number }) {
  return (
    <svg
      className="lune"
      viewBox="0 0 19 19"
      width={size}
      height={size}
      aria-hidden="true"
      focusable="false"
    >
      {disc.flatMap((columns, y) =>
        columns.map((x) => (
          <rect
            key={`${x}-${y}`}
            x={x * 4}
            y={y * 4}
            width="3"
            height="3"
            fillOpacity={lit[phase].includes(x) ? 1 : 0.2}
            style={{ transitionDelay: `${x * 40}ms` }}
          />
        )),
      )}
    </svg>
  );
}
