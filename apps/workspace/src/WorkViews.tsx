import { useState, type RefObject } from "react";
import { Bookmark, Check, ImageDown, Trash2 } from "lucide-react";
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
import type { Filters, GroupField } from "./WorkFilters";
import type { SortKey } from "./WorkSort";
import { FailureNote, failureOf } from "./TemplateImport";
import type { SetupFailure } from "./contracts/setup";
import { t } from "./i18n";

/** Everything that decides what the list shows and how. */
export type View = {
  name: string;
  filters: Filters;
  group: GroupField;
  sort: SortKey[];
  stage: string;
  attention: string;
  origin: string;
};

// Saved views are a per-viewer convenience: kept in this machine's browser storage, per
// model and kind, never written to SPM or a source.
const storeKey = (code: string, kind: string) =>
  `tinnci.views.v1.${code}.${kind}`;
export function loadViews(code: string, kind: string): View[] {
  try {
    const raw = localStorage.getItem(storeKey(code, kind));
    const list = raw ? (JSON.parse(raw) as View[]) : [];
    return Array.isArray(list) ? list.filter((v) => v && v.name) : [];
  } catch {
    return [];
  }
}
function storeViews(code: string, kind: string, views: View[]) {
  try {
    localStorage.setItem(storeKey(code, kind), JSON.stringify(views));
  } catch {
    // Storage may be unavailable; the views then last for this session only.
  }
}
const same = (a: View, b: Omit<View, "name">) =>
  JSON.stringify({ ...a, name: "" }) === JSON.stringify({ ...b, name: "" });

/** Saved views: one click applies a view, the current one can be saved under a name. */
export function ViewsMenu({
  code,
  kind,
  current,
  onApply,
}: {
  code: string;
  kind: string;
  current: Omit<View, "name">;
  onApply: (v: View) => void;
}) {
  const [views, setViews] = useState(() => loadViews(code, kind));
  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");
  const active = views.find((v) => same(v, current));
  const save = () => {
    const n = name.trim();
    if (!n) return;
    const next = [
      ...views.filter((v) => v.name !== n),
      { ...current, name: n },
    ];
    setViews(next);
    storeViews(code, kind, next);
    setNaming(false);
    setName("");
  };
  const remove = (n: string) => {
    const next = views.filter((v) => v.name !== n);
    setViews(next);
    storeViews(code, kind, next);
  };
  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger className="filter-add">
          <Bookmark size={14} aria-hidden="true" />
          {active?.name ?? t("views.title")}
        </DropdownMenuTrigger>
        <DropdownMenuContent className="filter-menu" align="start">
          {views.length ? (
            <DropdownMenuGroup>
              <DropdownMenuLabel>{t("views.saved")}</DropdownMenuLabel>
              {views.map((v) => (
                <DropdownMenuItem key={v.name} onClick={() => onApply(v)}>
                  <span className="filter-option">
                    <span>{v.name}</span>
                    {v === active ? (
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
          <DropdownMenuItem onClick={() => setNaming(true)}>
            {t("views.saveCurrent")}
          </DropdownMenuItem>
          {views.length ? (
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                {t("views.delete")}
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="filter-menu">
                {views.map((v) => (
                  <DropdownMenuItem key={v.name} onClick={() => remove(v.name)}>
                    <Trash2 size={14} aria-hidden="true" />
                    {v.name}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>
      {naming ? (
        <Dialog open onOpenChange={(open) => (open ? null : setNaming(false))}>
          <DialogContent className="local-dialog">
            <DialogTitle>{t("views.saveCurrent")}</DialogTitle>
            <DialogDescription>{t("views.saveHint")}</DialogDescription>
            <form
              className="local-row"
              onSubmit={(e) => {
                e.preventDefault();
                save();
              }}
            >
              <Input
                autoFocus
                value={name}
                maxLength={40}
                placeholder={t("views.name")}
                aria-label={t("views.name")}
                onChange={(e) => setName(e.target.value)}
              />
              <Button size="sm" type="submit" disabled={!name.trim()}>
                {t("views.save")}
              </Button>
            </form>
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
