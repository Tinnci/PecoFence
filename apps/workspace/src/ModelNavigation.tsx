import { useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronLeft, ChevronRight, ListFilter } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import { ActionIcon } from "./components/action-icon";
import { ContextTarget, type ContextAction } from "./components/context-menu";
import { TabLine } from "./Model";
import { prefetchBuilds } from "./focus-cache";
import { t } from "./i18n";

export type ModelNavItem = {
  code: string;
  name: string;
  short: boolean | null;
};

type Props = {
  models: ModelNavItem[];
  activeCode: string | null;
  homeActive: boolean;
  onHome: () => void;
  onModel: (code: string) => void;
  onWarm: (code: string) => void;
  onCancelWarm: () => void;
  actions: (code: string) => ContextAction[];
};

export function ModelNavigation({
  models,
  activeCode,
  homeActive,
  onHome,
  onModel,
  onWarm,
  onCancelWarm,
  actions,
}: Props) {
  useTranslation();
  const viewport = useRef<HTMLDivElement>(null);
  const track = useRef<HTMLDivElement>(null);
  const active = useRef<HTMLButtonElement>(null);
  const [bounds, setBounds] = useState({ left: false, right: false });
  const [picker, setPicker] = useState(false);
  const [search, setSearch] = useState("");
  const signature = JSON.stringify(
    models.map(({ code, name }) => [code, name]),
  );
  function measure() {
    const el = viewport.current;
    if (!el) return;
    const left = el.scrollLeft > 1;
    const right = el.scrollWidth - el.clientWidth - el.scrollLeft > 1;
    setBounds((old) =>
      old.left === left && old.right === right ? old : { left, right },
    );
  }
  function revealActive() {
    const el = viewport.current;
    const button = active.current;
    if (!el || !button) return;
    const view = el.getBoundingClientRect();
    const item = button.getBoundingClientRect();
    const delta =
      item.left < view.left
        ? item.left - view.left
        : item.right > view.right
          ? item.right - view.right
          : 0;
    // Only move this strip; scrollIntoView would also move the model page vertically.
    if (delta) el.scrollBy({ left: delta, behavior: "auto" });
    measure();
  }
  useLayoutEffect(() => {
    const resize = () => {
      revealActive();
      measure();
    };
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
    if (viewport.current) observer?.observe(viewport.current);
    if (track.current) observer?.observe(track.current);
    window.addEventListener("resize", resize);
    const el = viewport.current;
    const wheel = (event: WheelEvent) => {
      if (!el || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      const next = Math.max(
        0,
        Math.min(el.scrollWidth - el.clientWidth, el.scrollLeft + event.deltaY),
      );
      if (next === el.scrollLeft) return;
      event.preventDefault();
      el.scrollLeft = next;
      measure();
    };
    el?.addEventListener("wheel", wheel, { passive: false });
    measure();
    return () => {
      observer?.disconnect();
      el?.removeEventListener("wheel", wheel);
      window.removeEventListener("resize", resize);
    };
  }, []);
  useLayoutEffect(() => {
    revealActive();
  }, [activeCode, signature]);
  function scroll(direction: number) {
    const el = viewport.current;
    if (el)
      el.scrollBy({
        left: direction * Math.max(120, el.clientWidth * 0.75),
        behavior: window.matchMedia?.("(prefers-reduced-motion: reduce)")
          .matches
          ? "auto"
          : "smooth",
      });
  }
  const query = search.trim().toLocaleLowerCase();
  const filtered = models.filter((m) =>
    `${m.name}\n${m.code}`.toLocaleLowerCase().includes(query),
  );
  function choose(code: string) {
    setPicker(false);
    onCancelWarm();
    onModel(code);
  }
  return (
    <nav
      className="model-navigation"
      data-tauri-drag-region
      aria-label={t("app.mainNavigation")}
    >
      <button
        type="button"
        className="tab model-tabs-home"
        aria-current={homeActive ? "page" : undefined}
        onClick={onHome}
      >
        <ActionIcon name="follow" /> {t("app.following")}
        {homeActive ? <TabLine id="appbar" /> : null}
      </button>
      {bounds.left || bounds.right ? (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          disabled={!bounds.left}
          aria-label={t("modelNav.previous")}
          onClick={() => scroll(-1)}
        >
          <ChevronLeft aria-hidden="true" />
        </Button>
      ) : null}
      <div
        className="model-tabs"
        data-tauri-drag-region
        ref={viewport}
        onScroll={measure}
      >
        <div className="model-tabs-track" data-tauri-drag-region ref={track}>
          {models.map((model) => (
            <ContextTarget
              key={model.code}
              title={model.name}
              actions={actions(model.code)}
            >
              <button
                type="button"
                className="tab"
                ref={activeCode === model.code ? active : undefined}
                title={
                  model.name === model.code
                    ? model.name
                    : `${model.name}\n${model.code}`
                }
                aria-current={activeCode === model.code ? "page" : undefined}
                onClick={() => onModel(model.code)}
                onMouseEnter={() => onWarm(model.code)}
                onMouseLeave={onCancelWarm}
                onFocus={() => void prefetchBuilds(model.code)}
              >
                <i
                  aria-hidden="true"
                  className={
                    model.short === null
                      ? "dot"
                      : model.short
                        ? "dot short"
                        : "dot ok"
                  }
                />
                <span className="model-tab-name">{model.name}</span>
                {activeCode === model.code ? <TabLine id="appbar" /> : null}
              </button>
            </ContextTarget>
          ))}
        </div>
      </div>
      {bounds.left || bounds.right ? (
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          disabled={!bounds.right}
          aria-label={t("modelNav.next")}
          onClick={() => scroll(1)}
        >
          <ChevronRight aria-hidden="true" />
        </Button>
      ) : null}
      {models.length ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="model-picker-trigger"
          aria-label={t("modelNav.all", { count: models.length })}
          title={t("modelNav.all", { count: models.length })}
          onClick={() => {
            onCancelWarm();
            setSearch("");
            setPicker(true);
          }}
        >
          <ListFilter aria-hidden="true" />
          <span>{models.length}</span>
        </Button>
      ) : null}
      <Dialog open={picker} onOpenChange={setPicker}>
        <DialogContent className="model-picker">
          <DialogTitle>{t("modelNav.title")}</DialogTitle>
          <DialogDescription>{t("modelNav.hint")}</DialogDescription>
          <Input
            autoFocus
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            aria-label={t("modelNav.search")}
            placeholder={t("modelNav.search")}
            onKeyDown={(e) => {
              if (e.key === "Enter" && filtered[0]) {
                e.preventDefault();
                choose(filtered[0].code);
              }
            }}
          />
          <div className="model-picker-results">
            {filtered.map((model) => (
              <button
                type="button"
                key={model.code}
                className="model-picker-item"
                aria-current={activeCode === model.code ? "page" : undefined}
                onClick={() => choose(model.code)}
              >
                <i
                  aria-hidden="true"
                  className={
                    model.short === null
                      ? "dot"
                      : model.short
                        ? "dot short"
                        : "dot ok"
                  }
                />
                <span>
                  <strong>{model.name}</strong>
                  {model.name !== model.code ? (
                    <small>{model.code}</small>
                  ) : null}
                </span>
                {activeCode === model.code ? (
                  <Check size={16} aria-hidden="true" />
                ) : null}
              </button>
            ))}
            {!filtered.length ? (
              <p className="quiet" role="status">
                {t("modelNav.empty")}
              </p>
            ) : null}
          </div>
        </DialogContent>
      </Dialog>
    </nav>
  );
}
