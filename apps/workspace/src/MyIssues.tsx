import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import { Num } from "@/components/motion";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { focus } from "./api";
import { openContextAction } from "./context-actions";
import type { Mine, MyIssue } from "./contracts/setup";
import { focusKey, queryClient } from "./focus-cache";
import { t } from "./i18n";
import { RefreshStatus } from "./RefreshStatus";
import {
  BANDS,
  bandCounts,
  bandOf,
  groupWork,
  LENSES,
  mergeWork,
  workColumns,
  type Band,
  type Lens,
  type Work,
} from "./my-issues";

const mineKey = [...focusKey, "mine"];
const SHOWN = 8;
const LENS_KEY = "tinnci.mine.lens";
const REFRESH_MS = 30 * 60_000;

/** The last read at once, then a fresh one; a source that fails keeps the other's items. */
function useMine() {
  const cached = useQuery(
    {
      queryKey: [...mineKey, "cached"],
      queryFn: async (): Promise<Mine | null> =>
        (await focus("mine_cached"))?.mine ?? null,
      staleTime: Infinity,
    },
    queryClient,
  );
  const fresh = useQuery(
    {
      queryKey: mineKey,
      queryFn: async (): Promise<Mine> => {
        const result = await focus("mine");
        if (!result.mine) throw { code: "spm_invalid_response" };
        return result.mine;
      },
      staleTime: 5 * 60_000,
      refetchInterval: REFRESH_MS,
    },
    queryClient,
  );
  return {
    mine: fresh.data ?? cached.data,
    fresh,
    error: fresh.error || (!fresh.isFetching && !fresh.data && cached.error),
  };
}

function savedLens(): Lens {
  try {
    const saved = localStorage.getItem(LENS_KEY) as Lens | null;
    return saved && LENSES.includes(saved) ? saved : "rank";
  } catch {
    return "rank";
  }
}

function saveLens(lens: Lens) {
  try {
    localStorage.setItem(LENS_KEY, lens);
  } catch {
    // The view still changes; it just starts from the list next time.
  }
}

function open(item: MyIssue, onModel: (code: string) => void) {
  const source = openContextAction(item.id, item.title, item.url);
  if (source) return void source.run();
  if (item.followed) onModel(item.model);
}

function reasonLabel(key: string) {
  return t(`mine.reason.${key}`);
}

function WorkRow({
  work,
  onModel,
}: {
  work: Work;
  onModel: (code: string) => void;
}) {
  const { lead, items } = work;
  return (
    <li>
      <button
        type="button"
        className="my-issue"
        onClick={() => open(lead, onModel)}
      >
        <span className={`my-issue-score band-${bandOf(lead.score)}`}>
          {lead.score}
        </span>
        <span className="my-issue-text">
          <strong>{lead.title}</strong>
          <span className="quiet">
            {[
              lead.display_id,
              lead.model,
              lead.status,
              items.length > 1 ? t("mine.copies", { count: items.length }) : "",
            ]
              .filter(Boolean)
              .join(" · ")}
          </span>
        </span>
        <span className="my-issue-reasons">
          {lead.reasons.slice(0, 2).map((r) => (
            <span key={r.key} className="related-term">
              {r.key === "severity" && lead.severity
                ? lead.severity
                : reasonLabel(r.key)}
            </span>
          ))}
        </span>
      </button>
    </li>
  );
}

/** Where a set of work sits by urgency; one band can be picked to narrow the list. */
function BandBar({
  counts,
  picked,
  onPick,
  share = 1,
}: {
  counts: Record<Band, number>;
  /** This set's size against the largest one beside it. */
  share?: number;
  picked?: Band | null;
  onPick?: (band: Band | null) => void;
}) {
  return (
    <span
      className="band-bar"
      style={{ width: `${share * 100}%` }}
      aria-hidden={onPick ? undefined : "true"}
    >
      {BANDS.filter((band) => counts[band]).map((band) =>
        onPick ? (
          <button
            key={band}
            type="button"
            className={`band-${band}`}
            style={{ flexGrow: counts[band] }}
            aria-pressed={picked === band}
            aria-label={`${t(`mine.band.${band}`)} ${counts[band]}`}
            title={`${t(`mine.band.${band}`)} ${counts[band]}`}
            onClick={() => onPick(picked === band ? null : band)}
          />
        ) : (
          <span
            key={band}
            className={`band-${band}`}
            style={{ flexGrow: counts[band] }}
          />
        ),
      )}
    </span>
  );
}

/** 我的问题: Jira and 飞书项目 work under the signed-in user, by urgency score. */
export function MyIssues({ onModel }: { onModel: (code: string) => void }) {
  useTranslation();
  const { mine, fresh, error } = useMine();
  const [all, setAll] = useState(false);
  const [lens, setLens] = useState(savedLens);
  const [band, setBand] = useState<Band | null>(null);
  const works = useMemo(() => mergeWork(mine?.items ?? []), [mine]);
  if (
    !works.length &&
    !error &&
    !fresh.isFetching &&
    !mine?.sources.some((s) => s.error)
  )
    return null;
  const counts = bandCounts(works);
  const visible = band
    ? works.filter((w) => bandOf(w.lead.score) === band)
    : works;
  const groups = groupWork(visible, lens);
  const largest = Math.max(1, ...groups.map((g) => g.works.length));
  const shown = all || band ? visible : visible.slice(0, SHOWN);
  return (
    <section className="my-issues" aria-label={t("mine.title")}>
      <header className="my-issues-head">
        <h2>{t("mine.title")}</h2>
        <strong className="my-issues-total">
          <Num value={works.length} digits={0} />
        </strong>
        <span className="my-issues-bands">
          {BANDS.map((b) => (
            <span key={b} className={band === b ? "is-picked" : undefined}>
              <i className={`band-${b}`} />
              {t(`mine.band.${b}`)} {counts[b]}
            </span>
          ))}
        </span>
        <ToggleGroup
          value={[lens]}
          onValueChange={(value: string[]) => {
            const next = value[0] as Lens | undefined;
            if (!next) return;
            setLens(next);
            saveLens(next);
          }}
          aria-label={t("mine.lens")}
        >
          {LENSES.map((l) => (
            <ToggleGroupItem key={l} value={l} size="sm">
              {t(`mine.lens.${l}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <RefreshStatus
          reading={fresh.isFetching}
          error={error}
          hasData={!!mine}
          sources={mine?.sources.map((s) => ({
            label: t(`mine.source.${s.source}`),
            error: s.error,
            updatedAt: !s.error ? mine.computed_at : undefined,
          }))}
          label={t("mine.refresh")}
          onRefresh={fresh.refetch}
        />
      </header>
      <BandBar counts={counts} picked={band} onPick={setBand} />
      {lens === "board" ? (
        <div className="work-board">
          {workColumns(visible).map(([column, list]) => (
            <section
              key={column}
              className="board-column"
              aria-label={t(`work.stage.${column}`)}
            >
              <header>
                <strong>{t(`work.stage.${column}`)}</strong>
                <span>{list.length}</span>
              </header>
              <ol>
                {list.map(({ lead }) => (
                  <li key={`${lead.source}:${lead.id}`}>
                    <button
                      type="button"
                      className="board-card"
                      onClick={() => open(lead, onModel)}
                    >
                      <span className="board-card-title">{lead.title}</span>
                      <span className="board-card-foot">
                        <span
                          className={`my-issue-score band-${bandOf(lead.score)}`}
                        >
                          {lead.score}
                        </span>
                        <span className="quiet">{lead.display_id}</span>
                      </span>
                    </button>
                  </li>
                ))}
              </ol>
            </section>
          ))}
        </div>
      ) : lens === "rank" ? (
        <>
          <ol>
            {shown.map((work) => (
              <WorkRow
                key={`${work.lead.source}:${work.lead.id}`}
                work={work}
                onModel={onModel}
              />
            ))}
          </ol>
          {!band && visible.length > SHOWN ? (
            <Button variant="ghost" size="sm" onClick={() => setAll(!all)}>
              {all ? t("mine.less") : t("mine.all", { count: visible.length })}
            </Button>
          ) : null}
        </>
      ) : (
        groups.map((group) => (
          <Collapsible key={group.key} className="my-group">
            <CollapsibleTrigger className="my-group-head">
              <ChevronDown aria-hidden="true" />
              <strong>
                {lens === "model" ? group.key : reasonLabel(group.key)}
              </strong>
              <span className="quiet">{group.works.length}</span>
              <span className="my-group-track">
                <BandBar
                  counts={bandCounts(group.works)}
                  share={group.works.length / largest}
                />
              </span>
              <span className={`my-issue-score band-${bandOf(group.top)}`}>
                {group.top}
              </span>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <ol>
                {group.works.map((work) => (
                  <WorkRow
                    key={`${work.lead.source}:${work.lead.id}`}
                    work={work}
                    onModel={onModel}
                  />
                ))}
              </ol>
            </CollapsibleContent>
          </Collapsible>
        ))
      )}
    </section>
  );
}
