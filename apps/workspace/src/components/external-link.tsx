import { ActionIcon } from "./action-icon";
import { browserUrl } from "../browser-urls";
export { browserUrl } from "../browser-urls";
import type { ContextAction } from "./context-menu";
import { ContextTarget } from "./context-menu";
import { linkContextActions } from "../context-actions";
import {
  useState,
  type ReactNode,
  type CSSProperties,
  type MouseEventHandler,
} from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";

// Real anchors keep browser status previews, copy-link and keyboard navigation.
// Desktop clicks use the system browser rather than navigating the webview.
export function ExternalLink({
  href,
  children,
  className,
  style,
  title,
  contextMenu = true,
  actions,
  onMouseEnter,
  onMouseLeave,
}: {
  href?: string | null;
  children?: ReactNode;
  className?: string;
  style?: CSSProperties;
  title?: string;
  contextMenu?: boolean;
  actions?: ContextAction[];
  onMouseEnter?: MouseEventHandler<HTMLElement>;
  onMouseLeave?: MouseEventHandler<HTMLElement>;
}) {
  useTranslation();
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const url = browserUrl(href ?? undefined);
  if (!url)
    return (
      <span
        className={className}
        style={style}
        title={title}
        onMouseEnter={onMouseEnter}
        onMouseLeave={onMouseLeave}
      >
        {children}
      </span>
    );
  const link = (
    <a
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
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
      {children != null ? (
        <span className="external-link-icon">
          <ActionIcon name="open" />
        </span>
      ) : null}
    </a>
  );
  return (
    <>
      {contextMenu ? (
        <ContextTarget
          actions={
            actions ??
            linkContextActions(
              url,
              typeof children === "string" ? children : undefined,
            )
          }
        >
          {link}
        </ContextTarget>
      ) : (
        link
      )}
      {failedUrl === url ? (
        <span className="external-link-error" role="alert">
          {t("links.openFailed")}
        </span>
      ) : null}
    </>
  );
}
