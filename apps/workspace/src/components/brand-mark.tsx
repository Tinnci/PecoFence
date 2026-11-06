import { useTranslation } from "react-i18next";
import { t } from "../i18n";
// In-app mark: the 工 of icons/workspace-small.svg without its tile, in the text colour.
// The green tile belongs to Windows surfaces (taskbar, Start, title bar).
// It builds in once when it mounts; `busy` pulses the upright while a read runs.
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
      viewBox="128 184 768 656"
      width={size}
      height={(size * 656) / 768}
      fill="currentColor"
      role={busy ? "img" : undefined}
      aria-label={busy ? t("controls.loading") : undefined}
      aria-hidden={busy ? undefined : true}
      focusable="false"
    >
      <rect
        className="bm-beam"
        x="200"
        y="184"
        width="624"
        height="168"
        rx="24"
      />
      <rect className="bm-cell" x="424" y="260" width="176" height="504" />
      <rect
        className="bm-base"
        x="128"
        y="672"
        width="768"
        height="168"
        rx="24"
      />
    </svg>
  );
}
