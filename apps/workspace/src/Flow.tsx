import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ChevronRight, RefreshCw } from "lucide-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import type { FlowEvent, WorkItem } from "./contracts/setup";
import { flowOptions, refreshFlow } from "./flow-cache";
import { queryClient } from "./focus-cache";
import { FailureNote, failureOf } from "./TemplateImport";
import { OwnerChip } from "./Badges";
import { Linked } from "./ChangeDiff";
import { t } from "./i18n";

const clock: Intl.DateTimeFormatOptions = {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
};
const when = (at: string) => {
  const d = new Date(at);
  return Number.isNaN(d.getTime()) ? at : d.toLocaleString(undefined, clock);
};

// 飞书项目 records every field a transition form touches, and its workflow keeps internal
// fields of its own. What a reader needs: moves with the form filled for them, people
// assigned, edits that say something, comments.
const internal = new Set([
  "当前状态授权角色",
  "关联状态流转",
  "关联评审",
  "状态",
]);
const isKey = (v: string) =>
  /^\d{15,}$/.test(v) || /^role_/.test(v) || /^\d+_[a-z0-9]{4,}$/i.test(v);
const near = (a: FlowEvent, b: FlowEvent) =>
  a.by === b.by && Math.abs(Date.parse(a.at) - Date.parse(b.at)) <= 120_000;

/** Comment markdown as a reader sees it: images become a link, hidden markers go. */
function readable(text: string) {
  const clean = text.replace(/<!--[\s\S]*?-->/g, "");
  // Parse inertly, then extract text. Never render source HTML or its styles.
  if (
    !/<\/?(?:span|div|p|br|a|img|pre|code|strong|em|b|i|ul|ol|li|blockquote|h[1-6]|table|tr|td)\b[^>]*>|&(?:#\d+|#x[\da-f]+|[a-z]+);/i.test(
      clean,
    )
  )
    return clean
      .replace(/!\[[^\]]*\]\((\S+?)\)/g, `${t("flow.image")} $1`)
      .trim();
  const doc = new DOMParser().parseFromString(clean, "text/html");
  doc
    .querySelectorAll("script, style, iframe, object")
    .forEach((n) => n.remove());
  doc.querySelectorAll("br").forEach((n) => n.replaceWith("\n"));
  doc.querySelectorAll("a[href]").forEach((n) => {
    const href = n.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) {
      const label = n.textContent || href;
      n.replaceWith(label === href ? href : `[${label}](${href})`);
    }
  });
  doc.querySelectorAll("img").forEach((n) => {
    const src = n.getAttribute("src") ?? "";
    n.replaceWith(
      /^https?:\/\//i.test(src)
        ? `${t("flow.image")} ${src}`
        : (n.getAttribute("alt") ?? ""),
    );
  });
  doc
    .querySelectorAll("p, div, li, pre, blockquote, h1, h2, h3, tr")
    .forEach((n) => n.append("\n"));
  return (doc.body.textContent ?? "")
    .replace(/!\[[^\]]*\]\((\S+?)\)/g, `${t("flow.image")} $1`)
    .trim();
}

export type Shaped = FlowEvent & {
  details: { field: string; from: string; value: string }[];
};
const last = (list: Shaped[], test: (o: Shaped) => boolean) => {
  for (let i = list.length - 1; i >= 0; i--) if (test(list[i])) return list[i];
  return undefined;
};

/** Events as read: merged, grouped and cleared of what only the workflow needs. */
export function shapeFlow(events: FlowEvent[]): Shaped[] {
  const out: Shaped[] = [];
  const edits: FlowEvent[] = [];
  for (const e of events) {
    if (e.kind === "field") {
      if (!internal.has(e.field) && e.to && !isKey(e.to)) edits.push(e);
      continue;
    }
    if (e.kind === "assign") {
      const people = (v: string) =>
        v
          .split("、")
          .filter(Boolean)
          .map((p) => (/^\d{15,}$/.test(p) ? t("flow.formerMember") : p));
      const to = people(e.to);
      const from = people(e.from).filter((p) => !to.includes(p));
      const same = last(
        out,
        (o) => o.kind === "assign" && o.field === e.field && near(o, e),
      );
      if (same) {
        same.to = [
          ...new Set([...same.to.split("、").filter(Boolean), ...to]),
        ].join("、");
        same.from = [
          ...new Set([...same.from.split("、").filter(Boolean), ...from]),
        ]
          .filter((p) => !same.to.includes(p))
          .join("、");
        continue;
      }
      out.push({ ...e, to: to.join("、"), from: from.join("、"), details: [] });
      continue;
    }
    out.push({ ...e, text: readable(e.text), details: [] });
  }
  // A form filled during a move belongs to that move; other edits close in time group.
  for (const e of edits) {
    // The form belongs to the move written nearest to it, not the first one nearby.
    const move = out
      .filter((o) => o.kind === "status" && near(o, e))
      .sort(
        (a, b) =>
          Math.abs(Date.parse(a.at) - Date.parse(e.at)) -
          Math.abs(Date.parse(b.at) - Date.parse(e.at)),
      )[0];
    const group =
      move ??
      last(out, (o) => o.kind === "field" && near(o, e)) ??
      (() => {
        const g: Shaped = { ...e, kind: "field", to: "", details: [] };
        out.push(g);
        return g;
      })();
    group.details.push({ field: e.field, from: e.from, value: e.to });
  }
  return out.sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
}

/** A 飞书项目 item's 流转记录, read when first opened and ahead of it on hover. */
export function ItemFlow({
  code,
  item,
  expand = false,
}: {
  code: string;
  item: WorkItem;
  /** Held open, e.g. while the whole view is exported. */
  expand?: boolean;
}) {
  const { i18n } = useTranslation();
  const [opened, setOpen] = useState(false);
  const open = opened || expand;
  const flow = useQuery(
    { ...flowOptions(code, item), enabled: open && item.source === "meegle" },
    queryClient,
  );
  const events = useMemo(
    () => (flow.data ? shapeFlow(flow.data.events) : undefined),
    [flow.data, i18n.resolvedLanguage],
  );
  if (item.source !== "meegle") return null;
  const moves = events?.filter((e) => e.kind === "status").length ?? 0;
  return (
    <Collapsible open={open} onOpenChange={setOpen} className="item-flow">
      <CollapsibleTrigger
        className="change-files-toggle"
        onPointerEnter={() =>
          void queryClient.prefetchQuery(flowOptions(code, item))
        }
      >
        {flow.isFetching ? (
          <Spinner className="size-3" />
        ) : (
          <ChevronRight size={13} aria-hidden="true" className="chevron" />
        )}
        {t("flow.title")}
        {events ? (
          <span className="file-dir">
            {" "}
            · {t("flow.moves", { count: moves })}
          </span>
        ) : null}
      </CollapsibleTrigger>
      <CollapsibleContent className="change-files-panel">
        <button
          type="button"
          className="link no-export"
          disabled={flow.isFetching}
          onClick={() => void refreshFlow(code, item).catch(() => undefined)}
        >
          <RefreshCw size={12} aria-hidden="true" /> {t("work.refresh")}
        </button>
        {flow.error ? <FailureNote failure={failureOf(flow.error)} /> : null}
        {!flow.data && !flow.error ? (
          <div className="diff-loading" aria-busy="true">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="diff-loading-line" />
            ))}
          </div>
        ) : null}
        {events ? (
          events.length ? (
            <ol className="gerrit-timeline">
              {events.map((e, i) => (
                <FlowRow key={i} e={e} />
              ))}
            </ol>
          ) : (
            <p className="quiet">{t("flow.none")}</p>
          )
        ) : null}
      </CollapsibleContent>
    </Collapsible>
  );
}

function FlowRow({ e }: { e: Shaped }) {
  const [full, setFull] = useState(false);
  const long = e.text.length > 160 || e.text.split("\n").length > 3;
  return (
    <li className={`gerrit-event event-${e.kind}${e.bot ? " event-bot" : ""}`}>
      <time dateTime={e.at} title={new Date(e.at).toLocaleString()}>
        {when(e.at)}
      </time>
      <span className="event-dot" aria-hidden="true" />
      <div className="event-body">
        <span className="event-head">
          <strong>{e.by || t("flow.automation")}</strong>
          <span className="event-kind">
            {e.kind === "assign"
              ? t("flow.assign", { field: e.field })
              : e.kind === "field"
                ? e.details.length > 1
                  ? t("flow.fields", { count: e.details.length })
                  : t("flow.field", { field: e.details[0]?.field ?? e.field })
                : t(`flow.${e.kind}`)}
          </span>
          {e.kind === "status" ? (
            <span className="flow-move">
              {e.from ? <span className="flow-from">{e.from}</span> : null}
              <span aria-hidden="true">→</span>
              <span className="flow-to">{e.to}</span>
            </span>
          ) : null}
          {e.kind === "assign" ? (
            <>
              {e.to
                ? e.to
                    .split("、")
                    .map((name) => <OwnerChip key={name} name={name} />)
                : null}
              {e.from ? <span className="flow-from">{e.from}</span> : null}
            </>
          ) : null}
          {e.kind === "field" && e.details.length === 1 ? (
            <span className="flow-value">
              {e.details[0].from ? (
                <>
                  <span className="flow-from">{e.details[0].from}</span>
                  {" → "}
                </>
              ) : null}
              <Linked text={e.details[0].value} />
            </span>
          ) : null}
        </span>
        {e.text ? (
          <>
            <span
              className={`event-text flow-comment${!long || full ? " flow-comment-full" : ""}`}
            >
              <Linked text={e.text} />
            </span>
            {long ? (
              <button
                type="button"
                className="link no-export"
                aria-expanded={full}
                onClick={() => setFull(!full)}
              >
                {t(full ? "flow.collapseText" : "flow.expandText")}
              </button>
            ) : null}
          </>
        ) : null}
        {e.details.length > (e.kind === "field" ? 1 : 0) ? (
          <dl className="flow-form">
            {e.details.map((d, i) => (
              <div key={i}>
                <dt>{d.field}</dt>
                <dd title={d.value}>
                  {d.from ? (
                    <>
                      <span className="flow-from">{d.from}</span>
                      {" → "}
                    </>
                  ) : null}
                  <Linked text={d.value} />
                </dd>
              </div>
            ))}
          </dl>
        ) : null}
      </div>
    </li>
  );
}
