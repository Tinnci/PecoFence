import {
  Blocks,
  Globe,
  Mail,
  Users,
  ChartColumn,
  Terminal,
  Package,
  Activity,
  ArrowDownToLine,
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  Bookmark,
  BookmarkMinus,
  Bug,
  Check,
  CircleDashed,
  ClipboardList,
  Copy,
  FilePlus2,
  FolderOpen,
  GitBranch,
  GitCommitHorizontal,
  Hammer,
  HeartPulse,
  Languages,
  LayoutDashboard,
  ListFilter,
  ListChecks,
  ListTodo,
  LogIn,
  OctagonAlert,
  Plus,
  Save,
  Search,
  Settings,
  ShieldAlert,
  Undo2,
  User,
  X,
  ChevronDown,
  ChevronUp,
  RefreshCw,
  Unplug,
  type LucideIcon,
} from "lucide-react";

const icons = {
  overview: LayoutDashboard,
  branches: GitBranch,
  builds: Hammer,
  settings: Settings,
  issues: Bug,
  requirements: ListTodo,
  checklist: ClipboardList,
  check: Check,
  track: Bookmark,
  untrack: BookmarkMinus,
  save: Save,
  discard: Undo2,
  close: X,
  back: ArrowLeft,
  next: ArrowRight,
  add: Plus,
  remove: BookmarkMinus,
  folder: FolderOpen,
  search: Search,
  activity: Activity,
  health: HeartPulse,
  sources: Unplug,
  language: Languages,
  configure: Settings,
  expand: ChevronDown,
  collapse: ChevronUp,
  copy: Copy,
  open: ArrowUpRight,
  project: FilePlus2,
  follow: Bookmark,
  refresh: RefreshCw,
  account: User,
  repository: GitBranch,
  commit: GitCommitHorizontal,
  all: ListFilter,
  unresolved: CircleDashed,
  severe: ShieldAlert,
  fatal: OctagonAlert,
  setup: ListChecks,
  import: ArrowDownToLine,
  signIn: LogIn,
  diagnostic: ClipboardList,
} satisfies Record<string, LucideIcon>;
export type IconName = keyof typeof icons;

/** Decorative label icons disappear together when the window needs room for text. */
export function ActionIcon({
  name,
  responsive = true,
}: {
  name: IconName;
  responsive?: boolean;
}) {
  const Icon = icons[name];
  return (
    <Icon
      aria-hidden="true"
      focusable="false"
      className={responsive ? "action-icon responsive-icon" : "action-icon"}
    />
  );
}

/** Menu actions share the same semantics even when supplied by different screens. */
export function menuActionIcon(id: string): LucideIcon {
  if (/^copy/.test(id)) return Copy;
  if (/issues-|^(all|unresolved|severe|fatal)$/.test(id))
    return icons[id.replace("issues-", "") as IconName] ?? Bug;
  if (/^(configure|settings)$/.test(id)) return Settings;
  if (/^(refresh|recheck)$/.test(id)) return RefreshCw;
  if (/branch|^view$/.test(id)) return GitBranch;
  if (/project|repo/.test(id)) return GitBranch;
  if (/owner|account/.test(id)) return User;
  if (/console/.test(id)) return ClipboardList;
  if (/build|^job$/.test(id)) return Hammer;
  if (/reveal/.test(id)) return FolderOpen;
  if (/^(open|search)/.test(id)) return ArrowUpRight;
  if (/accept/.test(id)) return Check;
  if (/track/.test(id)) return Bookmark;
  return ListChecks;
}

function componentIcon(id: string): LucideIcon {
  if (/jenkins|build|bld/.test(id)) return Hammer;
  if (/jira|meegle/.test(id)) return Bug;
  if (/gerrit|grr|^git$/.test(id)) return GitBranch;
  if (/spm/.test(id)) return ClipboardList;
  if (/cdp|browser|chrome|forwarder/.test(id)) return Globe;
  if (/mail|heed/.test(id)) return Mail;
  if (/people|wecom/.test(id)) return Users;
  if (/visualizer/.test(id)) return ChartColumn;
  if (/pecofence/.test(id)) return LayoutDashboard;
  if (/wsl|terminal|shell/.test(id)) return Terminal;
  if (/python|rust|node|^uv$/.test(id)) return Package;
  return Blocks;
}
export function ComponentIcon({
  id,
  responsive = true,
}: {
  id: string;
  responsive?: boolean;
}) {
  const Icon = componentIcon(id);
  return (
    <Icon
      aria-hidden="true"
      focusable="false"
      className={responsive ? "action-icon responsive-icon" : "action-icon"}
    />
  );
}
