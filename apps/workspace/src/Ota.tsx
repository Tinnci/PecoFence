import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ChevronRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { ContextTarget } from "./components/context-menu";
import { copyContextAction, openContextAction } from "./context-actions";
import type { ModelOta, OtaItem, OtaPlan, OtaReport } from "./contracts/setup";
import { focus, type ModelTab } from "./api";
import { otaKey, queryClient } from "./focus-cache";
import { t } from "./i18n";
import { RefreshStatus } from "./RefreshStatus";

export { otaKey };
export const otaOptions = {
  queryKey: otaKey,
  queryFn: async (): Promise<OtaReport | null> =>
    requireOta((await focus("ota")).ota),
  staleTime: 5 * 60_000,
};

function requireOta(report: OtaReport | undefined): OtaReport {
  if (!report || !Array.isArray(report.models) || !Array.isArray(report.errors))
    throw { code: "spm_invalid_response" };
  return report;
}

const HOUR = 60 * 60_000;

/** The kept answer at once; the sources are reread only where OTA work is being looked at. */
export function useOta() {
  const query = useQuery(otaOptions, queryClient);
  const read = useQuery(
    {
      queryKey: [...otaKey, "read"],
      queryFn: async () => {
        const data = await focus("ota_refresh");
        const report = requireOta(data.ota);
        await queryClient.cancelQueries({ queryKey: otaKey, exact: true });
        queryClient.setQueryData(otaKey, report);
        return report;
      },
      staleTime: HOUR,
      refetchInterval: HOUR,
    },
    queryClient,
  );
  return {
    report: query.data,
    reading: read.isFetching,
    error: read.error || (!read.isFetching && query.error),
    reread: read.refetch,
  };
}

function OtaRefresh({
  state,
  code,
}: {
  state: ReturnType<typeof useOta>;
  code?: string;
}) {
  const sources = state.report?.sources?.filter(
    (s) => !code || s.code === code,
  );
  const issues = state.report?.sources
    ? []
    : state.report?.errors.filter((e) => !code || e.startsWith(`${code} `));
  return (
    <RefreshStatus
      reading={state.reading}
      error={state.error}
      hasData={sources ? sources.some((s) => !!s.updated_at) : !!state.report}
      sources={sources?.map((s) => ({
        label: [s.code, t(`ota.kind.${s.kind}`), s.label]
          .filter(Boolean)
          .join(" · "),
        updatedAt: s.updated_at,
        stale: s.stale,
        error: s.error,
      }))}
      issues={issues}
      label={t("ota.reread")}
      scope={t("refresh.workItems")}
      onRefresh={state.reread}
    />
  );
}

const shipping = (plan: OtaPlan) => plan.official || plan.rollout || plan.pre1;
/** How long an OTA that left items behind stays in view after it ships. */
const MISSED_DAYS = 60;
const daysSince = (day: string) => (Date.now() - Date.parse(day)) / 86_400_000;

/** Worth looking at now: still ahead, unplanned and unfinished, or just shipped without something. */
const live = (plan: OtaPlan) =>
  plan.state === "upcoming" ||
  (plan.state === "undated" && plan.verified < plan.total) ||
  (plan.late > 0 && daysSince(shipping(plan)) <= MISSED_DAYS);

/** The OTA a model is working towards. */
export function nextPlan(ota: ModelOta | undefined) {
  return ota?.plans.find((p) => p.state === "upcoming");
}

const STAGES = ["tagged", "fixed", "merged", "verified"];
function Stage({ stage }: { stage: string }) {
  const reached = STAGES.indexOf(stage);
  const name = t(`ota.stage.${stage}`);
  return (
    <span className="ota-stage" role="img" aria-label={name} title={name}>
      {STAGES.map((step, at) => (
        <i key={step} className={at <= reached ? "is-on" : undefined} />
      ))}
    </span>
  );
}

function Dates({ plan }: { plan: OtaPlan }) {
  const parts = [
    ["ota.pre1", plan.pre1],
    ["ota.official", plan.official],
    ["ota.rollout", plan.rollout],
  ].filter(([, day]) => day);
  return (
    <span className="ota-dates">
      {parts.length
        ? parts.map(([key, day]) => (
            <span key={key}>
              {t(key!)} {day!.slice(5)}
            </span>
          ))
        : t("ota.notPlanned")}
    </span>
  );
}

function Meter({ plan }: { plan: OtaPlan }) {
  return (
    <span className="ota-meter" title={t("ota.mergedOf")}>
      <Progress
        value={plan.total ? (plan.merged / plan.total) * 100 : 0}
        aria-label={t("ota.mergedOf")}
      />
      <span>
        {plan.merged}/{plan.total}
      </span>
    </span>
  );
}

function Flags({ plan }: { plan: OtaPlan }) {
  return (
    <>
      {plan.late ? (
        <Badge variant="destructive">
          {t("ota.lateCount", { count: plan.late })}
        </Badge>
      ) : null}
      {plan.must_open ? (
        <Badge variant="outline">
          {t("ota.mustCount", { count: plan.must_open })}
        </Badge>
      ) : null}
    </>
  );
}

async function assign(code: string, item: OtaItem, version: string) {
  await focus("annotate", {
    source: item.source,
    item: item.id,
    instance: item.instance,
    code,
    op: { op: "tags", tags: [...item.tags, version] },
  });
  await queryClient.invalidateQueries({ queryKey: otaKey, exact: true });
}

function Row({
  item,
  code,
  versions = [],
}: {
  item: OtaItem;
  code: string;
  versions?: string[];
}) {
  const [failed, setFailed] = useState(false);
  const source = openContextAction("open", t("ota.openItem"), item.url);
  return (
    <ContextTarget
      actions={[
        ...(source ? [source] : []),
        copyContextAction("copy", t("ota.copyId"), item.display_id),
      ]}
    >
      <li className={`ota-row${item.late ? " is-late" : ""}`}>
        <Stage stage={item.stage} />
        <a href={item.url} target="_blank" rel="noreferrer">
          {item.display_id}
        </a>
        <span
          className="ota-title"
          title={item.reason || item.labels.join(" ")}
        >
          {item.title}
        </span>
        <span className="quiet">
          {t(`ota.kind.${item.kind}`)} · {item.owner || item.status}
        </span>
        <span className="ota-flags">
          {item.must ? <Badge variant="outline">{t("ota.must")}</Badge> : null}
          {item.late ? (
            <Badge variant="destructive">{t("ota.late")}</Badge>
          ) : null}
          {versions.map((version) => (
            <Button
              key={version}
              type="button"
              variant="outline"
              size="xs"
              aria-label={t("ota.assign", {
                item: item.display_id,
                version,
              })}
              onClick={() => {
                setFailed(false);
                assign(code, item, version).catch(() => setFailed(true));
              }}
            >
              {version}
            </Button>
          ))}
          {failed ? (
            <span className="short" role="alert">
              {t("platform.saveFailed")}
            </span>
          ) : null}
        </span>
      </li>
    </ContextTarget>
  );
}

function Fold({
  title,
  count,
  start = false,
  children,
}: {
  title: string;
  count: number;
  start?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(start);
  return (
    <section className="ota-fold">
      <button
        type="button"
        className="ota-fold-head"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <ChevronRight aria-hidden="true" />
        {title}
        <span className="quiet">{count}</span>
      </button>
      {open ? children : null}
    </section>
  );
}

function Card({
  plan,
  code,
  start,
}: {
  plan: OtaPlan;
  code: string;
  start: boolean;
}) {
  const [open, setOpen] = useState(start);
  return (
    <article className="ota-card">
      <button
        type="button"
        className="ota-head"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <ChevronRight aria-hidden="true" />
        <strong>{plan.version}</strong>
        <Dates plan={plan} />
        <Meter plan={plan} />
        <Flags plan={plan} />
      </button>
      {open ? (
        plan.items.length ? (
          <ul className="ota-items">
            {plan.items.map((item) => (
              <Row key={`${item.source}:${item.id}`} item={item} code={code} />
            ))}
          </ul>
        ) : (
          <p className="quiet ota-empty">{t("ota.nothingYet")}</p>
        )
      ) : null}
    </article>
  );
}

function Waiting({ ota, plans }: { ota: ModelOta; plans?: OtaPlan[] }) {
  const versions = (plans ?? ota.plans)
    .filter((p) => p.state === "upcoming")
    .map((p) => p.version);
  return (
    <>
      {ota.undecided.length ? (
        <Fold
          title={t("ota.undecided")}
          count={ota.undecided.length}
          start={ota.undecided.length <= 10}
        >
          <ul className="ota-items">
            {ota.undecided.map((item) => (
              <Row
                key={`${item.source}:${item.id}`}
                item={item}
                code={ota.code}
                versions={item.stage === "verified" ? [] : versions}
              />
            ))}
          </ul>
        </Fold>
      ) : null}
      {ota.left_out.length ? (
        <Fold title={t("ota.leftOut")} count={ota.left_out.length}>
          <ul className="ota-items">
            {ota.left_out.map((item) => (
              <Row
                key={`${item.source}:${item.id}`}
                item={item}
                code={ota.code}
                versions={versions}
              />
            ))}
          </ul>
        </Fold>
      ) : null}
    </>
  );
}

/** One model's OTAs: what is coming open, what shipped folded away. */
export function ModelOtaTab({ code }: { code: string }) {
  useTranslation();
  const state = useOta();
  const ota = state.report?.models.find((m) => m.code === code);
  const current = ota?.plans.filter(live) ?? [];
  const past = ota?.plans.filter((p) => !live(p)).reverse() ?? [];
  return (
    <div className="ota-model">
      <OtaRefresh state={state} code={code} />
      {current.map((plan, at) => (
        <Card key={plan.version} plan={plan} code={code} start={at === 0} />
      ))}
      {ota ? <Waiting ota={ota} /> : null}
      {past.length ? (
        <Fold title={t("ota.shipped")} count={past.length}>
          {past.map((plan) => (
            <Card key={plan.version} plan={plan} code={code} start={false} />
          ))}
        </Fold>
      ) : null}
      {ota &&
      !state.reading &&
      !state.error &&
      !state.report?.sources?.some(
        (s) => s.code === code && (s.stale || s.error),
      ) &&
      !state.report?.errors.length &&
      !ota.plans.length &&
      !ota.undecided.length &&
      !ota.left_out.length ? (
        <p className="quiet">{t("ota.nothingYet")}</p>
      ) : null}
    </div>
  );
}

/** What an OTA-phase overview opens with: the coming OTA and everything in it. */
export function OtaNext({ code }: { code: string }) {
  useTranslation();
  const report = useQuery(otaOptions, queryClient).data;
  const plan = nextPlan(report?.models.find((m) => m.code === code));
  return plan ? (
    <div className="ota-next">
      <Card plan={plan} code={code} start />
    </div>
  ) : null;
}

/** Every model's OTAs, soonest first: the week's view across the portfolio. */
export function OtaPage({
  models,
  onModel,
}: {
  models: { code: string; name: string }[];
  onModel: (code: string, tab?: ModelTab) => void;
}) {
  useTranslation();
  const state = useOta();
  const { report } = state;
  const name = (code: string) =>
    models.find((m) => m.code === code)?.name ?? code;
  const rows = (report?.models ?? [])
    .flatMap((ota) =>
      ota.plans.filter(live).map((plan) => ({ code: ota.code, plan })),
    )
    .sort(
      (a, b) =>
        Number(!shipping(a.plan)) - Number(!shipping(b.plan)) ||
        shipping(a.plan).localeCompare(shipping(b.plan)),
    );
  const waiting = (report?.models ?? []).filter(
    (m) => m.undecided.length || m.left_out.length,
  );
  return (
    <div className="ota-page">
      <div className="ota-title-row">
        <h1>{t("nav.ota")}</h1>
        <OtaRefresh state={state} />
      </div>
      {rows.length ? (
        <ul className="ota-list">
          {rows.map(({ code, plan }) => (
            <li key={`${code}:${plan.version}`}>
              <button
                type="button"
                className="ota-head"
                onClick={() => onModel(code, "ota")}
              >
                <strong>{name(code)}</strong>
                <span>{plan.version}</span>
                <Dates plan={plan} />
                <Meter plan={plan} />
                <Flags plan={plan} />
              </button>
            </li>
          ))}
        </ul>
      ) : report &&
        !state.reading &&
        !state.error &&
        !report.errors.length &&
        !report.sources?.some((s) => s.stale || s.error) ? (
        <p className="quiet">{t("ota.nothingYet")}</p>
      ) : null}
      {waiting.map((ota) => (
        <section key={ota.code} className="ota-waiting">
          <h2>{name(ota.code)}</h2>
          <Waiting ota={ota} />
        </section>
      ))}
    </div>
  );
}
