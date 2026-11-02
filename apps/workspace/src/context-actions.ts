import { isTauri } from "@tauri-apps/api/core";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { copyText } from "./clipboard";
import {
  ArrowUpRight,
  Copy,
  FileText,
  LayoutDashboard,
  GitBranch,
  Hammer,
  Settings,
  RefreshCw,
  Check,
  Bookmark,
  BookmarkX,
} from "lucide-react";
import { t } from "./i18n";
import { browserUrl } from "./browser-urls";
import type { ContextAction } from "./components/context-menu";
import { focusKey, queryClient, summaryKey } from "./focus-cache";
import {
  consoleBrowserUrl,
  gerritSearchUrl,
  gerritTerm,
} from "./browser-links";
import type { BuildRun, FocusSummary } from "./contracts/setup";
import type { ModelTab } from "./api";

export function copyContextAction(
  id: string,
  label: string,
  text: string,
  separator = false,
): ContextAction {
  return { id, label, icon: Copy, separator, run: () => copyText(text) };
}
export function openContextAction(
  id: string,
  label: string,
  href?: string | null,
): ContextAction | null {
  const url = browserUrl(href ?? undefined);
  return url
    ? {
        id,
        label,
        icon: ArrowUpRight,
        run: async () => {
          if (isTauri()) await openUrl(url);
          else window.open(url, "_blank", "noopener,noreferrer");
        },
      }
    : null;
}
export function linkContextActions(
  href?: string | null,
  text?: string,
): ContextAction[] {
  const url = browserUrl(href ?? undefined);
  const open = openContextAction("open", t("links.openInBrowser"), url);
  return [
    ...(open
      ? [open, copyContextAction("copy-link", t("context.copyLink"), url!)]
      : []),
    ...(text
      ? [
          {
            ...copyContextAction(
              "copy-text",
              t("context.copyText"),
              text,
              !!open,
            ),
            icon: FileText,
          },
        ]
      : []),
  ];
}
export function modelContextActions(
  code: string,
  open: (code: string) => void,
  refresh: (code: string) => Promise<unknown>,
): ContextAction[] {
  const views: [ModelTab, string, ContextAction["icon"]][] = [
    ["overview", t("model.overview"), LayoutDashboard],
    ["branches", t("model.branches"), GitBranch],
    ["builds", t("model.builds"), Hammer],
    ["settings", t("model.settings"), Settings],
  ];
  const hasIssues = !!queryClient
    .getQueryData<FocusSummary>(summaryKey)
    ?.models.find((model) => model.code === code)?.issue_links?.length;
  return [
    ...views.map(([tab, label, icon]) => ({
      id: tab,
      label,
      icon,
      run: () => {
        queryClient.setQueryData([...focusKey, "view", code, "tab"], tab);
        open(code);
      },
    })),
    ...(["all", "unresolved", "severe", "fatal"] as const).map((filter) => ({
      id: `issues-${filter}`,
      label: t(`context.issues.${filter}`),
      disabled: !hasIssues,
      run: () => {
        queryClient.setQueryData(
          [...focusKey, "view", code, "issues-filter"],
          filter,
        );
        open(code);
      },
    })),
    {
      id: "refresh",
      label: t("model.refresh"),
      icon: RefreshCw,
      separator: true,
      run: () => {
        void refresh(code);
      },
    },
    copyContextAction("copy-code", t("context.copyModelCode"), code),
  ];
}

export function commitContextActions(
  item: {
    number: number;
    subject: string;
    url?: string;
    tracked?: boolean;
    project?: string;
    branch?: string;
    owner?: string;
    issues?: string[];
  },
  review?: (action: string) => void | Promise<unknown>,
): ContextAction[] {
  const actions = linkContextActions(item.url, item.subject).map((action) =>
    action.id === "copy-text"
      ? { ...action, label: t("context.copyCommitTitle") }
      : action,
  );
  actions.push(
    copyContextAction(
      "copy-number",
      t("context.copyCommitNumber"),
      String(item.number),
    ),
  );
  for (const [field, value, label] of [
    ["project", item.project, t("context.repository")],
    ["branch", item.branch, t("context.branch")],
    ["owner", item.owner, t("context.author")],
  ]) {
    if (!value) continue;
    const action = openContextAction(
      `search-${field}`,
      t("context.viewChanges", { name: label }),
      gerritSearchUrl(
        item.url,
        [
          field === "branch" && item.project
            ? gerritTerm("project", item.project)
            : "",
          gerritTerm(field!, value),
        ]
          .filter(Boolean)
          .join(" "),
      ),
    );
    if (action) actions.push(action);
    actions.push(
      copyContextAction(
        `copy-${field}`,
        t("context.copyField", { name: label }),
        value,
      ),
    );
  }
  if (item.issues?.length)
    actions.push(
      copyContextAction(
        "copy-issues",
        t("context.copyIssues"),
        item.issues.join("\n"),
      ),
    );
  if (review)
    actions.push(
      {
        id: "accept",
        label: t("model.accept"),
        icon: Check,
        separator: true,
        run: () => {
          void review("accept");
        },
      },
      {
        id: "track",
        label: item.tracked
          ? t("context.stopTracking")
          : t("model.keepTracking"),
        icon: item.tracked ? BookmarkX : Bookmark,
        run: () => {
          void review(item.tracked ? "untrack" : "track");
        },
      },
    );
  return actions;
}

export function buildContextActions(run: BuildRun): ContextAction[] {
  const console = openContextAction(
    "console",
    t("links.console"),
    consoleBrowserUrl(run),
  );
  return [
    ...linkContextActions(run.url),
    ...(console ? [console] : []),
    copyContextAction(
      "copy-build-number",
      t("context.copyBuildNumber"),
      String(run.number),
    ),
  ];
}

export function revealContextAction(path: string): ContextAction | null {
  if (!isTauri()) return null;
  return {
    id: "reveal",
    label: t("context.revealFolder"),
    icon: ArrowUpRight,
    run: () => revealItemInDir(path),
  };
}
