import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ChevronLeft, ChevronRight, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Toggle } from "@/components/ui/toggle";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { focus } from "./api";
import {
  isoDay,
  monthWeeks,
  shiftMonth,
  visibleEvents,
  weekBars,
  type CalendarView,
} from "./calendar";
import type { CalEvent, Calendar } from "./contracts/setup";
import { focusKey, queryClient } from "./focus-cache";
import { locale, t } from "./i18n";

const calendarKey = [...focusKey, "calendar"];
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
      queryFn: async (): Promise<Calendar | null> =>
        (await focus("calendar"))?.calendar ?? null,
      staleTime: 5 * 60_000,
      refetchInterval: REFRESH_MS,
    },
    queryClient,
  );
  return { calendar: fresh.data ?? cached.data, fresh };
}

const stageName = (e: CalEvent) => t(`calendar.stage.${e.key}`);
const barLabel = (e: CalEvent) =>
  e.key === "release" || e.key === "seal"
    ? `${e.version || e.project} ${stageName(e)}`
    : `${e.project} ${stageName(e)}`;

function Detail({
  event,
  onModel,
}: {
  event: CalEvent;
  onModel: (code: string) => void;
}) {
  const facts = [
    event.start === event.end ? event.start : `${event.start} – ${event.end}`,
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
    </section>
  );
}

/** A month of OTA stages and project dates, one bar per stage per week. */
export function CalendarPage({ onModel }: { onModel: (code: string) => void }) {
  useTranslation();
  const { calendar, fresh } = useCalendar();
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
  const failed = calendar?.sources.filter((s) => s.error) ?? [];
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
        <Button
          variant="ghost"
          size="icon-xs"
          disabled={fresh.isFetching}
          aria-label={t("calendar.refresh")}
          title={t("calendar.refresh")}
          onClick={() => void fresh.refetch()}
        >
          {fresh.isFetching ? (
            <Spinner aria-hidden="true" />
          ) : (
            <RefreshCw aria-hidden="true" />
          )}
        </Button>
      </header>
      {failed.map((s) => (
        <p key={s.source} className="short" role="alert">
          {t("calendar.failed", { source: t(`calendar.source.${s.source}`) })}
        </p>
      ))}
      {calendar && !calendar.configured ? (
        <p className="quiet">{t("calendar.notConfigured")}</p>
      ) : null}
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
                return (
                  <div
                    key={day}
                    className={`cal-day${at > 4 ? " is-weekend" : ""}${off ? " is-off" : ""}`}
                    style={{ gridColumn: at + 1 }}
                  >
                    <span
                      className={day === today ? "cal-num is-today" : "cal-num"}
                    >
                      {date.getDate()}
                    </span>
                  </div>
                );
              })}
              {shownBars.map((bar) => (
                <button
                  type="button"
                  key={bar.event.id}
                  className={`cal-bar cal-${bar.event.stage}${bar.event.status === "已完成" ? " is-done" : ""}${picked?.id === bar.event.id ? " is-picked" : ""}`}
                  title={barLabel(bar.event)}
                  style={{
                    gridColumn: `${bar.column + 1} / span ${bar.span}`,
                    gridRow: bar.lane + 2,
                  }}
                  onClick={() =>
                    setPicked(picked?.id === bar.event.id ? null : bar.event)
                  }
                >
                  {barLabel(bar.event)}
                </button>
              ))}
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
      {picked ? <Detail event={picked} onModel={onModel} /> : null}
    </section>
  );
}
