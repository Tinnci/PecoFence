import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ChevronLeft, ChevronRight, Info } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Toggle } from "@/components/ui/toggle";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { ContextTarget, type ContextAction } from "./components/context-menu";
import { focus, calendarSettings, type ModelTab } from "./api";
import { otaOptions } from "./Ota";
import { copyContextAction, openContextAction } from "./context-actions";
import {
  dayInfo,
  workingDays,
  isoDay,
  monthWeeks,
  shiftMonth,
  spanDays,
  visibleEvents,
  weekBars,
  type CalendarView,
} from "./calendar";
import type {
  CalEvent,
  CalOrigin,
  Calendar,
  OtaPlan,
  RegionalCalendar,
} from "./contracts/setup";
import { queryClient, calendarKey, profilesKey } from "./focus-cache";
import { locale, t } from "./i18n";
import { RefreshStatus } from "./RefreshStatus";

const REFRESH_MS = 30 * 60_000;
/** Lanes a week shows before the rest fold behind a count. */
const LANES = 5;

function useCalendar() {
  const cached = useQuery(
    {
      queryKey: [...calendarKey, "cached"],
      queryFn: async (): Promise<Calendar | null> =>
        (await focus("calendar_cached"))?.calendar ?? null,
      staleTime: Infinity,
    },
    queryClient,
  );
  const fresh = useQuery(
    {
      queryKey: calendarKey,
      queryFn: async (): Promise<Calendar> => {
        const result = await focus("calendar").finally(() =>
          queryClient.invalidateQueries({ queryKey: profilesKey }),
        );
        if (!result.calendar) throw { code: "spm_invalid_response" };
        return result.calendar;
      },
      staleTime: 5 * 60_000,
      refetchInterval: REFRESH_MS,
    },
    queryClient,
  );
  return {
    calendar: fresh.data ?? cached.data,
    fresh,
    error: fresh.error || (!fresh.isFetching && !fresh.data && cached.error),
  };
}

const stageName = (e: CalEvent) => t(`calendar.stage.${e.key}`);
const barLabel = (e: CalEvent) =>
  e.key === "release" || e.key === "seal"
    ? `${e.version || e.project} ${stageName(e)}`
    : `${e.project} ${stageName(e)}`;

/** Where a value lives, in a few words: 飞书表格 I12 · Pre1-OTA 版本释放时间. */
export function originPlace(origin: CalOrigin) {
  return [
    t(`calendar.document.${origin.document}`),
    origin.place,
    origin.document === "local"
      ? t(`calendar.stage.${origin.field}`)
      : origin.field,
  ]
    .filter(Boolean)
    .join(" · ");
}

const dates = (e: CalEvent) =>
  e.start === e.end ? e.start : `${e.start} – ${e.end}`;

function eventActions(
  event: CalEvent,
  onDetail: () => void,
  onModel: (code: string, tab?: ModelTab) => void,
  carried?: OtaPlan,
): ContextAction[] {
  const sources = event.origins.map((origin, at): ContextAction => {
    const label = `${t(`calendar.role.${origin.role}`)}：${originPlace(origin)}`;
    const id = `origin-${at}`;
    return (
      openContextAction(id, label, origin.url) ?? {
        id,
        label,
        disabled: true,
        run: () => {},
      }
    );
  });
  return [
    { id: "detail", label: t("calendar.detail"), icon: Info, run: onDetail },
    ...sources.map((action, at) => ({ ...action, separator: at === 0 })),
    ...(carried
      ? [
          {
            id: "ota",
            label: t("calendar.openOta", { version: carried.version }),
            separator: true,
            run: () => onModel(event.model, "ota"),
          },
        ]
      : []),
    ...(event.model
      ? [
          {
            id: "model",
            label: t("calendar.openModel"),
            separator: true,
            run: () => onModel(event.model),
          },
        ]
      : []),
    copyContextAction("copy-dates", t("calendar.copyDates"), dates(event)),
  ];
}

function duration(event: CalEvent, calendar?: RegionalCalendar | null) {
  const work = workingDays(event, calendar);
  return `${t("calendar.days", { count: spanDays(event) })} · ${work === undefined ? t("calendar.coverageMissing") : t("calendar.workdays", { count: work })}`;
}
function CalendarRegion({ calendar }: { calendar?: RegionalCalendar | null }) {
  const [country, setCountry] = useState(calendar?.country ?? "CN");
  const [subdivision, setSubdivision] = useState(calendar?.subdivision ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const save = async () => {
    setBusy(true);
    setError("");
    try {
      const work_calendar = await calendarSettings(
        country,
        subdivision.trim() || undefined,
      );
      for (const key of [calendarKey, [...calendarKey, "cached"]])
        queryClient.setQueryData<Calendar | null>(key, (old) =>
          old ? { ...old, work_calendar } : old,
        );
    } catch {
      setError(t("calendar.regionError"));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="cal-region">
      <select
        aria-label={t("calendar.region")}
        value={country}
        onChange={(e) => {
          setCountry(e.target.value);
          setSubdivision("");
        }}
      >
        {["CN", "IN", "BD", "BR"].map((c) => (
          <option key={c} value={c}>
            {t(`calendar.country.${c}`)}
          </option>
        ))}
      </select>
      {country === "IN" || country === "BR" ? (
        <Input
          aria-label={t("calendar.subdivision")}
          placeholder={t("calendar.subdivision")}
          value={subdivision}
          onChange={(e) => setSubdivision(e.target.value.toUpperCase())}
          maxLength={24}
        />
      ) : null}
      <Button
        size="xs"
        variant="outline"
        disabled={busy}
        onClick={() => void save()}
      >
        {t("calendar.apply")}
      </Button>
      {error ? <span role="alert">{error}</span> : null}
    </div>
  );
}

function Detail({
  event,
  onModel,
  regional,
}: {
  regional?: RegionalCalendar | null;
  event: CalEvent;
  onModel: (code: string, tab?: ModelTab) => void;
}) {
  const facts = [
    dates(event),
    event.milestone ? t("calendar.node") : duration(event, regional),
    event.milestone &&
    dayInfo(event.start, regional).covered &&
    !dayInfo(event.start, regional).working
      ? t("calendar.restConflict")
      : "",
    event.version,
    event.status,
    event.customer,
    event.owner,
  ].filter(Boolean);
  return (
    <section className="cal-detail" aria-label={barLabel(event)}>
      <header>
        <strong>{barLabel(event)}</strong>
        {event.model ? (
          <Button
            variant="outline"
            size="xs"
            onClick={() => onModel(event.model)}
          >
            {t("calendar.openModel")}
          </Button>
        ) : null}
      </header>
      <p className="quiet">{facts.join(" · ")}</p>
      {event.note ? <p className="cal-note">{event.note}</p> : null}
      <dl className="cal-origins">
        <dt>{t("calendar.origin")}</dt>
        {event.origins.map((origin) => {
          const open = openContextAction(
            origin.place,
            originPlace(origin),
            origin.url,
          );
          return (
            <dd key={`${origin.role}:${origin.place}:${origin.field}`}>
              <span className="quiet">{t(`calendar.role.${origin.role}`)}</span>
              {open ? (
                <button type="button" onClick={() => void open.run()}>
                  {originPlace(origin)}
                </button>
              ) : (
                <span>{originPlace(origin)}</span>
              )}
              <span className="quiet">{origin.value}</span>
            </dd>
          );
        })}
        {event.origins.length > 1 ? (
          <dd className="quiet cal-rule">
            {t(`calendar.rule.${event.stage}`, { defaultValue: "" })}
          </dd>
        ) : null}
      </dl>
    </section>
  );
}

/** A month of OTA stages and project dates, one bar per stage per week. */
export function CalendarPage({
  onModel,
}: {
  onModel: (code: string, tab?: ModelTab) => void;
}) {
  useTranslation();
  const { calendar, fresh, error } = useCalendar();
  const regional = calendar?.work_calendar;
  const ota = useQuery(otaOptions, queryClient).data;
  const planOf = (event: CalEvent) =>
    ota?.models
      .find((m) => m.code === event.model)
      ?.plans.find((p) => p.version === event.version.trim().toUpperCase());
  const progress = (event: CalEvent) => {
    const plan = planOf(event);
    return plan?.total && ["pre1", "official"].includes(event.key)
      ? ` · ${plan.merged}/${plan.total}`
      : "";
  };
  const today = isoDay(new Date());
  const [shown, setShown] = useState(() => {
    const now = new Date();
    return { year: now.getFullYear(), month: now.getMonth() };
  });
  const [view, setView] = useState<CalendarView>({
    scope: "followed",
    delayed: false,
    words: "",
  });
  const [picked, setPicked] = useState<CalEvent | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const events = useMemo(
    () => visibleEvents(calendar?.events ?? [], view),
    [calendar, view],
  );
  const weeks = monthWeeks(shown.year, shown.month);
  const title = new Date(shown.year, shown.month, 1).toLocaleDateString(
    locale(),
    { year: "numeric", month: "long" },
  );
  const weekdays = weeks[0].map((day) =>
    new Date(`${day}T00:00:00`).toLocaleDateString(locale(), {
      weekday: "short",
    }),
  );
  return (
    <section className="calendar" aria-label={t("nav.calendar")}>
      <header className="cal-toolbar">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("calendar.previous")}
          onClick={() => setShown(shiftMonth(shown.year, shown.month, -1))}
        >
          <ChevronLeft aria-hidden="true" />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("calendar.next")}
          onClick={() => setShown(shiftMonth(shown.year, shown.month, 1))}
        >
          <ChevronRight aria-hidden="true" />
        </Button>
        <h1>{title}</h1>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            const now = new Date();
            setShown({ year: now.getFullYear(), month: now.getMonth() });
          }}
        >
          {t("calendar.today")}
        </Button>
        <span className="cal-gap" />
        <ToggleGroup
          variant="outline"
          size="sm"
          className="chips"
          aria-label={t("calendar.scope")}
          value={[view.scope]}
          onValueChange={(next: string[]) => {
            if (next[0])
              setView({ ...view, scope: next[0] as CalendarView["scope"] });
          }}
        >
          <ToggleGroupItem value="followed">
            {t("calendar.followed")}
          </ToggleGroupItem>
          <ToggleGroupItem value="all">{t("calendar.all")}</ToggleGroupItem>
        </ToggleGroup>
        <Toggle
          variant="outline"
          size="sm"
          pressed={view.delayed}
          onPressedChange={(delayed) => setView({ ...view, delayed })}
        >
          {t("calendar.delayed")}
        </Toggle>
        <Input
          className="logs-search"
          value={view.words}
          maxLength={100}
          placeholder={t("calendar.search")}
          aria-label={t("calendar.search")}
          onChange={(e) => setView({ ...view, words: e.target.value })}
        />
        <RefreshStatus
          reading={fresh.isFetching}
          error={error}
          hasData={!!calendar}
          sources={calendar?.sources.map((s) => ({
            label: t(`calendar.source.${s.source}`),
            error: s.error,
            updatedAt: !s.error ? calendar.computed_at : undefined,
          }))}
          label={t("calendar.refresh")}
          onRefresh={fresh.refetch}
        />
      </header>
      {calendar && !calendar.configured ? (
        <p className="quiet">{t("calendar.notConfigured")}</p>
      ) : null}
      <CalendarRegion
        key={`${regional?.country}:${regional?.subdivision}`}
        calendar={regional}
      />
      <p className="quiet cal-calendar-source">
        {regional?.country}
        {regional?.subdivision ? ` · ${regional.subdivision}` : ""}{" "}
        {regional?.timezone} ·{" "}
        {(() => {
          const source = regional?.years.find((y) => y.year === shown.year);
          return source ? (
            <a
              href={source.source_url}
              target="_blank"
              rel="noreferrer"
              title={`${source.source} · ${source.revision}`}
            >
              {t(`calendar.status.${source.status}`)} · {source.year}
            </a>
          ) : (
            t("calendar.coverageMissing")
          );
        })()}
        {regional?.errors.length ? ` · ${regional.errors.join(" · ")}` : ""}
      </p>
      <div className="cal-head" aria-hidden="true">
        {weekdays.map((name) => (
          <span key={name}>{name}</span>
        ))}
      </div>
      <div className="cal-month">
        {weeks.map((week) => {
          const bars = weekBars(week, events);
          const wide = open === week[0];
          const shownBars = wide ? bars : bars.filter((b) => b.lane < LANES);
          const hidden = bars.length - shownBars.length;
          const lanes = Math.max(0, ...shownBars.map((b) => b.lane + 1));
          return (
            <div
              className="cal-week"
              key={week[0]}
              style={{ gridTemplateRows: `26px repeat(${lanes}, 24px) 1fr` }}
            >
              {week.map((day, at) => {
                const date = new Date(`${day}T00:00:00`);
                const off = date.getMonth() !== shown.month;
                const info = dayInfo(day, regional);
                return (
                  <div
                    key={day}
                    title={info.name}
                    className={`cal-day${!info.working ? " is-weekend" : ""}${off ? " is-off" : ""}`}
                    style={{ gridColumn: at + 1 }}
                  >
                    <span
                      className={day === today ? "cal-num is-today" : "cal-num"}
                    >
                      {date.getDate()}
                    </span>
                    {info.exception ? (
                      <span className="cal-day-badge">
                        {t(info.working ? "calendar.work" : "calendar.rest")}
                      </span>
                    ) : null}
                    <span className="cal-holiday">{info.name}</span>
                  </div>
                );
              })}
              {shownBars.map((bar) => {
                const work = workingDays(bar.event, regional);
                const conflict =
                  bar.event.milestone &&
                  dayInfo(bar.event.start, regional).covered &&
                  !dayInfo(bar.event.start, regional).working;
                const label = barLabel(bar.event);
                const shape = bar.event.milestone
                  ? "is-node"
                  : `is-span${bar.before ? " from-before" : ""}${bar.after ? " to-after" : ""}`;
                return (
                  <ContextTarget
                    key={bar.event.id}
                    title={label}
                    actions={eventActions(
                      bar.event,
                      () => setPicked(bar.event),
                      onModel,
                      planOf(bar.event),
                    )}
                  >
                    <button
                      type="button"
                      className={`cal-bar ${shape} cal-${bar.event.stage}${bar.event.status === "已完成" ? " is-done" : ""}${picked?.id === bar.event.id ? " is-picked" : ""}`}
                      title={
                        bar.event.milestone
                          ? `${label} · ${bar.event.start}${conflict ? ` · ${t("calendar.restConflict")}` : ""}`
                          : `${label} · ${dates(bar.event)} · ${duration(bar.event, regional)}`
                      }
                      style={{
                        gridColumn: `${bar.column + 1} / span ${bar.span}`,
                        gridRow: bar.lane + 2,
                      }}
                      onClick={() =>
                        setPicked(
                          picked?.id === bar.event.id ? null : bar.event,
                        )
                      }
                    >
                      {conflict ? "⚠ " : ""}
                      {label}
                      {progress(bar.event)}
                      {bar.event.milestone || bar.before
                        ? null
                        : ` · ${work === undefined ? t("calendar.days", { count: spanDays(bar.event) }) : t("calendar.workdays", { count: work })}`}
                    </button>
                  </ContextTarget>
                );
              })}
              {hidden > 0 || wide ? (
                <button
                  type="button"
                  className="cal-more"
                  style={{ gridRow: lanes + 2 }}
                  onClick={() => setOpen(wide ? null : week[0])}
                >
                  {wide
                    ? t("calendar.less")
                    : t("calendar.more", { count: hidden })}
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
      <p className="cal-legend" aria-hidden="true">
        <span>
          <i className="cal-legend-node" />
          {t("calendar.node")}
        </span>
        <span>
          <i className="cal-legend-span" />
          {t("calendar.span")}
        </span>
      </p>
      {picked ? (
        <Detail event={picked} onModel={onModel} regional={regional} />
      ) : null}
    </section>
  );
}
