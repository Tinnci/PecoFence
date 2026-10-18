//! Chinese calendar annotations for the calendar widget: lunar dates, the 24 solar
//! terms and common festivals. Lunar conversion covers 1901–2100; solar terms use
//! the 21st-century formula and are shown for 2001–2099 only.

use chinese_lunisolar_calendar::{ChineseVariant, LunarMonth, LunisolarDate};
use chrono::{Datelike, NaiveDate};

/// Short label under a date: festival, then solar term, then the lunar day
/// (the first day of a lunar month shows the month name instead).
pub(crate) struct DayNote {
    pub(crate) text: &'static str,
    /// Festivals and solar terms are emphasized; plain lunar days are not.
    pub(crate) highlight: bool,
}

// (name, C) per term in calendar order: Jan 小寒 … Dec 冬至.
const TERMS: [(&str, f64); 24] = [
    ("小寒", 5.4055),
    ("大寒", 20.12),
    ("立春", 3.87),
    ("雨水", 18.73),
    ("惊蛰", 5.63),
    ("春分", 20.646),
    ("清明", 4.81),
    ("谷雨", 20.1),
    ("立夏", 5.52),
    ("小满", 21.04),
    ("芒种", 5.678),
    ("夏至", 21.37),
    ("小暑", 7.108),
    ("大暑", 22.83),
    ("立秋", 7.5),
    ("处暑", 23.13),
    ("白露", 7.646),
    ("秋分", 23.042),
    ("寒露", 8.318),
    ("霜降", 23.438),
    ("立冬", 7.438),
    ("小雪", 22.36),
    ("大雪", 7.18),
    ("冬至", 21.94),
];

// Published corrections where the formula is one day off in 2001–2099.
const TERM_CORRECTIONS: [(i32, usize, i32); 10] = [
    (2019, 0, -1),
    (2082, 1, 1),
    (2026, 3, -1),
    (2084, 5, 1),
    (2008, 9, 1),
    (2016, 12, 1),
    (2002, 14, 1),
    (2089, 19, 1),
    (2089, 20, 1),
    (2021, 23, -1),
];

pub(crate) fn solar_term(date: NaiveDate) -> Option<&'static str> {
    let year = date.year();
    if !(2001..=2099).contains(&year) {
        return None;
    }
    let y = (year % 100) as f64;
    let month0 = date.month0() as usize;
    for index in [month0 * 2, month0 * 2 + 1] {
        let (name, c) = TERMS[index];
        // January and February terms belong to the previous year's leap cycle.
        let leap = if index < 4 { (y - 1.0) / 4.0 } else { y / 4.0 }.floor();
        let correction = TERM_CORRECTIONS
            .iter()
            .find(|(y, i, _)| *y == year && *i == index)
            .map_or(0, |(_, _, delta)| *delta);
        let day = (y * 0.2422 + c).floor() as i32 - leap as i32 + correction;
        if day == date.day() as i32 {
            return Some(name);
        }
    }
    None
}

fn lunar(date: NaiveDate) -> Option<LunisolarDate> {
    LunisolarDate::from_date(date).ok()
}

fn solar_festival(date: NaiveDate) -> Option<&'static str> {
    Some(match (date.month(), date.day()) {
        (1, 1) => "元旦",
        (3, 8) => "妇女节",
        (5, 1) => "劳动节",
        (5, 4) => "青年节",
        (6, 1) => "儿童节",
        (9, 10) => "教师节",
        (10, 1) => "国庆节",
        _ => return None,
    })
}

fn lunar_festival(date: NaiveDate, day: LunisolarDate) -> Option<&'static str> {
    let month = day.to_lunar_month();
    if month.is_leap_month() {
        return None;
    }
    let found = match (month.to_u8(), day.to_lunar_day().to_u8()) {
        (1, 1) => "春节",
        (1, 15) => "元宵节",
        (5, 5) => "端午节",
        (7, 7) => "七夕",
        (8, 15) => "中秋节",
        (9, 9) => "重阳节",
        (12, 8) => "腊八",
        _ => "",
    };
    if !found.is_empty() {
        return Some(found);
    }
    // New Year's Eve is the last day of the twelfth month, which may be the 29th or 30th.
    let next = date.succ_opt().and_then(lunar)?;
    (month == LunarMonth::Twelfth && next.to_lunar_month().to_u8() == 1).then_some("除夕")
}

pub(crate) fn day_note(date: NaiveDate) -> Option<DayNote> {
    let lunar_day = lunar(date);
    let festival = solar_festival(date).or_else(|| lunar_day.and_then(|d| lunar_festival(date, d)));
    if let Some(text) = festival.or_else(|| solar_term(date)) {
        return Some(DayNote {
            text,
            highlight: true,
        });
    }
    let lunar_day = lunar_day?;
    let day = lunar_day.to_lunar_day();
    Some(DayNote {
        text: if day.to_u8() == 1 {
            lunar_day.to_lunar_month().to_str(ChineseVariant::Simple)
        } else {
            day.to_str()
        },
        highlight: false,
    })
}

/// "农历九月初八", with the festival or solar term appended when there is one.
pub(crate) fn describe(date: NaiveDate) -> Option<String> {
    let day = lunar(date)?;
    let mut text = format!(
        "农历{}{}",
        day.to_lunar_month().to_str(ChineseVariant::Simple),
        day.to_lunar_day().to_str()
    );
    if let Some(note) = day_note(date).filter(|note| note.highlight) {
        text.push_str(" · ");
        text.push_str(note.text);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn solar_terms_match_published_dates_including_corrections() {
        for (date, name) in [
            (d(2026, 1, 5), "小寒"),
            (d(2026, 2, 4), "立春"),
            (d(2026, 2, 18), "雨水"),
            (d(2026, 4, 5), "清明"),
            (d(2026, 10, 23), "霜降"),
            (d(2026, 12, 22), "冬至"),
            (d(2024, 3, 20), "春分"),
            (d(2019, 1, 5), "小寒"),
            (d(2021, 12, 21), "冬至"),
            (d(2016, 7, 7), "小暑"),
        ] {
            assert_eq!(solar_term(date), Some(name), "{date}");
        }
        assert_eq!(solar_term(d(2026, 2, 19)), None);
        assert_eq!(solar_term(d(1999, 12, 22)), None);
    }

    #[test]
    fn festivals_take_priority_over_lunar_days() {
        // 2024-02-10 was the Spring Festival; the 9th was New Year's Eve.
        assert_eq!(day_note(d(2024, 2, 10)).unwrap().text, "春节");
        assert_eq!(day_note(d(2024, 2, 9)).unwrap().text, "除夕");
        assert_eq!(day_note(d(2024, 9, 17)).unwrap().text, "中秋节");
        assert_eq!(day_note(d(2026, 10, 1)).unwrap().text, "国庆节");
        let plain = day_note(d(2024, 2, 15)).unwrap();
        assert_eq!(plain.text, "初六");
        assert!(!plain.highlight);
        // The first lunar day names its month.
        assert_eq!(day_note(d(2024, 3, 10)).unwrap().text, "二月");
        assert_eq!(describe(d(2024, 9, 17)).unwrap(), "农历八月十五 · 中秋节");
    }
}
