import { useTranslation } from "react-i18next";
import { t } from "../i18n";

/** Weeks of code activity shaped as a disc, dim to bright: icons/lune.svg without its tile. */
const rows = [
  "..134..",
  ".11344.",
  "1113444",
  "1123434",
  "1113444",
  ".11244.",
  "..134..",
];
const weight: Record<string, number> = { 1: 0.16, 2: 0.38, 3: 0.68, 4: 1 };

// The dark tile belongs to Windows surfaces (taskbar, Start, title bar).
// Cells light column by column once on mount; `busy` keeps them sweeping while a read runs.
export function BrandMark({
  size = 24,
  busy = false,
}: {
  size?: number;
  busy?: boolean;
}) {
  useTranslation();
  return (
    <svg
      className={busy ? "brand-mark busy" : "brand-mark"}
      viewBox="0 0 82 82"
      width={size}
      height={size}
      fill="currentColor"
      role={busy ? "img" : undefined}
      aria-label={busy ? t("controls.loading") : undefined}
      aria-hidden={busy ? undefined : true}
      focusable="false"
    >
      {rows.flatMap((row, y) =>
        [...row].flatMap((level, x) =>
          level === "."
            ? []
            : [
                <rect
                  key={`${x}-${y}`}
                  x={x * 12}
                  y={y * 12}
                  width="10"
                  height="10"
                  rx="2"
                  fillOpacity={weight[level]}
                  style={{ animationDelay: `${x * 60}ms` }}
                />,
              ],
        ),
      )}
    </svg>
  );
}
