import { useTranslation } from "react-i18next";
import { t } from "../i18n";
// In-app mark: the 工 of icons/workspace-small.svg without its tile — beam and base in the
// text colour, the upright one green and one flagged block.
// The paper tile belongs to Windows surfaces (taskbar, Start, title bar).
// It builds in once when it mounts; `busy` pulses the green block while a read runs.
export function BrandMark({
  size = 26,
  busy = false,
}: {
  size?: number;
  busy?: boolean;
}) {
  useTranslation();
  return (
    <svg
      className={busy ? "brand-mark busy" : "brand-mark"}
      viewBox="96 152 832 720"
      width={size}
      height={(size * 720) / 832}
      fill="currentColor"
      role={busy ? "img" : undefined}
      aria-label={busy ? t("controls.loading") : undefined}
      aria-hidden={busy ? undefined : true}
      focusable="false"
    >
      <rect
        className="bm-beam"
        x="176"
        y="152"
        width="672"
        height="168"
        rx="24"
      />
      <rect
        className="bm-base"
        x="96"
        y="704"
        width="832"
        height="168"
        rx="24"
      />
      <rect
        className="bm-cell"
        x="368"
        y="344"
        width="288"
        height="160"
        rx="20"
        fill="#3f7a3c"
      />
      <rect
        className="bm-flag"
        x="368"
        y="520"
        width="288"
        height="160"
        rx="20"
        fill="#b4532a"
      />
    </svg>
  );
}
