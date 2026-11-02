import { useTranslation } from "react-i18next";
import { t } from "../i18n";
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Caption buttons for the undecorated window, sized like Windows 11's (46 px wide, the
// band's full height). Glyphs are 10 px strokes; close turns red on hover. They stay out of
// the tab order, as native caption buttons do; Alt+F4 and Win+arrows still work.
export function WindowControls() {
  useTranslation();
  const [maximized, setMaximized] = useState(false);
  // The native Snap Layouts overlay covers the maximise button, so the page learns its hover
  // and press from `caption-max` events instead of the pointer.
  const [maxState, setMaxState] = useState<"" | "hover" | "pressed">("");
  useEffect(() => {
    let stop: (() => void) | undefined;
    void listen<string>("caption-max", ({ payload }) =>
      setMaxState(
        payload === "down" ? "pressed" : payload === "leave" ? "" : "hover",
      ),
    ).then((fn) => (stop = fn));
    return () => stop?.();
  }, []);
  useEffect(() => {
    const win = getCurrentWindow();
    let stop: (() => void) | undefined;
    const sync = () =>
      void win
        .isMaximized()
        .then(setMaximized)
        .catch(() => {});
    sync();
    let unfocus: (() => void) | undefined;
    void win.onResized(sync).then((fn) => (stop = fn));
    // Like a native caption, the band dims while another window has focus.
    void win
      .onFocusChanged(({ payload }) => {
        document.documentElement.toggleAttribute("data-inactive", !payload);
      })
      .then((fn) => (unfocus = fn));
    return () => {
      stop?.();
      unfocus?.();
    };
  }, []);
  const win = () => getCurrentWindow();
  return (
    <div className="window-controls">
      <button
        type="button"
        tabIndex={-1}
        aria-label={t("controls.minimize")}
        onClick={() => void win().minimize()}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0 5.5h10" />
        </svg>
      </button>
      <button
        type="button"
        tabIndex={-1}
        aria-label={maximized ? t("controls.restore") : t("controls.maximize")}
        className={maxState}
        onClick={() => void win().toggleMaximize()}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true">
          {maximized ? (
            <path d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z" />
          ) : (
            <path d="M.5.5h9v9h-9z" />
          )}
        </svg>
      </button>
      <button
        type="button"
        tabIndex={-1}
        className="close"
        aria-label={t("controls.close")}
        onClick={() => void win().close()}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M.5.5l9 9M9.5.5l-9 9" />
        </svg>
      </button>
    </div>
  );
}
