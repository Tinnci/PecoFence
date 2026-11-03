import { useState } from "react";
import { useIsFetching, useQuery } from "@tanstack/react-query";
import {
  ChevronRight,
  FileText,
  History,
  GitMerge,
  GitPullRequestArrow,
  GitPullRequestClosed,
} from "lucide-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { ExternalLink } from "@/components/external-link";
import type { ChangeEvent, ChangeFile } from "./contracts/setup";
import { focus } from "./api";
import { queryClient } from "./focus-cache";
import { FailureNote, failureOf } from "./TemplateImport";
import { t } from "./i18n";

/** Gerrit's status as a pill: merged, open for review, abandoned, or the raw value. */
export function ChangeStatus({ status }: { status: string }) {
  const [Icon, label] =
    status === "MERGED"
      ? [GitMerge, t("model.merged")]
      : status === "ABANDONED"
        ? [GitPullRequestClosed, t("model.abandoned")]
        : status === "NEW"
          ? [GitPullRequestArrow, t("model.awaitingReview")]
          : [null, status];
  return (
    <span className={`change-status status-${status.toLowerCase()}`}>
      {Icon ? <Icon size={12} aria-hidden="true" /> : null}
      {label}
    </span>
  );
}

const kinds: Record<string, string> = {
  ADDED: "A",
  MODIFIED: "M",
  DELETED: "D",
  RENAMED: "R",
  COPIED: "C",
  REWRITE: "W",
};

/** Lines that stand in for content while it is read: a pulse, never a blank gap. */
function Loading({ lines = 3 }: { lines?: number }) {
  return (
    <div className="diff-loading" aria-busy="true">
      {Array.from({ length: lines }, (_, i) => (
        <Skeleton key={i} className="diff-loading-line" />
      ))}
    </div>
  );
}

// Change reads, in one place. A change's files and message are one read (about 1 s); every
// file's diff is one more read of the whole patchset (about 3 s), split per file here, so
// opening files never waits on Gerrit once that read lands. SPM keeps both on disk: merged and
// abandoned changes for 30 days, open ones for 10 minutes.
const FIVE_MINUTES = 5 * 60_000;
export const changeFilesOptions = (code: string, change: number) => ({
  queryKey: ["change-files", code, change] as const,
  queryFn: async () =>
    (await focus("change_files", { code, change })).change_files ?? null,
  staleTime: FIVE_MINUTES,
});
const diffsOptions = (code: string, change: number) => ({
  queryKey: ["change-diffs", code, change] as const,
  queryFn: async () =>
    (await focus("change_diffs", { code, change })).change_diffs ?? null,
  staleTime: FIVE_MINUTES,
  retry: false,
});
const diffKey = (code: string, change: number, path: string) =>
  ["change-diff", code, change, path] as const;
const diffOptions = (code: string, change: number, path: string) => ({
  queryKey: diffKey(code, change, path),
  queryFn: async () => {
    // The whole-patchset read is usually already under way; a file it holds needs no read.
    const all = await queryClient
      .fetchQuery(diffsOptions(code, change))
      .catch(() => null);
    const text = all?.files[path];
    if (text !== undefined)
      return {
        change,
        path,
        patch_set: all!.patch_set,
        diff: text,
        truncated: false,
      };
    return (
      (await focus("change_diff", { code, change, path })).change_diff ?? null
    );
  },
  staleTime: FIVE_MINUTES,
});

/** Files and message ahead of a click; the diffs once the change is open. */
export function prefetchChange(code: string, change: number, diffs = false) {
  void queryClient.prefetchQuery(changeFilesOptions(code, change));
  if (diffs) void queryClient.prefetchQuery(diffsOptions(code, change));
}

/** A change's message and files, each opening in place; read only when first expanded. */
export function ChangeFiles({
  code,
  change,
}: {
  code: string;
  change: number;
}) {
  const [open, setOpen] = useState(false);
  const read = useQuery(
    { ...changeFilesOptions(code, change), enabled: open },
    queryClient,
  );
  const list = read.data?.files;
  const added = list?.reduce((n, f) => n + f.insertions, 0) ?? 0;
  const removed = list?.reduce((n, f) => n + f.deletions, 0) ?? 0;
  return (
    <Collapsible
      open={open}
      onOpenChange={(next) => {
        if (next) prefetchChange(code, change, true);
        setOpen(next);
      }}
      className="change-files"
    >
      <CollapsibleTrigger
        className="change-files-toggle"
        onPointerEnter={() => prefetchChange(code, change)}
        onFocus={() => prefetchChange(code, change)}
      >
        {read.isFetching ? (
          <Spinner className="size-3" />
        ) : (
          <ChevronRight size={13} aria-hidden="true" className="chevron" />
        )}
        {list ? t("diff.files", { count: list.length }) : t("diff.changes")}
        {list?.length ? (
          <span className="diff-counts">
            <span className="added">+{added}</span>{" "}
            <span className="removed">−{removed}</span>
          </span>
        ) : null}
      </CollapsibleTrigger>
      <CollapsibleContent className="change-files-panel">
        {read.error ? <FailureNote failure={failureOf(read.error)} /> : null}
        {!read.data && !read.error ? <Loading /> : null}
        {read.data ? (
          <ul className="file-list">
            {read.data.message ? (
              <MessageRow message={read.data.message} />
            ) : null}
            {read.data.timeline?.length ? (
              <TimelineRow events={read.data.timeline} />
            ) : null}
            {list?.map((f) => (
              <FileRow key={f.path} code={code} change={change} file={f} />
            ))}
            {list && !list.length ? (
              <li className="quiet">{t("diff.noFiles")}</li>
            ) : null}
          </ul>
        ) : null}
      </CollapsibleContent>
    </Collapsible>
  );
}

const clock: Intl.DateTimeFormatOptions = {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
};

/** Gerrit's review history: who uploaded, voted, merged or abandoned, and when. */
function TimelineRow({ events }: { events: ChangeEvent[] }) {
  const closing = [...events]
    .reverse()
    .find((e) => e.kind === "merged" || e.kind === "abandoned");
  const people = events.filter((e) => !e.bot).length;
  return (
    <li>
      <Collapsible defaultOpen>
        <CollapsibleTrigger className="file-row message-row">
          <ChevronRight size={12} aria-hidden="true" className="chevron" />
          <History size={12} aria-hidden="true" className="file-kind" />
          <span className="file-path message-title">
            {t("gerrit.timeline")}
            <span className="file-dir">
              {" "}
              · {t("gerrit.steps", { count: people })}
              {closing
                ? ` · ${t(`gerrit.${closing.kind}`)} ${new Date(closing.at).toLocaleString(undefined, clock)}`
                : ""}
            </span>
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <ol className="gerrit-timeline">
            {events.map((e, i) => (
              <li
                key={i}
                className={`gerrit-event event-${e.kind}${e.bot ? " event-bot" : ""}`}
              >
                <time dateTime={e.at}>
                  {new Date(e.at).toLocaleString(undefined, clock)}
                </time>
                <span className="event-dot" aria-hidden="true" />
                <div className="event-body">
                  <span className="event-head">
                    <strong>{e.by || "Gerrit"}</strong>
                    <span className="event-kind">
                      {e.kind === "upload" && e.patch_set
                        ? t("gerrit.uploadSet", { n: e.patch_set })
                        : t(`gerrit.${e.kind}`)}
                    </span>
                    {e.votes.map(([label, value]) => (
                      <span
                        key={label}
                        className={`vote ${value.startsWith("-") ? "vote-down" : "vote-up"}${/[2-9]/.test(value) ? " vote-strong" : ""}`}
                      >
                        {label} {value}
                      </span>
                    ))}
                    {e.patch_set && e.kind !== "upload" ? (
                      <span className="event-ps">PS{e.patch_set}</span>
                    ) : null}
                  </span>
                  {eventText(e) ? (
                    <span className="event-text" title={e.text}>
                      <Linked text={eventText(e)} />
                    </span>
                  ) : null}
                </div>
              </li>
            ))}
          </ol>
        </CollapsibleContent>
      </Collapsible>
    </li>
  );
}

/** The message under its lead: votes and "Uploaded patch set" say nothing the row does not. */
function eventText(e: ChangeEvent) {
  return e.text
    .split("\n")
    .filter(
      (l, i) =>
        !(i === 0 && (e.votes.length || /^Uploaded patch set/i.test(l))),
    )
    .join("\n")
    .trim();
}

function MessageRow({ message }: { message: string }) {
  const parsed = parseMessage(message);
  return (
    <li>
      <Collapsible defaultOpen>
        <CollapsibleTrigger className="file-row message-row">
          <ChevronRight size={12} aria-hidden="true" className="chevron" />
          <FileText size={12} aria-hidden="true" className="file-kind" />
          <span className="file-path message-title">
            {t("diff.message")}
            <span className="file-dir"> · {parsed.title}</span>
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CommitMessage parsed={parsed} />
        </CollapsibleContent>
      </Collapsible>
    </li>
  );
}

function FileRow({
  code,
  change,
  file,
}: {
  code: string;
  change: number;
  file: ChangeFile;
}) {
  const [open, setOpen] = useState(false);
  const loading =
    useIsFetching({ queryKey: diffKey(code, change, file.path) }, queryClient) >
    0;
  const slash = file.path.lastIndexOf("/");
  const total = file.insertions + file.deletions;
  return (
    <li>
      <Collapsible open={open} onOpenChange={setOpen}>
        <CollapsibleTrigger className="file-row" title={file.path}>
          {loading ? (
            <Spinner className="size-3" />
          ) : (
            <ChevronRight size={12} aria-hidden="true" className="chevron" />
          )}
          <span
            className={`file-kind kind-${file.kind.toLowerCase()}`}
            title={file.kind}
          >
            {kinds[file.kind] ?? "?"}
          </span>
          <span className="file-path">
            {slash >= 0 ? (
              <span className="file-dir">{file.path.slice(0, slash + 1)}</span>
            ) : null}
            {file.path.slice(slash + 1)}
          </span>
          <span className="diff-counts">
            <span className="added">+{file.insertions}</span>{" "}
            <span className="removed">−{file.deletions}</span>
          </span>
          <span className="diff-bar" aria-hidden="true">
            <span
              className="added"
              style={{ flexGrow: total ? file.insertions : 0 }}
            />
            <span
              className="removed"
              style={{ flexGrow: total ? file.deletions : 0 }}
            />
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          {open ? (
            <FileDiff code={code} change={change} path={file.path} />
          ) : null}
        </CollapsibleContent>
      </Collapsible>
    </li>
  );
}

function FileDiff({
  code,
  change,
  path,
}: {
  code: string;
  change: number;
  path: string;
}) {
  const diff = useQuery(diffOptions(code, change, path), queryClient);
  if (diff.error) return <FailureNote failure={failureOf(diff.error)} />;
  if (!diff.data) return <Loading lines={6} />;
  return (
    <>
      <DiffView text={diff.data.diff} />
      {diff.data.truncated ? (
        <p className="quiet diff-truncated">{t("diff.truncated")}</p>
      ) : null}
    </>
  );
}

export type Message = {
  /** Bracketed tags opening the subject: [HM][P6700LV][SPM][Feature]. */
  tags: string[];
  title: string;
  /** key: value lines, a key alone on its line taking the lines under it. */
  fields: { key: string; value: string }[];
  /** Paragraphs that are not fields. */
  text: string[];
  /** Git trailers: Change-Id, Signed-off-by and the like. */
  trailers: { key: string; value: string }[];
};

const TRAILER =
  /^(Change-Id|Signed-off-by|Reviewed-on|Reviewed-by|Tested-by|Acked-by|Cc|Co-authored-by|Bug|Test):\s*(.*)$/i;
const FIELD = /^([^\s:：][^:：]{0,24}?)\s*[:：]\s*(.*)$/;

/** The team's commit layout: tagged subject, key: value fields, then git trailers. */
export function parseMessage(message: string): Message {
  const lines = message.replace(/\r/g, "").split("\n");
  const subject = lines[0] ?? "";
  const tags: string[] = [];
  let rest = subject.trim();
  for (
    let m = /^\[([^\]]{1,40})\]/.exec(rest);
    m;
    m = /^\[([^\]]{1,40})\]/.exec(rest)
  ) {
    tags.push(m[1]);
    rest = rest.slice(m[0].length).trimStart();
  }
  const title = rest.replace(/^[:：]\s*/, "") || subject;
  const out: Message = { tags, title, fields: [], text: [], trailers: [] };
  let open: { key: string; value: string } | null = null;
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length) out.text.push(paragraph.join("\n"));
    paragraph = [];
  };
  for (const raw of lines.slice(1)) {
    const line = raw.trim();
    if (!line) {
      open = null;
      flush();
      continue;
    }
    const trailer = TRAILER.exec(line);
    if (trailer) {
      flush();
      open = null;
      out.trailers.push({ key: trailer[1], value: trailer[2] });
      continue;
    }
    const field = FIELD.exec(line);
    // "http://..." is a link, not a key.
    if (field && !field[2].startsWith("//")) {
      flush();
      open = { key: field[1], value: field[2] };
      out.fields.push(open);
      continue;
    }
    if (open && !open.value) {
      open.value = line;
      continue;
    }
    if (open) {
      open.value += "\n" + line;
      continue;
    }
    paragraph.push(line);
  }
  flush();
  return out;
}

/** Text with its links made clickable. */
function Linked({ text }: { text: string }) {
  const parts = text.split(/(https?:\/\/[^\s，。、)）]+)/g);
  return (
    <>
      {parts.map((p, i) =>
        /^https?:\/\//.test(p) ? (
          <ExternalLink key={i} href={p}>
            {p}
          </ExternalLink>
        ) : (
          p
        ),
      )}
    </>
  );
}

/** A verdict like [PASS] or FAIL as a pill. */
function Value({ value }: { value: string }) {
  const verdict = /^\[?\s*(PASS|PASSED|OK|FAIL|FAILED|NA|N\/A)\s*\]?$/i.exec(
    value,
  );
  if (verdict) {
    const pass = /^(PASS|PASSED|OK)$/i.test(verdict[1]);
    return (
      <span className={`verdict ${pass ? "verdict-pass" : "verdict-fail"}`}>
        {verdict[1].toUpperCase()}
      </span>
    );
  }
  return <Linked text={value} />;
}

function CommitMessage({ parsed }: { parsed: Message }) {
  return (
    <article className="commit-message" aria-label={t("diff.message")}>
      <div className="commit-subject">
        {parsed.tags.map((tag) => (
          <span key={tag} className="commit-tag">
            {tag}
          </span>
        ))}
        <strong>{parsed.title}</strong>
      </div>
      {parsed.text.map((p, i) => (
        <p key={i}>
          <Linked text={p} />
        </p>
      ))}
      {parsed.fields.length ? (
        <dl className="commit-fields">
          {parsed.fields.map((f, i) => (
            <div key={i}>
              <dt>{f.key}</dt>
              <dd>{f.value ? <Value value={f.value} /> : "—"}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {parsed.trailers.length ? (
        <dl className="commit-trailers">
          {parsed.trailers.map((f, i) => (
            <div key={i}>
              <dt>{f.key}</dt>
              <dd>{f.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
    </article>
  );
}

type Line = {
  kind: "hunk" | "add" | "del" | "ctx" | "meta";
  text: string;
  old?: number;
  new?: number;
};

/** Unified diff lines with old and new line numbers taken from each hunk header. */
export function diffLines(text: string): Line[] {
  const out: Line[] = [];
  let old = 0;
  let next = 0;
  let inHunk = false;
  for (const raw of text.split("\n")) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(raw);
    if (hunk) {
      old = Number(hunk[1]);
      next = Number(hunk[2]);
      inHunk = true;
      out.push({ kind: "hunk", text: raw });
    } else if (!inHunk) {
      if (raw) out.push({ kind: "meta", text: raw });
    } else if (raw.startsWith("+")) {
      out.push({ kind: "add", text: raw.slice(1), new: next++ });
    } else if (raw.startsWith("-")) {
      out.push({ kind: "del", text: raw.slice(1), old: old++ });
    } else if (raw.startsWith("\\")) {
      out.push({ kind: "meta", text: raw });
    } else if (raw.startsWith("diff --git ")) {
      inHunk = false;
      out.push({ kind: "meta", text: raw });
    } else {
      out.push({ kind: "ctx", text: raw.slice(1), old: old++, new: next++ });
    }
  }
  // A trailing newline leaves one empty context line behind.
  while (
    out.length &&
    out[out.length - 1].kind === "ctx" &&
    !out[out.length - 1].text
  )
    out.pop();
  return out;
}

function DiffView({ text }: { text: string }) {
  const lines = diffLines(text).filter((l) => l.kind !== "meta");
  if (!lines.length) return <p className="quiet">{t("diff.binary")}</p>;
  return (
    <div className="diff-view" role="region" aria-label={t("diff.view")}>
      <table>
        <tbody>
          {lines.map((l, i) => (
            <tr key={i} className={`diff-${l.kind}`}>
              <td className="ln">{l.old ?? ""}</td>
              <td className="ln">{l.new ?? ""}</td>
              <td className="sign">
                {l.kind === "add" ? "+" : l.kind === "del" ? "−" : ""}
              </td>
              <td className="code">{l.text}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
