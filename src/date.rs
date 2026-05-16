use anyhow::{anyhow, Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct CivilDate {
    pub(crate) year: i32,
    pub(crate) month: u8,
    pub(crate) day: u8,
}

impl CivilDate {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value.len() != 10 {
            return None;
        }
        let year = value.get(0..4)?.parse::<i32>().ok()?;
        let month = value.get(5..7)?.parse::<u8>().ok()?;
        let day = value.get(8..10)?.parse::<u8>().ok()?;
        if value.as_bytes().get(4) != Some(&b'-') || value.as_bytes().get(7) != Some(&b'-') {
            return None;
        }
        if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
            return None;
        }
        Some(Self { year, month, day })
    }

    pub(crate) fn month_index(self) -> i32 {
        self.year * 12 + i32::from(self.month) - 1
    }

    pub(crate) fn from_month_index(index: i32) -> (i32, u8) {
        let year = index.div_euclid(12);
        let month = index.rem_euclid(12) + 1;
        (year, month as u8)
    }

    pub(crate) fn days_since_epoch(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    pub(crate) fn add_days(self, days: i64) -> Self {
        civil_from_days(self.days_since_epoch() + days)
    }

    pub(crate) fn weekday_sunday_index(self) -> usize {
        let days = days_from_civil(self.year, self.month, self.day);
        (days + 4).rem_euclid(7) as usize
    }
}

impl std::fmt::Display for CivilDate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

pub(crate) fn parse_date_filter_value(value: &str, today: CivilDate) -> Result<CivilDate> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("today") {
        return Ok(today);
    }
    if let Some(days) = value.strip_suffix('d').or_else(|| value.strip_suffix('D')) {
        let days = days
            .parse::<i64>()
            .with_context(|| format!("parse relative date filter {value:?}"))?;
        if days < 0 {
            return Err(anyhow!("relative date filters must be non-negative"));
        }
        return Ok(today.add_days(-days));
    }
    CivilDate::parse(value).ok_or_else(|| {
        anyhow!("invalid date filter {value:?}; use YYYY-MM-DD, today, or a relative value like 7d")
    })
}

pub(crate) fn today_utc() -> Result<CivilDate> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| anyhow!("system time is before Unix epoch: {err}"))?;
    let days = (duration.as_secs() / 86_400) as i64;
    Ok(civil_from_days(days))
}

pub(crate) fn month_abbr(month: u8) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "   ",
    }
}

pub(crate) fn weekday_label(weekday: usize) -> &'static str {
    match weekday {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "",
    }
}

pub(crate) fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> CivilDate {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    CivilDate {
        year: year as i32,
        month: month as u8,
        day: day as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_date_filter_value, CivilDate};

    #[test]
    fn parses_valid_calendar_dates() {
        assert_eq!(
            CivilDate::parse("2026-05-13"),
            Some(CivilDate {
                year: 2026,
                month: 5,
                day: 13
            })
        );
        assert_eq!(CivilDate::parse("2026-02-29"), None);
        assert_eq!(CivilDate::parse("2026-05-13-extra"), None);
        assert_eq!(
            CivilDate::parse("2024-02-29"),
            Some(CivilDate {
                year: 2024,
                month: 2,
                day: 29
            })
        );
    }

    #[test]
    fn parses_absolute_and_relative_date_filters() {
        let today = CivilDate::parse("2026-05-16").unwrap();
        assert_eq!(
            parse_date_filter_value("2026-05-01", today).unwrap(),
            CivilDate::parse("2026-05-01").unwrap()
        );
        assert_eq!(
            parse_date_filter_value("today", today).unwrap(),
            CivilDate::parse("2026-05-16").unwrap()
        );
        assert_eq!(
            parse_date_filter_value("7d", today).unwrap(),
            CivilDate::parse("2026-05-09").unwrap()
        );
        assert!(parse_date_filter_value("bad", today).is_err());
    }

    #[test]
    fn maps_weekday_with_sunday_origin() {
        assert_eq!(
            CivilDate::parse("2026-05-10")
                .unwrap()
                .weekday_sunday_index(),
            0
        );
        assert_eq!(
            CivilDate::parse("2026-05-13")
                .unwrap()
                .weekday_sunday_index(),
            3
        );
        assert_eq!(
            CivilDate::parse("2026-05-16")
                .unwrap()
                .weekday_sunday_index(),
            6
        );
    }

    #[test]
    fn adds_days_across_month_boundaries() {
        assert_eq!(
            CivilDate::parse("2026-03-01").unwrap().add_days(-1),
            CivilDate {
                year: 2026,
                month: 2,
                day: 28
            }
        );
        assert_eq!(
            CivilDate::parse("2024-02-28").unwrap().add_days(1),
            CivilDate {
                year: 2024,
                month: 2,
                day: 29
            }
        );
    }
}
