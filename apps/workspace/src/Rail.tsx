import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  CalendarDays,
  PackageCheck,
  ChevronRight,
  Gauge,
  Inbox,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Settings,
  Smartphone,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ContextTarget, type ContextAction } from "./components/context-menu";
import { Fragment } from "react";
import { TabLine } from "./Model";
import { prefetchBuilds } from "./focus-cache";
import { t } from "./i18n";
import type { Page } from "./api";

export type ModelNavItem = {
  code: string;
  name: string;
  short: boolean | null;
  legacy?: boolean;
};

const COLLAPSED_KEY = "workspace-rail-collapsed";
const NARROW = "(max-width: 760px)";

export const systemPages = ["health", "setup", "logs", "traces"] as const;
export function isSystem(page: Page) {
  return page === "project" || systemPages.some((p) => p === page);
}

function savedCollapsed() {
  try {
    return localStorage.getItem(COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

function saveCollapsed(collapsed: boolean) {
  try {
    localStorage.setItem(COLLAPSED_KEY, collapsed ? "1" : "0");
  } catch {
    // The rail still folds; it just opens wide next time.
  }
}

/** A narrow window folds the rail whatever was chosen, and gives it back when it widens. */
function useNarrow() {
  const [narrow, setNarrow] = useState(
    () => window.matchMedia?.(NARROW).matches ?? false,
  );
  useEffect(() => {
    const media = window.matchMedia?.(NARROW);
    if (!media) return;
    const change = () => setNarrow(media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  return narrow;
}

function RailLink({
  icon: Icon,
  label,
  current,
  collapsed,
  count,
  onClick,
}: {
  icon: LucideIcon;
  label: string;
  current?: boolean;
  /** What is waiting here; the name says so to a screen reader. */
  count?: number;
  collapsed: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="rail-link"
      aria-current={current ? "page" : undefined}
      aria-label={label}
      aria-description={count ? t("nav.waiting", { count }) : undefined}
      title={collapsed ? label : undefined}
      onClick={onClick}
    >
      <Icon aria-hidden="true" />
      {collapsed ? null : <span>{label}</span>}
      {count ? <b className="rail-count">{count}</b> : null}
    </button>
  );
}

function dotClass(short: boolean | null) {
  return short === null ? "dot" : short ? "dot short" : "dot ok";
}

function ModelActions({
  name,
  actions,
}: {
  name: string;
  actions: ContextAction[];
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-xs"
            className="rail-model-more"
            aria-label={t("nav.modelActions", { name })}
          >
            <MoreHorizontal aria-hidden="true" />
          </Button>
        }
      />
      <DropdownMenuContent align="start" className="workspace-menu">
        {actions.map((action) => (
          <Fragment key={action.id}>
            {action.separator ? <DropdownMenuSeparator /> : null}
            <DropdownMenuItem
              disabled={action.disabled}
              onClick={() => void action.run()}
            >
              {action.icon ? <action.icon aria-hidden="true" /> : null}
              {action.label}
            </DropdownMenuItem>
          </Fragment>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Where everything lives: your work, the models, and the workspace itself. */
export function Rail({
  models,
  page,
  activeCode,
  waiting,
  onPage,
  onModel,
  onFollow,
  onSettings,
  onWarm,
  onCancelWarm,
  actions,
}: {
  models: ModelNavItem[];
  page: Page;
  activeCode: string | null;
  waiting: number;
  onPage: (page: Page) => void;
  onModel: (code: string) => void;
  onFollow: () => void;
  onSettings: () => void;
  onWarm: (code: string) => void;
  onCancelWarm: () => void;
  actions: (code: string) => ContextAction[];
}) {
  useTranslation();
  const [chosen, setChosen] = useState(savedCollapsed);
  const narrow = useNarrow();
  const collapsed = chosen || narrow;
  const current = useRef<HTMLButtonElement>(null);
  useLayoutEffect(() => {
    current.current?.scrollIntoView?.({ block: "nearest" });
  }, [activeCode, collapsed]);
  const link = (target: Page, icon: LucideIcon, label: string, on: boolean) => (
    <RailLink
      icon={icon}
      label={label}
      current={on}
      collapsed={collapsed}
      onClick={() => onPage(target)}
    />
  );
  const active = models.filter((m) => !m.legacy);
  const legacy = models.filter((m) => m.legacy);
  const [openLegacy, setOpenLegacy] = useState(false);
  const showLegacy = openLegacy || legacy.some((m) => m.code === activeCode);
  const modelRow = (model: ModelNavItem) => (
    <div className="rail-model" key={model.code}>
      <ContextTarget title={model.name} actions={actions(model.code)}>
        <button
          type="button"
          className="rail-model-open"
          ref={activeCode === model.code ? current : undefined}
          aria-label={model.name}
          title={
            model.name === model.code
              ? model.name
              : `${model.name}\n${model.code}`
          }
          aria-current={activeCode === model.code ? "page" : undefined}
          onClick={() => {
            onCancelWarm();
            onModel(model.code);
          }}
          onMouseEnter={() => onWarm(model.code)}
          onMouseLeave={onCancelWarm}
          onFocus={() => void prefetchBuilds(model.code)}
        >
          <i
            aria-hidden="true"
            className={dotClass(model.legacy ? null : model.short)}
          />
          <span>{collapsed ? model.name.slice(-4) : model.name}</span>
        </button>
      </ContextTarget>
      {collapsed ? null : (
        <ModelActions name={model.name} actions={actions(model.code)} />
      )}
    </div>
  );
  return (
    <nav
      className="rail"
      data-collapsed={collapsed}
      aria-label={t("app.mainNavigation")}
    >
      <RailLink
        icon={Inbox}
        label={t("nav.mine")}
        current={page === "mine"}
        collapsed={collapsed}
        count={waiting}
        onClick={() => onPage("mine")}
      />
      <div className="rail-group">
        {link("home", Smartphone, t("nav.models"), page === "home")}
        {collapsed ? null : (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={t("app.followAModel")}
            title={t("app.followAModel")}
            onClick={onFollow}
          >
            <Plus aria-hidden="true" />
          </Button>
        )}
      </div>
      <div className="rail-models">
        {active.map(modelRow)}
        {legacy.length ? (
          <>
            <button
              type="button"
              className="rail-legacy"
              aria-expanded={showLegacy}
              title={t("legacy.title")}
              onClick={() => setOpenLegacy(!showLegacy)}
            >
              <ChevronRight aria-hidden="true" />
              {collapsed ? null : <span>{t("legacy.title")}</span>}
              <span className="quiet">{legacy.length}</span>
            </button>
            {showLegacy ? legacy.map(modelRow) : null}
          </>
        ) : null}
      </div>
      <div className="rail-foot">
        {link("calendar", CalendarDays, t("nav.calendar"), page === "calendar")}
        {link("ota", PackageCheck, t("nav.ota"), page === "ota")}
        {link("activity", Activity, t("nav.activity"), page === "activity")}
        {link("health", Gauge, t("nav.system"), isSystem(page))}
        <RailLink
          icon={Settings}
          label={t("nav.settings")}
          collapsed={collapsed}
          onClick={onSettings}
        />
        {narrow ? null : (
          <RailLink
            icon={chosen ? PanelLeftOpen : PanelLeftClose}
            label={t(chosen ? "nav.expand" : "nav.collapse")}
            collapsed={collapsed}
            onClick={() => {
              setChosen(!chosen);
              saveCollapsed(!chosen);
            }}
          />
        )}
      </div>
    </nav>
  );
}

/** The workspace's own pages, one row above whichever is open. */
export function SystemTabs({
  page,
  onPage,
}: {
  page: Page;
  onPage: (page: Page) => void;
}): ReactNode {
  useTranslation();
  return (
    <nav className="system-tabs" aria-label={t("nav.system")}>
      {systemPages.map((target) => (
        <button
          type="button"
          key={target}
          aria-current={page === target ? "page" : undefined}
          onClick={() => onPage(target)}
        >
          {t(`nav.${target}`)}
          {page === target ? <TabLine id="system" /> : null}
        </button>
      ))}
    </nav>
  );
}
