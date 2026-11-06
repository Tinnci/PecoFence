//! A common offline data contract for calendars, workday counts and exported plans.
//! Native desktop, SPM and Python consumers share the same country/year snapshots.
mod bundled;
pub mod china;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Day {
    pub date: String,
    pub name: String,
    pub is_off_day: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarYear {
    pub schema_version: u32,
    pub country: String,
    pub subdivision: Option<String>,
    pub year: i32,
    pub timezone: String,
    pub weekdays: Vec<u32>,
    pub source: String,
    pub source_url: String,
    pub revision: String,
    /// published for Chinese annual notices; calculated for rule-based provider data.
    pub status: String,
    pub days: Vec<Day>,
}
impl CalendarYear {
    pub fn parse(
        bytes: &[u8],
        country: &str,
        subdivision: Option<&str>,
        year: i32,
    ) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("calendar payload exceeds 1 MiB".into());
        }
        let value = if country == "CN" {
            let schedule = china::YearSchedule::parse(bytes, year, None)?;
            let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
                (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
            });
            Self {
                schema_version: 1,
                country: country.into(),
                subdivision: None,
                year,
                timezone: "Asia/Shanghai".into(),
                weekdays: vec![0, 1, 2, 3, 4],
                source: "NateScarlet/holiday-cn".into(),
                source_url: schedule.papers.first().cloned().unwrap_or_default(),
                revision: format!("fnv1a-{hash:016x}"),
                status: "published".into(),
                days: schedule
                    .days
                    .into_iter()
                    .map(|(date, day)| Day {
                        date: date.to_string(),
                        name: day.name,
                        is_off_day: day.is_off_day,
                    })
                    .collect(),
            }
        } else {
            serde_json::from_slice::<Self>(bytes).map_err(|e| e.to_string())?
        };
        if value.schema_version != 1
            || value.country != country
            || value.subdivision.as_deref() != subdivision
            || value.year != year
            || value.days.len() > 400
            || value.weekdays.is_empty()
            || value.weekdays.iter().any(|d| *d > 6)
        {
            return Err("calendar identity or workweek is invalid".into());
        }
        for day in &value.days {
            let date =
                NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").map_err(|e| e.to_string())?;
            if (date.year() - year).abs() > 1 || day.name.trim().is_empty() {
                return Err("invalid calendar date".into());
            }
        }
        Ok(value)
    }
    pub fn visualizer(&self) -> serde_json::Value {
        serde_json::json!({"id": key(&self.country,self.subdivision.as_deref()), "label": key(&self.country,self.subdivision.as_deref()),
            "region": self.country, "subdivision": self.subdivision.as_deref().unwrap_or(""), "timezone":self.timezone,
            "weekdays":self.weekdays, "source":self.source, "source_url":self.source_url,
            "coverage_start":format!("{}-01-01",self.year), "coverage_end":format!("{}-12-31",self.year),
            "holidays":self.days.iter().filter(|d| d.is_off_day && d.date.starts_with(&self.year.to_string())).map(|d| serde_json::json!({"date":d.date,"name":d.name,"source":self.source,"source_url":self.source_url})).collect::<Vec<_>>(),
            "working_dates":self.days.iter().filter(|d| !d.is_off_day && d.date.starts_with(&self.year.to_string())).map(|d| &d.date).collect::<Vec<_>>()})
    }
}
pub fn key(country: &str, subdivision: Option<&str>) -> String {
    subdivision
        .filter(|s| !s.is_empty())
        .map_or_else(|| country.into(), |s| format!("{country}-{s}"))
}
pub fn valid_region(country: &str, subdivision: Option<&str>) -> bool {
    country.len() == 2
        && country.bytes().all(|c| c.is_ascii_uppercase())
        && subdivision.is_none_or(|s| {
            !s.is_empty()
                && s.len() <= 24
                && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
pub fn bundled(country: &str, subdivision: Option<&str>, year: i32) -> Option<&'static [u8]> {
    if country == "CN" && subdivision.is_none() {
        china::bundled(year)
    } else {
        bundled::bytes(&key(country, subdivision), year)
    }
}
pub fn cache_directory() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|s| PathBuf::from(s).join(".local/state")))
        .map(|p| p.join("Tinnci/calendars"))
}
