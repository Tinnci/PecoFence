import { useState, type RefObject } from "react";
import { Bookmark, Check, Copy, ImageDown, Pencil, Trash2 } from "lucide-react";
import { toPng } from "html-to-image";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { savePng } from "./api";
import {
  normalizeView,
  readViewStore,
  sameView,
  saveViewStore,
  viewStorageFailed,
  type SavedView,
  type ViewSettings,
} from "./view-storage";
import { FailureNote, failureOf } from "./TemplateImport";
import type { SetupFailure } from "./contracts/setup";
import { t } from "./i18n";

export type View = ViewSettings & { name: string };
export const loadViews = (code: string, kind: string) =>
  readViewStore(code, kind).views;

/** Named views persist alongside the current view, isolated by model and kind. */
export function ViewsMenu({
  code,
  kind,
  current,
  onApply,
}: {
  code: string;
  kind: string;
  current: ViewSettings;
  onApply: (view: View) => void;
}) {
  const [views, setViews] = useState(() => loadViews(code, kind));
  const [selected, setSelected] = useState(
    () => readViewStore(code, kind).selected,
  );
  const [naming, setNaming] = useState<{
    mode: "create" | "rename" | "duplicate";
    source?: SavedView;
  } | null>(null);
  const [name, setName] = useState("");
  const active =
    views.find((v) => v.id === selected) ??
    views.find((v) => sameView(v, current));
  const modified = active && !sameView(active, current);
  const duplicateName = views.some(
    (v) =>
      v.name === name.trim() &&
      !(naming?.mode === "rename" && v.id === naming.source?.id),
  );
  const commit = (next: SavedView[], id: string | null) => {
    setViews(next);
    setSelected(id);
    saveViewStore(code, kind, { views: next, selected: id });
  };
  const start = (
    mode: "create" | "rename" | "duplicate",
    source?: SavedView,
  ) => {
    setNaming({ mode, source });
    let next = mode === "rename" ? (source?.name ?? "") : "";
    if (mode === "duplicate" && source) {
      const base = `${source.name.slice(0, 60)} ${t("views.copySuffix")}`;
      next = base;
      let number = 2;
      while (views.some((v) => v.name === next)) next = `${base} ${number++}`;
    }
    setName(next);
  };
  const save = () => {
    if (!naming || !name.trim() || duplicateName) return;
    if (naming.mode === "rename" && naming.source) {
      commit(
        views.map((v) =>
          v.id === naming.source?.id ? { ...v, name: name.trim() } : v,
        ),
        selected,
      );
    } else {
      if (views.length >= 50) return;
      const next: SavedView = {
        ...normalizeView(naming.source ?? current),
        id: crypto.randomUUID(),
        name: name.trim(),
      };
      commit([...views, next], next.id);
      onApply(next);
    }
    setNaming(null);
  };
  const title =
    naming?.mode === "rename"
      ? t("views.rename")
      : naming?.mode === "duplicate"
        ? t("views.duplicate")
        : t("views.saveCurrent");
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger className="filter-add">
          <Bookmark size={14} aria-hidden="true" />
          {active
            ? `${active.name}${modified ? ` · ${t("views.modified")}` : ""}`
            : t("views.title")}
        </DropdownMenuTrigger>
        <DropdownMenuContent className="filter-menu" align="start">
          {views.length ? (
            <DropdownMenuGroup>
              <DropdownMenuLabel>{t("views.saved")}</DropdownMenuLabel>
              {views.map((view) => (
                <DropdownMenuItem
                  key={view.id}
                  onClick={() => {
                    setSelected(view.id);
                    saveViewStore(code, kind, { selected: view.id });
                    onApply(view);
                  }}
                >
                  <span className="filter-option">
                    <span>{view.name}</span>
                    {view.id === active?.id ? (
                      <Check
                        size={14}
                        aria-hidden="true"
                        className="filter-count"
                      />
                    ) : null}
                  </span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
          ) : null}
          {views.length ? <DropdownMenuSeparator /> : null}
          {modified && active ? (
            <DropdownMenuItem
              onClick={() =>
                commit(
                  views.map((v) =>
                    v.id === active.id
                      ? { ...normalizeView(current), id: v.id, name: v.name }
                      : v,
                  ),
                  active.id,
                )
              }
            >
              {t("views.updateCurrent")}
            </DropdownMenuItem>
          ) : null}
          <DropdownMenuItem
            disabled={views.length >= 50}
            onClick={() => start("create")}
          >
            {t("views.saveCurrent")}
          </DropdownMenuItem>
          {(["rename", "duplicate", "delete"] as const).map((action) =>
            views.length ? (
              <DropdownMenuSub key={action}>
                <DropdownMenuSubTrigger
                  disabled={action === "duplicate" && views.length >= 50}
                >
                  {t(`views.${action}`)}
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent className="filter-menu">
                  {views.map((view) => (
                    <DropdownMenuItem
                      key={view.id}
                      onClick={() => {
                        if (action === "delete")
                          commit(
                            views.filter((v) => v.id !== view.id),
                            selected === view.id ? null : selected,
                          );
                        else start(action, view);
                      }}
                    >
                      {action === "rename" ? (
                        <Pencil size={14} aria-hidden="true" />
                      ) : action === "duplicate" ? (
                        <Copy size={14} aria-hidden="true" />
                      ) : (
                        <Trash2 size={14} aria-hidden="true" />
                      )}
                      {view.name}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            ) : null,
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {viewStorageFailed(code, kind) ? (
        <span className="quiet" role="alert">
          {t("views.storageFailed")}
        </span>
      ) : null}
      {naming ? (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open) setNaming(null);
          }}
        >
          <DialogContent className="local-dialog">
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{t("views.saveHint")}</DialogDescription>
            <form
              className="local-row"
              onSubmit={(event) => {
                event.preventDefault();
                save();
              }}
            >
              <Input
                autoFocus
                value={name}
                maxLength={80}
                placeholder={t("views.name")}
                aria-label={t("views.name")}
                aria-invalid={duplicateName || undefined}
                onChange={(event) => setName(event.target.value)}
              />
              <Button
                size="sm"
                type="submit"
                disabled={!name.trim() || duplicateName}
              >
                {t("views.save")}
              </Button>
            </form>
            {duplicateName ? (
              <p className="quiet" role="alert">
                {t("views.nameExists")}
              </p>
            ) : null}
          </DialogContent>
        </Dialog>
      ) : null}
    </>
  );
}

/** The list as a PNG: what the view shows, under a caption saying which view it is. */
export function ExportButton({
  target,
  fileName,
  prepare,
}: {
  target: RefObject<HTMLElement | null>;
  fileName: string;
  /** Gets the page ready (expanded or not) and returns how to put it back. */
  prepare: (expanded: boolean) => Promise<() => void>;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<SetupFailure | null>(null);
  async function run(expanded: boolean) {
    const node = target.current;
    if (!node) return;
    setBusy(true);
    setFailure(null);
    let restore = () => {};
    try {
      restore = await prepare(expanded);
      node.classList.add("exporting");
      if (expanded) node.classList.add("exporting-all");
      // Fonts can change wrapping and row heights after expansion.
      await document.fonts.ready;
      // Let React commit the expanded rows and the static export styles lay out.
      await new Promise((r) =>
        requestAnimationFrame(() => requestAnimationFrame(r)),
      );
      const background =
        getComputedStyle(document.body).backgroundColor || "#ffffff";
      // Browsers cap a canvas near 32k pixels a side; a long expanded view drops to 1x.
      const width = node.scrollWidth;
      const height = node.scrollHeight;
      const ratio = Math.max(width, height) * 2 > 30000 ? 1 : 2;
      const url = await toPng(node, {
        width,
        height,
        pixelRatio: ratio,
        backgroundColor: background,
        cacheBust: true,
        filter: (n) =>
          !(n instanceof HTMLElement && n.classList.contains("no-export")),
      });
      const bytes = Uint8Array.from(atob(url.split(",")[1]), (c) =>
        c.charCodeAt(0),
      );
      const path = await savePng(fileName, bytes);
      if (path) await revealItemInDir(path).catch(() => undefined);
    } catch (error) {
      setFailure(
        failureOf(error).code === "spm_unavailable"
          ? { code: "export_failed" }
          : failureOf(error),
      );
    } finally {
      node.classList.remove("exporting", "exporting-all");
      restore();
      setBusy(false);
    }
  }
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          className="filter-add"
          disabled={busy}
          title={t("views.export")}
        >
          {busy ? (
            <Spinner className="size-3.5" />
          ) : (
            <ImageDown size={14} aria-hidden="true" />
          )}
          {t("views.exportShort")}
        </DropdownMenuTrigger>
        <DropdownMenuContent className="filter-menu" align="end">
          <DropdownMenuItem onClick={() => void run(false)}>
            <span className="export-option">
              <span>{t("views.exportAsShown")}</span>
              <span className="quiet">{t("views.exportAsShownHint")}</span>
            </span>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => void run(true)}>
            <span className="export-option">
              <span>{t("views.exportExpanded")}</span>
              <span className="quiet">{t("views.exportExpandedHint")}</span>
            </span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {failure ? <FailureNote failure={failure} /> : null}
    </>
  );
}
