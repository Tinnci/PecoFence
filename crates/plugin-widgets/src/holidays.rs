//! Mainland China holiday exceptions. Annual government arrangements are data,
//! independent of the lunar calendar. Rendering reads immutable snapshots only.
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

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
    days: BTreeMap<NaiveDate, Day>,
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

#[derive(Clone, Debug, Default)]
pub struct CalendarData {
    years: BTreeMap<i32, Arc<YearSchedule>>,
}
impl CalendarData {
    pub fn bundled() -> Self {
        let mut result = Self::default();
        for (year, bytes) in [
            (
                2025,
                include_bytes!("../data/holidays/2025.json").as_slice(),
            ),
            (
                2026,
                include_bytes!("../data/holidays/2026.json").as_slice(),
            ),
        ] {
            let schedule =
                YearSchedule::parse(bytes, year, None).expect("validated bundled holidays");
            result.years.insert(year, Arc::new(schedule));
        }
        result
    }
    pub fn year(&self, year: i32) -> Option<&YearSchedule> {
        self.years.get(&year).map(AsRef::as_ref)
    }
    /// Later annual notices may override dates in the preceding December.
    pub fn day(&self, date: NaiveDate) -> Option<&Day> {
        self.years
            .iter()
            .rev()
            .find_map(|(_, schedule)| schedule.days.get(&date))
    }
    pub fn description(&self, date: NaiveDate, chinese: bool) -> String {
        if let Some(day) = self.day(date) {
            return match (chinese, day.is_off_day) {
                (true, true) => format!("{}假期 · 休息", day.name),
                (true, false) => format!("{}调休 · 补班", day.name),
                (false, true) => format!("{} · Day off", day.name),
                (false, false) => format!("{} · Make-up workday", day.name),
            };
        }
        if self.year(date.year()).is_none() {
            return if chinese {
                format!("暂无{}年放假安排", date.year())
            } else {
                format!("No {} holiday data", date.year())
            };
        }
        match (chinese, date.weekday().number_from_monday() >= 6) {
            (true, true) => "周末休息".into(),
            (true, false) => "普通工作日".into(),
            (false, true) => "Weekend".into(),
            (false, false) => "Weekday".into(),
        }
    }
}

static DATA: OnceLock<RwLock<Arc<CalendarData>>> = OnceLock::new();
static REVISION: AtomicU64 = AtomicU64::new(0);
fn data() -> &'static RwLock<Arc<CalendarData>> {
    DATA.get_or_init(|| RwLock::new(Arc::new(CalendarData::bundled())))
}
pub fn snapshot() -> Arc<CalendarData> {
    data().read().unwrap_or_else(|e| e.into_inner()).clone()
}
pub fn revision() -> u64 {
    REVISION.load(Ordering::Acquire)
}
/// Called by the host worker after validation, never from the paint path.
pub fn publish(schedule: YearSchedule) -> bool {
    let mut current = data().write().unwrap_or_else(|e| e.into_inner());
    if current.year(schedule.year) == Some(&schedule) {
        return false;
    }
    let mut next = (**current).clone();
    next.years.insert(schedule.year, Arc::new(schedule));
    *current = Arc::new(next);
    REVISION.fetch_add(1, Ordering::Release);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    #[test]
    fn published_exceptions_override_weekends_without_turning_festivals_into_leave() {
        let data = CalendarData::bundled();
        assert!(!data.day(date("2026-10-10")).unwrap().is_off_day);
        assert!(data.day(date("2026-10-01")).unwrap().is_off_day);
        assert!(data.day(date("2026-10-18")).is_none()); // Chongyang is not a holiday exception.
        assert_eq!(
            data.description(date("2026-10-10"), true),
            "国庆节调休 · 补班"
        );
        assert_eq!(data.description(date("2026-10-18"), true), "周末休息");
        assert!(
            data.description(date("2030-10-01"), true)
                .contains("暂无2030")
        );
    }
    #[test]
    fn malformed_unpublished_wrong_year_and_conflicting_data_are_rejected() {
        let valid = br#"{"year":2026,"papers":["https://www.gov.cn/a"],"days":[{"date":"2026-10-10","name":"National Day","isOffDay":false}]}"#;
        assert!(YearSchedule::parse(valid, 2026, None).is_ok());
        assert!(YearSchedule::parse(valid, 2027, None).is_err());
        for text in [
            "{}",
            r#"{"year":2027,"papers":[],"days":[]}"#,
            r#"{"year":2026,"papers":["https://evil.invalid/a"],"days":[{"date":"2026-10-10","name":"x","isOffDay":false}]}"#,
            r#"{"year":2026,"papers":["https://www.gov.cn/a"],"days":[{"date":"2026-02-30","name":"x","isOffDay":true}]}"#,
            r#"{"year":2026,"papers":["https://www.gov.cn/a"],"days":[{"date":"2026-10-10","name":"x","isOffDay":true},{"date":"2026-10-10","name":"x","isOffDay":false}]}"#,
        ] {
            assert!(YearSchedule::parse(text.as_bytes(), 2026, None).is_err());
        }
        assert!(YearSchedule::parse(&vec![b' '; MAX_BYTES + 1], 2026, None).is_err());
    }
    #[test]
    fn next_year_notice_takes_precedence_on_cross_year_dates() {
        let mut data = CalendarData::bundled();
        let schedule = YearSchedule::parse(br#"{"year":2027,"papers":["https://www.gov.cn/a"],"days":[{"date":"2026-12-31","name":"New Year","isOffDay":true}]}"#, 2027, None).unwrap();
        data.years.insert(2027, Arc::new(schedule));
        assert!(data.day(date("2026-12-31")).unwrap().is_off_day);
    }
}
