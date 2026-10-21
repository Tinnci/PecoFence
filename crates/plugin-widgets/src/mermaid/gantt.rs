//! Gantt charts: `title`, `dateFormat YYYY-MM-DD`, sections and tasks with dates,
//! `after` dependencies, durations, milestones and `excludes` / `weekend` calendars.
//! Drawn row by row, no layout pass.

use super::{
    Diagnostic, Diagram, ErrorKind, MAX_LABEL_CHARS, MAX_TASKS, Severity, Source, decode_entities,
    diagnostic, error,
};
use chrono::{Datelike, NaiveDate, Weekday};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Task {
    pub(crate) name: String,
    pub(crate) section: Option<String>,
    pub(crate) start: NaiveDate,
    /// Exclusive end; equal to `start` for milestones.
    pub(crate) end: NaiveDate,
    pub(crate) done: bool,
    pub(crate) active: bool,
    pub(crate) critical: bool,
    pub(crate) milestone: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Gantt {
    pub(crate) title: Option<String>,
    pub(crate) tasks: Vec<Task>,
}

impl Gantt {
    pub(crate) fn span(&self) -> Option<(NaiveDate, NaiveDate)> {
        let start = self.tasks.iter().map(|t| t.start).min()?;
        let end = self.tasks.iter().map(|t| t.end.max(t.start)).max()?;
        Some((start, end.max(start.succ_opt().unwrap_or(start))))
    }
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

fn parse_duration(text: &str) -> Option<i64> {
    let text = text.trim();
    let (number, multiplier) = if let Some(number) = text.strip_suffix('d') {
        (number, 1.0)
    } else {
        (text.strip_suffix('w')?, 7.0)
    };
    let number: f64 = number.trim().parse().ok()?;
    let days = number * multiplier;
    (days.is_finite() && (0.0..=3650.0).contains(&days)).then(|| days.ceil() as i64)
}

/// Days `excludes` takes out of durations: weekends (Saturday and Sunday, or Friday
/// and Saturday after `weekend friday`), named weekdays and single dates.
struct Calendar {
    weekend_from: Weekday,
    weekends: bool,
    /// Bit `n` closes the weekday `n` days from Monday.
    weekdays: u8,
    dates: Vec<NaiveDate>,
}

impl Calendar {
    fn closes(&self, weekday: Weekday) -> bool {
        (self.weekends && (weekday == self.weekend_from || weekday == self.weekend_from.succ()))
            || self.weekdays & (1 << weekday.num_days_from_monday()) != 0
    }

    fn closed(&self, day: NaiveDate) -> bool {
        self.closes(day.weekday()) || self.dates.contains(&day)
    }

    /// The first open day on or after `day`.
    fn open_from(&self, mut day: NaiveDate) -> Option<NaiveDate> {
        while self.closed(day) {
            day = day.succ_opt()?;
        }
        Some(day)
    }

    /// Exclusive end of `days` open days from `start`, as Mermaid stretches a duration
    /// over excluded days.
    fn finish(&self, start: NaiveDate, days: i64) -> Option<NaiveDate> {
        let (mut day, mut left) = (start, days);
        while left > 0 {
            left -= i64::from(!self.closed(day));
            day = day.succ_opt()?;
        }
        Some(day)
    }

    /// Read the scheduling directives first: Mermaid applies them to every task,
    /// wherever they appear.
    fn read(source: &Source, warnings: &mut Vec<Diagnostic>) -> Self {
        let mut calendar = Calendar {
            weekend_from: Weekday::Sat,
            weekends: false,
            weekdays: 0,
            dates: Vec::new(),
        };
        for &(number, line) in &source.lines[1..] {
            let mut warn = |what: &str| {
                let kind = ErrorKind::BadExclude(what.into());
                warnings.push(diagnostic(number, kind, line, Severity::Warning));
            };
            match line.split_once(char::is_whitespace) {
                Some(("excludes", value)) => {
                    for token in value.split([' ', '\t', ',']).filter(|t| !t.is_empty()) {
                        // chrono reads weekday names case-insensitively.
                        if token.eq_ignore_ascii_case("weekends") {
                            calendar.weekends = true;
                        } else if let Ok(day) = token.parse::<Weekday>() {
                            calendar.weekdays |= 1 << day.num_days_from_monday();
                        } else if let Some(date) = parse_date(token) {
                            calendar.dates.push(date);
                        } else {
                            warn(token);
                        }
                    }
                }
                Some(("weekend", value)) => match value.trim() {
                    "friday" => calendar.weekend_from = Weekday::Fri,
                    "saturday" => calendar.weekend_from = Weekday::Sat,
                    other => warn(other),
                },
                _ => {}
            }
        }
        if (0..7).all(|n| calendar.closes(Weekday::try_from(n).unwrap_or(Weekday::Mon))) {
            warnings.push(diagnostic(
                0,
                ErrorKind::ExcludesEverything,
                "",
                Severity::Warning,
            ));
            calendar.weekends = false;
            calendar.weekdays = 0;
        }
        calendar
    }
}

pub(super) fn parse(
    source: &Source,
    warnings: &mut Vec<Diagnostic>,
) -> Result<Diagram, Diagnostic> {
    let calendar = Calendar::read(source, warnings);
    let mut title = source.title.clone();
    let mut section = None;
    let mut tasks: Vec<Task> = Vec::new();
    // Where a task's successors may start: the next open day after a stretched
    // duration; an explicit end date stands as written.
    let mut ready: Vec<NaiveDate> = Vec::new();
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut default_start: Option<NaiveDate> = None;
    for &(number, line) in &source.lines[1..] {
        let fail = |kind| error(number, kind, line);
        let (keyword, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let mut note = |kind, severity| warnings.push(diagnostic(number, kind, line, severity));
        match keyword {
            "title" => {
                title = Some(value.trim().to_string());
                continue;
            }
            "dateFormat" => {
                if value.trim() != "YYYY-MM-DD" {
                    note(ErrorKind::UnsupportedDateFormat, Severity::Warning);
                }
                continue;
            }
            "section" => {
                section = Some(value.trim().to_string());
                continue;
            }
            "excludes" | "weekend" => continue,
            "includes" | "todayMarker" | "tickInterval" | "weekday" | "inclusiveEndDates" => {
                note(ErrorKind::Ignored(keyword.into()), Severity::Warning);
                continue;
            }
            "axisFormat" | "topAxis" | "displayMode" => {
                note(ErrorKind::IgnoredStyle(keyword.into()), Severity::Info);
                continue;
            }
            _ => {}
        }
        let Some((name, meta)) = line.split_once(':') else {
            return Err(fail(ErrorKind::UnexpectedText));
        };
        let name = decode_entities(name.trim());
        if name.is_empty() {
            return Err(fail(ErrorKind::EmptyLabel));
        }
        if tasks.len() >= MAX_TASKS {
            return Err(fail(ErrorKind::TooLarge));
        }
        let mut parts: Vec<&str> = meta.split(',').map(str::trim).collect();
        let mut task = Task {
            name: name.chars().take(MAX_LABEL_CHARS).collect(),
            section: section.clone(),
            start: NaiveDate::MIN,
            end: NaiveDate::MIN,
            done: false,
            active: false,
            critical: false,
            milestone: false,
        };
        while let Some(&tag) = parts.first() {
            match tag {
                "done" => task.done = true,
                "active" => task.active = true,
                "crit" => task.critical = true,
                "milestone" => task.milestone = true,
                _ => break,
            }
            parts.remove(0);
        }
        // One part: duration after the previous task; two: start, end;
        // three: id, start, end (Mermaid's own reading).
        let (id, start, end) = match parts.as_slice() {
            [end] => (None, None, *end),
            [start, end] => (None, Some(*start), *end),
            [id, start, end] => (Some(*id), Some(*start), *end),
            _ => return Err(fail(ErrorKind::UnexpectedText)),
        };
        let start = match start {
            Some(text) if text.starts_with("after ") => {
                let mut latest = None;
                for other in text["after ".len()..].split_whitespace() {
                    let index = *ids
                        .get(other)
                        .ok_or_else(|| fail(ErrorKind::UnknownTask(other.into())))?;
                    latest = latest.max(Some(ready[index]));
                }
                latest.ok_or_else(|| fail(ErrorKind::BadDate))?
            }
            Some(text) => parse_date(text).ok_or_else(|| fail(ErrorKind::BadDate))?,
            None => ready
                .last()
                .copied()
                .or(default_start)
                .ok_or_else(|| fail(ErrorKind::BadDate))?,
        };
        let (end_date, next) = if let Some(date) = parse_date(end) {
            (date, date)
        } else {
            let days = parse_duration(end).ok_or_else(|| fail(ErrorKind::BadDuration))?;
            let end = calendar
                .finish(start, days)
                .ok_or_else(|| fail(ErrorKind::BadDuration))?;
            let next = calendar
                .open_from(end)
                .ok_or_else(|| fail(ErrorKind::BadDuration))?;
            (end, next)
        };
        task.start = start;
        task.end = if task.milestone {
            start
        } else {
            end_date.max(start)
        };
        default_start.get_or_insert(start);
        if let Some(id) = id {
            ids.insert(id.to_string(), tasks.len());
        }
        ready.push(if task.milestone {
            start
        } else {
            next.max(start)
        });
        tasks.push(task);
    }
    Ok(Diagram::Gantt(Gantt { title, tasks }))
}
