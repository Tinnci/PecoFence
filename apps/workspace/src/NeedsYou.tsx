import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  AlarmClock,
  ChevronDown,
  Ban,
  CalendarCheck,
  CircleX,
  Clock,
  GitPullRequest,
  Hourglass,
  RotateCcw,
  type LucideIcon,
} from "lucide-react";
import { focus } from "./api";
import { openContextAction } from "./context-actions";
import type { NeedItem, Needs } from "./contracts/setup";
import { focusKey, queryClient } from "./focus-cache";
import { t } from "./i18n";

const needsKey = [...focusKey, "needs"];
const REFRESH_MS = 30 * 60_000;
const icons: Record<string, LucideIcon> = {
  build_failed: CircleX,
  overdue: AlarmClock,
  review: GitPullRequest,
  reopened: RotateCcw,
  check_due: CalendarCheck,
  due_soon: Clock,
  blocked: Ban,
  stale: Hourglass,
};

/** The last answer at once, then a fresh one; the rail and the page share this. */
export function useNeeds() {
  const cached = useQuery(
    {
      queryKey: [...needsKey, "cached"],
      queryFn: async (): Promise<Needs | null> =>
        (await focus("needs_cached"))?.needs ?? null,
      staleTime: Infinity,
    },
    queryClient,
  );
  const fresh = useQuery(
    {
      queryKey: needsKey,
      queryFn: async (): Promise<Needs | null> =>
        (await focus("needs"))?.needs ?? null,
      staleTime: 5 * 60_000,
      refetchInterval: REFRESH_MS,
    },
    queryClient,
  );
  return fresh.data ?? cached.data;
}

/** What is actually held up; quiet items are worth a look but do not earn the badge. */
export function hardNeeds(items: NeedItem[]) {
  return items.filter((item) => item.kind !== "stale");
}

function open(item: NeedItem, onModel: (code: string) => void) {
  const source = openContextAction(item.id, item.title, item.url);
  if (source) void source.run();
  else if (item.model) onModel(item.model);
}

function NeedRows({
  items,
  onModel,
}: {
  items: NeedItem[];
  onModel: (code: string) => void;
}) {
  return (
    <ol>
      {items.map((item) => {
        const Icon = icons[item.kind] ?? Clock;
        return (
          <li key={`${item.kind}:${item.id}:${item.title}`}>
            <button type="button" onClick={() => open(item, onModel)}>
              <Icon
                aria-hidden="true"
                className={`need-icon need-${item.kind}`}
              />
              <span className="need-kind">{t(`needs.${item.kind}`)}</span>
              <span className="need-title">{item.title}</span>
              <span className="quiet">
                {[item.model, item.id].filter(Boolean).join(" · ")}
              </span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

/** One short queue: what cannot move until you do, with what has gone quiet folded below. */
export function NeedsYou({ onModel }: { onModel: (code: string) => void }) {
  useTranslation();
  const needs = useNeeds();
  if (!needs) return null;
  const hard = hardNeeds(needs.items);
  const quiet = needs.items.filter((item) => item.kind === "stale");
  return (
    <section className="needs" aria-label={t("needs.title")}>
      <header>
        <h2>{t("needs.title")}</h2>
        <strong>{hard.length}</strong>
      </header>
      {needs.error ? (
        <p className="short" role="alert">
          {t("needs.failed")}
        </p>
      ) : null}
      {hard.length ? (
        <NeedRows items={hard} onModel={onModel} />
      ) : (
        <p className="quiet">{t("needs.none")}</p>
      )}
      {quiet.length ? (
        <Collapsible className="my-group">
          <CollapsibleTrigger className="my-group-head">
            <ChevronDown aria-hidden="true" />
            <strong>{t("needs.stale")}</strong>
            <span className="quiet">{quiet.length}</span>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <NeedRows items={quiet} onModel={onModel} />
          </CollapsibleContent>
        </Collapsible>
      ) : null}
    </section>
  );
}
