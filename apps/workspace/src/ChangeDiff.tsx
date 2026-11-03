import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronRight,
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
import type { ChangeFile } from "./contracts/setup";
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

/** A change's files, each opening its diff; read only when first expanded. */
export function ChangeFiles({
  code,
  change,
}: {
  code: string;
  change: number;
}) {
  const [open, setOpen] = useState(false);
  const files = useQuery(
    {
      queryKey: ["change-files", code, change],
      queryFn: async () =>
        (await focus("change_files", { code, change })).change_files?.files ??
        [],
      enabled: open,
      staleTime: 5 * 60_000,
    },
    queryClient,
  );
  const list = files.data;
  const added = list?.reduce((n, f) => n + f.insertions, 0) ?? 0;
  const removed = list?.reduce((n, f) => n + f.deletions, 0) ?? 0;
  return (
    <Collapsible open={open} onOpenChange={setOpen} className="change-files">
      <CollapsibleTrigger className="change-files-toggle">
        <ChevronRight size={13} aria-hidden="true" className="chevron" />
        {list ? t("diff.files", { count: list.length }) : t("diff.changes")}
        {list?.length ? (
          <span className="diff-counts">
            <span className="added">+{added}</span>{" "}
            <span className="removed">−{removed}</span>
          </span>
        ) : null}
      </CollapsibleTrigger>
      <CollapsibleContent className="change-files-panel">
        {files.error ? <FailureNote failure={failureOf(files.error)} /> : null}
        {!list && !files.error ? <Skeleton className="diff-skeleton" /> : null}
        {list ? (
          list.length ? (
            <ul className="file-list">
              {list.map((f) => (
                <FileRow key={f.path} code={code} change={change} file={f} />
              ))}
            </ul>
          ) : (
            <p className="quiet">{t("diff.noFiles")}</p>
          )
        ) : null}
      </CollapsibleContent>
    </Collapsible>
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
  const slash = file.path.lastIndexOf("/");
  const total = file.insertions + file.deletions;
  return (
    <li>
      <Collapsible open={open} onOpenChange={setOpen}>
        <CollapsibleTrigger className="file-row" title={file.path}>
          <ChevronRight size={12} aria-hidden="true" className="chevron" />
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
  const diff = useQuery(
    {
      queryKey: ["change-diff", code, change, path],
      queryFn: async () =>
        (await focus("change_diff", { code, change, path })).change_diff ??
        null,
      staleTime: 5 * 60_000,
    },
    queryClient,
  );
  if (diff.error) return <FailureNote failure={failureOf(diff.error)} />;
  if (!diff.data) return <Skeleton className="diff-skeleton" />;
  return (
    <>
      <DiffView text={diff.data.diff} />
      {diff.data.truncated ? (
        <p className="quiet diff-truncated">{t("diff.truncated")}</p>
      ) : null}
    </>
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
