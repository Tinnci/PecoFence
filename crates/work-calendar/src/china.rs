use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use std::collections::BTreeMap;
pub const MAX_BYTES: usize = 256 * 1024;
pub const SOURCE: &str = "https://github.com/NateScarlet/holiday-cn";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    pub name: String,
    pub is_off_day: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YearSchedule {
    pub year: i32,
    pub papers: Vec<String>,
    pub checked_on: Option<NaiveDate>,
    pub(crate) days: BTreeMap<NaiveDate, Day>,
}

#[derive(Deserialize)]
struct Document {
    year: i32,
    papers: Vec<String>,
    days: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    date: String,
    name: String,
    is_off_day: bool,
}

impl YearSchedule {
    pub fn day(&self, date: NaiveDate) -> Option<&Day> {
        self.days.get(&date)
    }
    pub fn parse(
        bytes: &[u8],
        expected_year: i32,
        checked_on: Option<NaiveDate>,
    ) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("holiday document is too large".into());
        }
        let doc: Document = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if doc.year != expected_year || !(1900..=2200).contains(&doc.year) {
            return Err("holiday year mismatch".into());
        }
        if doc.papers.is_empty() || doc.days.is_empty() {
            return Err("annual arrangement has not been published".into());
        }
        if doc.papers.len() > 16
            || doc.days.len() > 400
            || doc.papers.iter().any(|url| {
                !url.starts_with("https://www.gov.cn/") || url.chars().any(char::is_control)
            })
        {
            return Err("invalid holiday announcement sources".into());
        }
        let mut days = BTreeMap::new();
        for entry in doc.days {
            let date =
                NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").map_err(|e| e.to_string())?;
            if date.to_string() != entry.date
                || (date.year() - doc.year).abs() > 1
                || entry.name.trim().is_empty()
                || entry.name.chars().count() > 40
                || entry.name.chars().any(char::is_control)
            {
                return Err("invalid holiday date or name".into());
            }
            let day = Day {
                name: entry.name,
                is_off_day: entry.is_off_day,
            };
            if let Some(previous) = days.insert(date, day.clone())
                && previous != day
            {
                return Err("conflicting holiday dates".into());
            }
        }
        Ok(Self {
            year: doc.year,
            papers: doc.papers,
            days,
            checked_on,
        })
    }
}

pub fn bundled(year: i32) -> Option<&'static [u8]> {
    match year {
        2025 => Some(include_bytes!("../data/china/2025.json")),
        2026 => Some(include_bytes!("../data/china/2026.json")),
        2027 => Some(include_bytes!("../data/china/2027.json")),
        _ => None,
    }
}
