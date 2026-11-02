import { useState, type ReactNode, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";

export function browserUrl(value?: string): string | null {
  if (!value || /[\u0000-\u001f\u007f]/.test(value)) return null;
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) &&
      url.hostname &&
      !url.username &&
      !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}

// Real anchors keep browser status previews, copy-link and keyboard navigation.
// Desktop clicks use the system browser rather than navigating the webview.
export function ExternalLink({
  href,
  children,
  className,
  style,
  title,
}: {
  href?: string | null;
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
  title?: string;
}) {
  useTranslation();
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const url = browserUrl(href ?? undefined);
  if (!url)
    return (
      <span className={className} style={style} title={title}>
        {children}
      </span>
    );
  return (
    <>
      <a
        className={["external-link", className].filter(Boolean).join(" ")}
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        style={style}
        title={title ?? t("links.openInBrowser")}
        onClick={async (event) => {
          if (!isTauri()) return;
          event.preventDefault();
          setFailedUrl(null);
          try {
            await openUrl(url);
          } catch {
            setFailedUrl(url);
          }
        }}
      >
        {children}
      </a>
      {failedUrl === url ? (
        <span className="external-link-error" role="alert">
          {t("links.openFailed")}
        </span>
      ) : null}
    </>
  );
}
