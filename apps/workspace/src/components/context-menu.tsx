import { menuActionIcon } from "./action-icon";
import { ContextMenu } from "@base-ui/react/context-menu";
import {
  cloneElement,
  createContext,
  useCallback,
  useContext,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { Copy, type LucideIcon } from "lucide-react";
import { copyText } from "../clipboard";
import { t } from "../i18n";

export type ContextAction = {
  id: string;
  label: string;
  icon?: LucideIcon;
  disabled?: boolean;
  separator?: boolean;
  run: () => void | Promise<unknown>;
};
type Entry = { title?: string; actions: ContextAction[] };
const RegistryContext = createContext<
  ((id: string, entry: Entry) => () => void) | null
>(null);

// Targets only register actions and reuse their existing DOM element. The app has
// one menu store, one portal, and one set of positioning/dismissal listeners.
export function ContextTarget({
  children,
  actions,
  title,
}: Entry & { children: ReactElement }) {
  const register = useContext(RegistryContext);
  const id = useId();
  useLayoutEffect(
    () => register?.(id, { title, actions }),
    [register, id, title, actions],
  );
  if (!register) return children;
  const props = children.props as { tabIndex?: number };
  return cloneElement(children as ReactElement<Record<string, unknown>>, {
    "data-context-target": id,
    "aria-haspopup": "menu",
    tabIndex: props.tabIndex ?? 0,
  });
}

export function ContextMenuProvider({ children }: { children: ReactNode }) {
  useTranslation();
  const entries = useRef(new Map<string, Entry>());
  const returnFocus = useRef<HTMLElement | null>(null);
  const generation = useRef(0);
  const [entry, setEntry] = useState<Entry | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const register = useCallback((id: string, value: Entry) => {
    entries.current.set(id, value);
    return () => {
      entries.current.delete(id);
    };
  }, []);
  function dismiss() {
    generation.current++;
    setOpen(false);
    setBusy(null);
    setFailed(false);
  }
  function prepare(target: { element: HTMLElement; entry: Entry }) {
    generation.current++;
    returnFocus.current = target.element;
    const selection = window.getSelection();
    const text = selection?.toString();
    const actions =
      text &&
      target.element.contains(selection?.anchorNode ?? null) &&
      target.element.contains(selection?.focusNode ?? null)
        ? [
            {
              id: "copy-selection",
              label: t("context.copySelection"),
              icon: Copy,
              run: () => copyText(text),
            },
            ...target.entry.actions.map((action, index) =>
              index === 0 ? { ...action, separator: true } : action,
            ),
          ]
        : target.entry.actions;
    setEntry({ ...target.entry, actions });
    setBusy(null);
    setFailed(false);
  }
  function targetOf(target: EventTarget | null) {
    if (
      !(target instanceof Element) ||
      target.closest(
        "input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=textbox]",
      )
    )
      return null;
    const element = target.closest<HTMLElement>("[data-context-target]");
    const found =
      element && entries.current.get(element.dataset.contextTarget!);
    return element && found?.actions.length ? { element, entry: found } : null;
  }
  return (
    <RegistryContext.Provider value={register}>
      <ContextMenu.Root
        open={open}
        onOpenChange={(next) => {
          if (next) setOpen(true);
          else dismiss();
        }}
      >
        <ContextMenu.Trigger
          className="context-menu-surface"
          onContextMenu={(event) => {
            const target = targetOf(event.target);
            if (!target) {
              // Keep the native editing/selection menu and bypass the library's
              // document listener without cancelling the browser default.
              event.preventBaseUIHandler();
              event.stopPropagation();
              dismiss();
              return;
            }
            prepare(target);
          }}
          onTouchStart={(event) => {
            const target = targetOf(event.target);
            if (!target) {
              event.preventBaseUIHandler();
              event.stopPropagation();
              return;
            }
            prepare(target);
          }}
          onKeyDown={(event) => {
            if (
              event.key !== "ContextMenu" &&
              !(event.shiftKey && event.key === "F10")
            )
              return;
            const target = targetOf(event.target);
            if (!target) return;
            event.preventDefault();
            event.stopPropagation();
            const rect = target.element.getBoundingClientRect();
            target.element.dispatchEvent(
              new MouseEvent("contextmenu", {
                bubbles: true,
                cancelable: true,
                button: 2,
                clientX: rect.left + Math.min(rect.width / 2, 24),
                clientY: rect.top + Math.min(rect.height, 24),
              }),
            );
          }}
        >
          {children}
        </ContextMenu.Trigger>
        <ContextMenu.Portal>
          <ContextMenu.Positioner
            className="context-menu-positioner"
            sideOffset={4}
            collisionPadding={8}
          >
            <ContextMenu.Popup
              className="context-menu-popup"
              aria-label={t("context.actions")}
              finalFocus={() =>
                returnFocus.current?.isConnected ? returnFocus.current : false
              }
            >
              <ContextMenu.Group>
                {entry?.title ? (
                  <ContextMenu.GroupLabel className="context-menu-label">
                    {entry.title}
                  </ContextMenu.GroupLabel>
                ) : null}
                {entry?.actions.map((action) => {
                  const Icon = action.icon ?? menuActionIcon(action.id);
                  return (
                    <div key={action.id} role="presentation">
                      {action.separator ? (
                        <ContextMenu.Separator className="context-menu-separator" />
                      ) : null}
                      <ContextMenu.Item
                        className="context-menu-item"
                        label={action.label}
                        disabled={action.disabled || !!busy}
                        closeOnClick={false}
                        onClick={async () => {
                          if (busy) return;
                          const request = generation.current;
                          setBusy(action.id);
                          setFailed(false);
                          try {
                            await action.run();
                            if (request === generation.current) dismiss();
                          } catch {
                            if (request === generation.current) setFailed(true);
                          } finally {
                            if (request === generation.current) setBusy(null);
                          }
                        }}
                      >
                        {Icon ? (
                          <Icon aria-hidden="true" size={15} />
                        ) : (
                          <span className="context-menu-icon" />
                        )}
                        <span>{action.label}</span>
                      </ContextMenu.Item>
                    </div>
                  );
                })}
              </ContextMenu.Group>
              {busy ? (
                <span className="context-menu-status" role="status">
                  {t("context.working")}
                </span>
              ) : null}
              {failed ? (
                <span
                  className="context-menu-status context-menu-error"
                  role="alert"
                >
                  {t("context.failed")}
                </span>
              ) : null}
            </ContextMenu.Popup>
          </ContextMenu.Positioner>
        </ContextMenu.Portal>
      </ContextMenu.Root>
    </RegistryContext.Provider>
  );
}
