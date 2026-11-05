import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  Circle,
  CircleCheck,
  CircleMinus,
  CircleX,
  ClipboardList,
  Copy,
  Plus,
  ArrowRight,
  Link2,
  ShieldCheck,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { ExternalLink } from "@/components/external-link";
import { copyText } from "./clipboard";
import { t } from "./i18n";
import { shortTime } from "./time-format";
import { Textarea } from "./components/ui/textarea";
import {
  checkCounts,
  checkTemplates,
  confirmed,
  emptyEntry,
  readReleaseChecks,
  writeReleaseChecks,
  type CheckEntry,
  type CheckPhase,
  type CheckStatus,
  type ReleaseCheck,
} from "./release-checklist";
import "./release-checklist.css";

const statuses: CheckStatus[] = ["pending", "passed", "blocked", "na"];
const statusIcons = {
  pending: Circle,
  passed: CircleCheck,
  blocked: CircleX,
  na: CircleMinus,
};
function StatusMark({ status }: { status: CheckStatus }) {
  const Icon = statusIcons[status];
  return (
    <Icon
      size={17}
      className={`release-status-icon ${status}`}
      aria-hidden="true"
    />
  );
}
function load(code: string) {
  try {
    return { releases: readReleaseChecks(code), failed: false };
  } catch {
    return { releases: [] as ReleaseCheck[], failed: true };
  }
}

export default function ReleaseChecklist({
  code,
  branches = [],
}: {
  code: string;
  branches?: string[];
}) {
  useTranslation();
  const [store, setStore] = useState(() => load(code));
  const [selectedRelease, setSelectedRelease] = useState<string | null>(null);
  const [phase, setPhase] = useState<CheckPhase>("plan");
  const [selected, setSelected] = useState<string>("scope");
  const [filter, setFilter] = useState("all");
  const [creating, setCreating] = useState(false);
  const [drafts, setDrafts] = useState<Record<string, CheckEntry>>({});
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const release =
    store.releases.find((r) => r.id === selectedRelease) ?? store.releases[0];
  const totals = release ? checkCounts(release) : null;
  const phaseTotals = release ? checkCounts(release, phase) : null;
  const items = checkTemplates.filter((item) => item.phase === phase);
  const visible = items.filter(
    (item) =>
      filter === "all" ||
      (filter === "blocked"
        ? release?.entries[item.id]?.status === "blocked"
        : !confirmed(release?.entries[item.id])),
  );
  // Keep the editor on a visible item after a filter or phase changes.
  const item = visible.find((item) => item.id === selected) ?? visible[0];
  const draftKey = release && item ? `${release.id}/${item.id}` : "";
  const entry =
    release && item ? (release.entries[item.id] ?? emptyEntry()) : emptyEntry();
  const draft = drafts[draftKey] ?? entry;
  const dirty = !!drafts[draftKey];
  const unsaved = release
    ? Object.keys(drafts).filter((key) => key.startsWith(`${release.id}/`))
        .length
    : 0;
  function commit(update: (latest: ReleaseCheck[]) => ReleaseCheck[]) {
    try {
      const next = update(readReleaseChecks(code));
      writeReleaseChecks(code, next);
      setStore({ releases: next, failed: false });
      setError("");
      setCopied(false);
      return true;
    } catch (failure) {
      setError(
        failure instanceof Error && failure.message === "checklist_conflict"
          ? t("releaseCheck.conflict")
          : t("releaseCheck.saveFailed"),
      );
      return false;
    }
  }
  function change(patch: Partial<CheckEntry>) {
    setDrafts((old) => ({ ...old, [draftKey]: { ...draft, ...patch } }));
    setError("");
  }
  function saveEntry(event: React.FormEvent) {
    event.preventDefault();
    if (!release || !item) return;
    const next = {
      ...draft,
      owner: draft.owner.trim(),
      note: draft.note.trim(),
      evidence: draft.evidence.trim(),
      updatedAt: new Date().toISOString(),
    };
    if (
      (next.status === "passed" || next.status === "na") &&
      !confirmed(next)
    ) {
      setError(t("releaseCheck.validation"));
      return;
    }
    if (next.status === "blocked" && (!next.owner || !next.note)) {
      setError(t("releaseCheck.blockValidation"));
      return;
    }
    if (
      commit((latest) => {
        const current = latest.find((r) => r.id === release.id);
        if (
          !current ||
          (current.entries[item.id]?.updatedAt ?? "") !== entry.updatedAt
        )
          throw new Error("checklist_conflict");
        return latest.map((r) =>
          r.id === release.id
            ? { ...r, entries: { ...r.entries, [item.id]: next } }
            : r,
        );
      })
    )
      setDrafts((old) => {
        const next = { ...old };
        delete next[draftKey];
        return next;
      });
  }
  async function copyReport() {
    if (!release) return;
    const lines = [
      `# ${code} · ${release.name}`,
      `${t("releaseCheck.baseline")}: ${release.baseline}`,
      `${t("releaseCheck.branch")}: ${release.branch}`,
      `${t("releaseCheck.target")}: ${release.target || "—"}`,
    ];
    for (const phase of ["plan", "release"] as const) {
      lines.push(`\n## ${t(`releaseCheck.phase.${phase}`)}`);
      for (const item of checkTemplates.filter(
        (item) => item.phase === phase,
      )) {
        const e = release.entries[item.id] ?? emptyEntry();
        lines.push(
          `\n- [${confirmed(e) ? "x" : " "}] ${item.priority} ${t(`releaseCheck.item.${item.id}`)} · ${t(`releaseCheck.status.${e.status}`)}`,
          `  ${t("releaseCheck.owner")}: ${e.owner || "—"}`,
        );
        if (e.note) lines.push(`  ${e.note.replaceAll("\n", "\n  ")}`);
        if (e.evidence)
          lines.push(
            `  ${t("releaseCheck.evidence")}: ${e.evidence.replaceAll("\n", "\n  ")}`,
          );
        if (e.updatedAt) lines.push(`  ${e.updatedAt}`);
      }
    }
    try {
      await copyText(lines.join("\n"));
      setCopied(true);
      setError("");
    } catch {
      setError(t("releaseCheck.copyFailed"));
    }
  }
  return (
    <section className="release-checklist" aria-label={t("releaseCheck.title")}>
      <header className="release-heading">
        <div>
          <h2>{t("releaseCheck.title")}</h2>
          <p>{t("releaseCheck.subtitle")}</p>
        </div>
        <div className="release-actions">
          <Button
            variant="ghost"
            size="icon"
            aria-label={t("releaseCheck.retry")}
            title={t("releaseCheck.retry")}
            onClick={() => {
              setStore(load(code));
              setError("");
            }}
          >
            <RefreshCw />
          </Button>
          {release ? (
            <Button
              variant="outline"
              disabled={unsaved > 0}
              title={unsaved ? t("releaseCheck.unsavedReport") : undefined}
              onClick={() => void copyReport()}
            >
              <Copy />
              {t(copied ? "releaseCheck.copied" : "releaseCheck.copy")}
            </Button>
          ) : null}
          <Button onClick={() => setCreating(true)} disabled={store.failed}>
            <Plus />
            {t("releaseCheck.new")}
          </Button>
        </div>
      </header>
      {store.failed ? (
        <div className="release-message" role="alert">
          <p>{t("releaseCheck.loadFailed")}</p>
          <Button variant="outline" onClick={() => setStore(load(code))}>
            {t("releaseCheck.retry")}
          </Button>
        </div>
      ) : null}
      {error ? (
        <p className="release-message" role="alert">
          {error}
        </p>
      ) : null}
      {!release && !store.failed ? (
        <div className="release-empty">
          <ClipboardList size={30} aria-hidden="true" />
          <h3>{t("releaseCheck.emptyTitle")}</h3>
          <p>{t("releaseCheck.emptyBody")}</p>
          <Button onClick={() => setCreating(true)}>
            {t("releaseCheck.create")}
            <ArrowRight />
          </Button>
        </div>
      ) : release && totals && phaseTotals ? (
        <>
          <div className="release-summary">
            <div className="release-version">
              <Select
                value={release.id}
                onValueChange={(value) => {
                  setSelectedRelease(value);
                  setCopied(false);
                  setError("");
                }}
                items={store.releases.map((r) => ({
                  value: r.id,
                  label: r.name,
                }))}
              >
                <SelectTrigger aria-label={t("releaseCheck.chooseVersion")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {store.releases.map((r) => (
                    <SelectItem key={r.id} value={r.id}>
                      {r.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <span
                className={`release-gate ${totals.blocked ? "blocked" : totals.done === totals.total ? "passed" : "pending"}`}
              >
                {totals.blocked ? (
                  <CircleX size={15} />
                ) : totals.done === totals.total ? (
                  <ShieldCheck size={15} />
                ) : (
                  <Circle size={15} />
                )}
                {totals.blocked
                  ? t("releaseCheck.blockedCount", { count: totals.blocked })
                  : t(
                      totals.done === totals.total
                        ? "releaseCheck.complete"
                        : "releaseCheck.inProgress",
                    )}
              </span>
            </div>
            <dl className="release-baseline">
              <div>
                <dt>{t("releaseCheck.baseline")}</dt>
                <dd>{release.baseline}</dd>
              </div>
              <div>
                <dt>{t("releaseCheck.branch")}</dt>
                <dd>{release.branch}</dd>
              </div>
              {release.target ? (
                <div>
                  <dt>{t("releaseCheck.target")}</dt>
                  <dd>{release.target}</dd>
                </div>
              ) : null}
            </dl>
            <div className="release-progress-label">
              <span>
                {t("releaseCheck.confirmed", {
                  done: totals.done,
                  total: totals.total,
                })}
              </span>
              <span>
                {unsaved
                  ? t("releaseCheck.unsavedCount", { count: unsaved })
                  : t("releaseCheck.local")}
              </span>
            </div>
            <Progress
              value={(totals.done / totals.total) * 100}
              aria-label={t("releaseCheck.progress")}
            />
          </div>
          <div className="release-phase-bar">
            <div
              className="release-phases"
              role="group"
              aria-label={t("releaseCheck.phaseLabel")}
            >
              {(["plan", "release"] as const).map((p, i) => {
                const c = checkCounts(release, p);
                return (
                  <Button
                    key={p}
                    variant="ghost"
                    aria-pressed={phase === p}
                    onClick={() => {
                      setPhase(p);
                      setFilter("all");
                    }}
                  >
                    <span className="release-phase-number">
                      {c.done === c.total ? <Check size={13} /> : i + 1}
                    </span>
                    {t(`releaseCheck.phase.${p}`)}
                    <span className="release-phase-count">
                      {c.done}/{c.total}
                    </span>
                  </Button>
                );
              })}
            </div>
            <div
              className="release-filters"
              role="group"
              aria-label={t("releaseCheck.filterLabel")}
            >
              {["all", "remaining", "blocked"].map((f) => (
                <Button
                  key={f}
                  variant="ghost"
                  size="sm"
                  aria-pressed={filter === f}
                  onClick={() => setFilter(f)}
                >
                  {t(`releaseCheck.filter.${f}`)}
                  {f === "blocked" && phaseTotals.blocked ? (
                    <span>{phaseTotals.blocked}</span>
                  ) : null}
                </Button>
              ))}
            </div>
          </div>
          <div className="release-workspace">
            <div
              className="release-list"
              role="group"
              aria-label={t(`releaseCheck.phase.${phase}`)}
            >
              {visible.map((row) => {
                const e = release.entries[row.id] ?? emptyEntry();
                return (
                  <button
                    type="button"
                    className="release-row"
                    aria-pressed={item?.id === row.id}
                    key={row.id}
                    onClick={() => setSelected(row.id)}
                  >
                    <StatusMark status={e.status} />
                    <span className="release-row-copy">
                      <strong>{t(`releaseCheck.item.${row.id}`)}</strong>
                      <span>
                        {e.owner || t("releaseCheck.unassigned")}
                        <span className="release-row-dot">·</span>
                        {t(`releaseCheck.status.${e.status}`)}
                      </span>
                    </span>
                    <span
                      className={`release-priority ${row.priority.toLowerCase()}`}
                    >
                      {row.priority}
                    </span>
                  </button>
                );
              })}
              {!visible.length ? (
                <div className="release-list-empty">
                  <CircleCheck size={22} />
                  <p>
                    {t(
                      filter === "blocked"
                        ? "releaseCheck.noBlocked"
                        : "releaseCheck.noRemaining",
                    )}
                  </p>
                </div>
              ) : null}
            </div>
            {item ? (
              <aside
                className="release-detail"
                aria-label={t("releaseCheck.detail")}
              >
                <div className="release-detail-heading">
                  <Badge variant="outline">{item.priority}</Badge>
                  <span>{t(`releaseCheck.phase.${phase}`)}</span>
                </div>
                <h3>{t(`releaseCheck.item.${item.id}`)}</h3>
                <p className="release-instruction">
                  {t(`releaseCheck.guide.${item.id}`)}
                </p>
                <form onSubmit={saveEntry}>
                  <label className="release-field">
                    <span>{t("releaseCheck.result")}</span>
                    <Select
                      value={draft.status}
                      onValueChange={(status) =>
                        change({ status: status as CheckStatus })
                      }
                      items={statuses.map((status) => ({
                        value: status,
                        label: t(`releaseCheck.status.${status}`),
                      }))}
                    >
                      <SelectTrigger aria-label={t("releaseCheck.result")}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {statuses.map((status) => (
                          <SelectItem key={status} value={status}>
                            {t(`releaseCheck.status.${status}`)}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </label>
                  <label className="release-field">
                    <span>{t("releaseCheck.owner")}</span>
                    <Input
                      value={draft.owner}
                      maxLength={120}
                      placeholder={t("releaseCheck.ownerPlaceholder")}
                      onChange={(e) => change({ owner: e.target.value })}
                    />
                  </label>
                  <label className="release-field">
                    <span>
                      {t(
                        draft.status === "na"
                          ? "releaseCheck.reason"
                          : "releaseCheck.note",
                      )}
                    </span>
                    <Textarea
                      rows={3}
                      maxLength={12000}
                      value={draft.note}
                      placeholder={t("releaseCheck.notePlaceholder")}
                      onChange={(e) => change({ note: e.target.value })}
                    />
                  </label>
                  <label className="release-field">
                    <span>
                      <Link2 size={13} />
                      {t("releaseCheck.evidence")}
                    </span>
                    <Textarea
                      rows={2}
                      maxLength={2000}
                      value={draft.evidence}
                      placeholder={t("releaseCheck.evidencePlaceholder")}
                      onChange={(e) => change({ evidence: e.target.value })}
                    />
                  </label>
                  {entry.evidence
                    .split(/\s+/)
                    .filter((url) => /^https?:\/\//i.test(url))
                    .map((url, i) => (
                      <ExternalLink
                        key={`${url}-${i}`}
                        href={url}
                        className="release-evidence-link"
                      >
                        {t("releaseCheck.openEvidence", { count: i + 1 })}
                      </ExternalLink>
                    ))}
                  <div className="release-save">
                    <span role="status">
                      {dirty
                        ? t("releaseCheck.unsaved")
                        : entry.updatedAt
                          ? t("releaseCheck.savedAt", {
                              time: shortTime(entry.updatedAt),
                            })
                          : t("releaseCheck.notChecked")}
                    </span>
                    <Button type="submit" disabled={!dirty}>
                      <Check />
                      {t("releaseCheck.save")}
                    </Button>
                  </div>
                </form>
              </aside>
            ) : null}
          </div>
        </>
      ) : null}
      <Dialog open={creating} onOpenChange={setCreating}>
        <DialogContent className="release-create-dialog">
          <DialogTitle>{t("releaseCheck.new")}</DialogTitle>
          <DialogDescription>{t("releaseCheck.newHint")}</DialogDescription>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              const fields = new FormData(event.currentTarget);
              const name = String(fields.get("name") ?? "").trim();
              const baseline = String(fields.get("baseline") ?? "").trim();
              const branch = String(fields.get("branch") ?? "").trim();
              if (!name || !baseline || !branch) return;
              const id = crypto.randomUUID();
              if (
                commit((latest) => {
                  if (
                    latest.some(
                      (r) => r.name.toLowerCase() === name.toLowerCase(),
                    )
                  )
                    throw new Error("checklist_conflict");
                  return [
                    {
                      id,
                      name,
                      baseline,
                      branch,
                      target: String(fields.get("target") ?? ""),
                      entries: {},
                      createdAt: new Date().toISOString(),
                    },
                    ...latest,
                  ];
                })
              ) {
                setSelectedRelease(id);
                setCreating(false);
                setPhase("plan");
                setFilter("all");
              }
            }}
          >
            <label className="release-field">
              <span>{t("releaseCheck.version")}</span>
              <Input
                name="name"
                required
                maxLength={120}
                placeholder="OTA1 · Pre1"
                autoFocus
              />
            </label>
            <label className="release-field">
              <span>{t("releaseCheck.baseline")}</span>
              <Input
                name="baseline"
                required
                maxLength={500}
                placeholder={t("releaseCheck.baselinePlaceholder")}
              />
            </label>
            <label className="release-field">
              <span>{t("releaseCheck.branch")}</span>
              <Textarea
                rows={2}
                name="branch"
                required
                maxLength={500}
                defaultValue={branches.join("\n")}
                placeholder={t("releaseCheck.branchPlaceholder")}
              />
            </label>
            <label className="release-field">
              <span>{t("releaseCheck.target")}</span>
              <Input type="date" name="target" />
            </label>
            {error ? (
              <p className="release-message" role="alert">
                {error}
              </p>
            ) : null}
            <Button type="submit">
              {t("releaseCheck.create")}
              <ArrowRight />
            </Button>
          </form>
        </DialogContent>
      </Dialog>
    </section>
  );
}
