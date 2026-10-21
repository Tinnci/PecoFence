//! Gantt charts: `title`, `dateFormat YYYY-MM-DD`, sections and tasks with dates,
//! `after` dependencies, durations and milestones. Drawn row by row, no layout pass.

use super::{Diagram, ErrorKind, MAX_LABEL_CHARS, MAX_TASKS, ParseError, Source, error};
use chrono::NaiveDate;
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

pub(super) fn parse(source: &Source) -> Result<Diagram, ParseError> {
    let mut title = source.title.clone();
    let mut section = None;
    let mut tasks: Vec<Task> = Vec::new();
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut default_start: Option<NaiveDate> = None;
    for &(number, line) in &source.lines[1..] {
        let fail = |kind| error(number, kind, line);
        if let Some(value) = line.strip_prefix("title ") {
            title = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("dateFormat ") {
            if value.trim() != "YYYY-MM-DD" {
                return Err(fail(ErrorKind::UnsupportedDateFormat));
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("section ") {
            section = Some(value.trim().to_string());
            continue;
        }
        if [
            "axisFormat",
            "excludes",
            "todayMarker",
            "tickInterval",
            "weekday",
            "includes",
            "inclusiveEndDates",
            "topAxis",
            "displayMode",
        ]
        .iter()
        .any(|k| line.starts_with(k))
        {
            continue;
        }
        let Some((name, meta)) = line.split_once(':') else {
            return Err(fail(ErrorKind::UnexpectedText));
        };
        let name = name.trim();
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
                    latest = latest.max(Some(tasks[index].end));
                }
                latest.ok_or_else(|| fail(ErrorKind::BadDate))?
            }
            Some(text) => parse_date(text).ok_or_else(|| fail(ErrorKind::BadDate))?,
            None => tasks
                .last()
                .map(|t| t.end)
                .or(default_start)
                .ok_or_else(|| fail(ErrorKind::BadDate))?,
        };
        let end_date = if let Some(date) = parse_date(end) {
            date
        } else {
            let days = parse_duration(end).ok_or_else(|| fail(ErrorKind::BadDuration))?;
            start
                .checked_add_signed(chrono::Duration::days(days))
                .ok_or_else(|| fail(ErrorKind::BadDuration))?
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
        tasks.push(task);
    }
    Ok(Diagram::Gantt(Gantt { title, tasks }))
}
