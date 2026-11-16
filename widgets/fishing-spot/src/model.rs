// Copyright (C) 2026  Braiins Forge s.r.o.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Braiins Systems s.r.o. and Braiins Forge s.r.o. each reserve the right
// to grant any party a license to this program, or any part thereof,
// under any terms, and such a grant shall be considered distinct from
// the grant above.

//! The Open-Meteo replies the index is computed from, and the calendar
//! helpers the scoring needs.
//!
//! The three replies are the ones the dovepescare.com site reads when it
//! calls Open-Meteo itself (`assets/js/api.js`): the daily forecast, the
//! river discharge, and the sea surface temperature and waves.

use bmc_wasm_sdk::*;

use crate::url::PAST_DAYS;

/// One forecast day. Every measured field is optional: the provider can drop
/// a variable for a single day, and a gap must read as "unavailable" rather
/// than as a zero that would quietly move the score.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Day {
    /// Calendar date, `YYYY-MM-DD`, Italian time.
    pub date: String,
    /// WMO weather interpretation code.
    pub weather_code: Option<i64>,
    pub t_max: Option<f64>,
    pub t_min: Option<f64>,
    pub t_mean: Option<f64>,
    /// Precipitation total, mm.
    pub precipitation: Option<f64>,
    /// Maximum wind speed, km/h.
    pub wind: Option<f64>,
    /// Mean sea-level pressure, hPa.
    pub pressure: Option<f64>,
    /// Mean cloud cover, percent.
    pub cloud_cover: Option<f64>,
    /// Local time of day, `HH:MM`, empty when the provider omitted it.
    pub sunrise: String,
    pub sunset: String,
    /// Mean sea surface temperature of the day, °C. Sea spots only.
    pub sea_temp: Option<f64>,
    /// Highest wave of the day, m. Sea spots only.
    pub wave: Option<f64>,
}

/// The forecast window: past days for the water model and the rain total,
/// then today and the days ahead.
#[derive(Clone, Debug, Default)]
pub struct Forecast {
    /// Ground elevation, metres. Sets the annual mean the water reverts to.
    pub elevation: f64,
    /// The reply had a daily mean air temperature series. Without one, the
    /// mean is taken from the day's maximum and minimum.
    pub has_mean: bool,
    pub days: Vec<Day>,
    /// Index of today within `days`.
    pub today_index: usize,
}

impl Forecast {
    /// Absolute index of the day at `offset` days after today.
    #[must_use]
    pub fn index_of(&self, offset: usize) -> Option<usize> {
        let index = self.today_index + offset;
        (index < self.days.len()).then_some(index)
    }

    /// Put the sea readings into the days they belong to.
    pub fn merge_sea(&mut self, sea: &Sea) {
        for day in &mut self.days {
            let reading = sea.days.iter().find(|s| s.date == day.date);
            day.sea_temp = reading.and_then(|s| s.temp);
            day.wave = reading.and_then(|s| s.wave);
        }
    }

    /// Remove the sea readings, when the marine reply is no longer good.
    pub fn clear_sea(&mut self) {
        for day in &mut self.days {
            day.sea_temp = None;
            day.wave = None;
        }
    }
}

/// River discharge per day, m³/s (GloFAS through Open-Meteo).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flow {
    pub days: Vec<(String, Option<f64>)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeaDay {
    pub date: String,
    pub temp: Option<f64>,
    pub wave: Option<f64>,
}

/// Sea surface temperature (the day's mean of the hourly values, to 0.1 °C,
/// as the site keeps it) and the day's highest wave.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sea {
    pub days: Vec<SeaDay>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    InvalidDocument,
    NoDays,
}

impl TryFrom<&JsonDoc> for Forecast {
    type Error = ParseError;

    fn try_from(doc: &JsonDoc) -> Result<Self, Self::Error> {
        if !doc.is_valid() {
            return Err(ParseError::InvalidDocument);
        }
        let count = doc.len("/daily/time").unwrap_or(0);
        let mut days = Vec::with_capacity(count);
        for i in 0..count {
            let Some(date) = doc.str(&fmt!("/daily/time/{}", i)) else {
                continue;
            };
            days.push(Day {
                date,
                weather_code: doc.i64(&fmt!("/daily/weather_code/{}", i)),
                t_max: doc.f64(&fmt!("/daily/temperature_2m_max/{}", i)),
                t_min: doc.f64(&fmt!("/daily/temperature_2m_min/{}", i)),
                t_mean: doc.f64(&fmt!("/daily/temperature_2m_mean/{}", i)),
                precipitation: doc.f64(&fmt!("/daily/precipitation_sum/{}", i)),
                wind: doc.f64(&fmt!("/daily/wind_speed_10m_max/{}", i)),
                pressure: doc.f64(&fmt!("/daily/pressure_msl_mean/{}", i)),
                cloud_cover: doc.f64(&fmt!("/daily/cloud_cover_mean/{}", i)),
                sunrise: doc
                    .str(&fmt!("/daily/sunrise/{}", i))
                    .map(|s| clock_of(&s))
                    .unwrap_or_default(),
                sunset: doc
                    .str(&fmt!("/daily/sunset/{}", i))
                    .map(|s| clock_of(&s))
                    .unwrap_or_default(),
                sea_temp: None,
                wave: None,
            });
        }
        if days.is_empty() {
            return Err(ParseError::NoDays);
        }

        // `current.time` is the provider's clock in Italian time, which is
        // what decides "today", as on the site.
        let now = doc.str("/current/time").unwrap_or_default();
        let today = now.get(..10).unwrap_or_default();
        let today_index = today_index(&days, today);

        Ok(Forecast {
            elevation: doc.f64("/elevation").unwrap_or(0.0),
            has_mean: doc.len("/daily/temperature_2m_mean").is_some(),
            days,
            today_index,
        })
    }
}

impl TryFrom<&JsonDoc> for Flow {
    type Error = ParseError;

    fn try_from(doc: &JsonDoc) -> Result<Self, Self::Error> {
        if !doc.is_valid() {
            return Err(ParseError::InvalidDocument);
        }
        let count = doc.len("/daily/time").unwrap_or(0);
        let days: Vec<(String, Option<f64>)> = (0..count)
            .filter_map(|i| {
                let date = doc.str(&fmt!("/daily/time/{}", i))?;
                Some((date, doc.f64(&fmt!("/daily/river_discharge/{}", i))))
            })
            .collect();
        if days.is_empty() {
            return Err(ParseError::NoDays);
        }
        Ok(Flow { days })
    }
}

impl TryFrom<&JsonDoc> for Sea {
    type Error = ParseError;

    fn try_from(doc: &JsonDoc) -> Result<Self, Self::Error> {
        if !doc.is_valid() {
            return Err(ParseError::InvalidDocument);
        }
        // The hourly temperature becomes the day's mean, as api.js does it.
        let mut temps: Vec<(String, f64, u32)> = Vec::new();
        let hours = doc.len("/hourly/time").unwrap_or(0);
        for i in 0..hours {
            let (Some(time), Some(v)) = (
                doc.str(&fmt!("/hourly/time/{}", i)),
                doc.f64(&fmt!("/hourly/sea_surface_temperature/{}", i)),
            ) else {
                continue;
            };
            let date = time.get(..10).unwrap_or_default().to_owned();
            match temps.iter_mut().find(|(d, _, _)| *d == date) {
                Some((_, sum, n)) => {
                    *sum += v;
                    *n += 1;
                }
                None => temps.push((date, v, 1)),
            }
        }
        let mut days: Vec<SeaDay> = Vec::new();
        let count = doc.len("/daily/time").unwrap_or(0);
        for i in 0..count {
            let Some(date) = doc.str(&fmt!("/daily/time/{}", i)) else {
                continue;
            };
            let temp = temps
                .iter()
                .find(|(d, _, _)| *d == date)
                .map(|(_, sum, n)| round_tenth(sum / f64::from(*n)));
            days.push(SeaDay {
                date,
                temp,
                wave: doc.f64(&fmt!("/daily/wave_height_max/{}", i)),
            });
        }
        // Hourly days that the daily series does not list keep their mean.
        for (date, sum, n) in &temps {
            if !days.iter().any(|d| d.date == *date) {
                days.push(SeaDay {
                    date: date.clone(),
                    temp: Some(round_tenth(sum / f64::from(*n))),
                    wave: None,
                });
            }
        }
        if days.is_empty() {
            return Err(ParseError::NoDays);
        }
        Ok(Sea { days })
    }
}

/// `Math.round(x * 10) / 10`.
fn round_tenth(x: f64) -> f64 {
    let scaled = x * 10.0;
    let floor = scaled.floor();
    let rounded = if scaled - floor >= 0.5 { floor + 1.0 } else { floor };
    rounded / 10.0
}

/// Index of the entry whose date matches `today`. When the provider's clock
/// and the data disagree, the day after the past days the request asked for.
#[must_use]
pub fn today_index(days: &[Day], today: &str) -> usize {
    days.iter()
        .position(|d| d.date == today)
        .unwrap_or_else(|| PAST_DAYS.min(days.len().saturating_sub(1)))
}

/// Parse `YYYY-MM-DD` (or any longer string starting with one) into its
/// calendar parts. `None` for anything malformed.
#[must_use]
pub fn parse_date(date: &str) -> Option<(i64, u32, u32)> {
    let b = date.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let year = parse_digits(&b[0..4])?;
    let month = u32::try_from(parse_digits(&b[5..7])?).ok()?;
    let day = u32::try_from(parse_digits(&b[8..10])?).ok()?;
    if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

/// The `HH:MM` inside an Open-Meteo local timestamp (`YYYY-MM-DDTHH:MM`).
#[must_use]
pub fn clock_of(timestamp: &str) -> String {
    timestamp.get(11..16).unwrap_or_default().to_owned()
}

/// Shift an `HH:MM` clock time by `minutes`, wrapping within the day.
#[must_use]
pub fn shift_clock(clock: &str, minutes: i64) -> Option<String> {
    let b = clock.as_bytes();
    if b.len() < 5 || b[2] != b':' {
        return None;
    }
    let hours = parse_digits(&b[0..2])?;
    let mins = parse_digits(&b[3..5])?;
    let total = (hours * 60 + mins + minutes).rem_euclid(1440);
    let mut out = String::with_capacity(5);
    push_two_digits(&mut out, total / 60);
    out.push(':');
    push_two_digits(&mut out, total % 60);
    Some(out)
}

fn push_two_digits(out: &mut String, value: i64) {
    let value = value.clamp(0, 99);
    out.push(char::from(b'0' + u8::try_from(value / 10).unwrap_or(0)));
    out.push(char::from(b'0' + u8::try_from(value % 10).unwrap_or(0)));
}

/// Short English weekday for a `YYYY-MM-DD` date.
#[must_use]
pub fn weekday_short(date: &str) -> Option<&'static str> {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let (year, month, day) = parse_date(date)?;
    let days = days_from_civil(year, month, day)?;
    WEEKDAYS.get(usize::try_from(weekday(days)).ok()?).copied()
}

/// Short English month name for a `YYYY-MM-DD` date.
#[must_use]
pub fn month_short(date: &str) -> Option<&'static str> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (_, month, _) = parse_date(date)?;
    MONTHS
        .get(usize::try_from(month).ok()?.checked_sub(1)?)
        .copied()
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in a month (1-based).
#[must_use]
pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Day of the week, 0 = Sunday, for days since 1970-01-01.
#[must_use]
pub fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

/// `n` days later (or earlier).
#[must_use]
pub const fn add_days(days: i64, n: i64) -> i64 {
    days + n
}

/// Days from 1970-01-01 to a proleptic-Gregorian date (Howard Hinnant's
/// algorithm). `None` for an out-of-range month or day.
#[must_use]
pub fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let month = i64::from(month);
    let day = i64::from(day);
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2).div_euclid(5) + day - 1;
    let doe = yoe * 365 + yoe.div_euclid(4) - yoe.div_euclid(100) + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The `YYYY-MM-DD` date of a day since 1970-01-01 (Hinnant's inverse).
#[must_use]
pub fn iso_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe.div_euclid(1460) + doe.div_euclid(36_524) - doe.div_euclid(146_096))
        .div_euclid(365);
    let doy = doe - (365 * yoe + yoe.div_euclid(4) - yoe.div_euclid(100));
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let mut out = String::with_capacity(10);
    let year = year.clamp(0, 9999);
    push_two_digits(&mut out, year.div_euclid(100));
    push_two_digits(&mut out, year.rem_euclid(100));
    out.push('-');
    push_two_digits(&mut out, month);
    out.push('-');
    push_two_digits(&mut out, day);
    out
}

fn parse_digits(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for &c in bytes {
        if !c.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i64::from(c - b'0');
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str) -> Day {
        Day {
            date: date.to_string(),
            ..Day::default()
        }
    }

    #[test]
    fn today_is_found_by_date_not_by_position() {
        let days = [day("2026-09-16"), day("2026-09-17"), day("2026-09-18")];
        assert_eq!(today_index(&days, "2026-09-18"), 2);
    }

    #[test]
    fn an_unmatched_today_falls_back_to_the_first_day_after_the_past_days() {
        let days: Vec<Day> = (1..=21)
            .map(|n| day(&iso_date(days_from_civil(2026, 9, n).unwrap_or(0))))
            .collect();
        assert_eq!(today_index(&days, ""), PAST_DAYS);
        assert_eq!(today_index(&days[..3], "1999-01-01"), 2);
    }

    #[test]
    fn dates_parse_and_reject_garbage() {
        assert_eq!(parse_date("2026-09-17"), Some((2026, 9, 17)));
        assert_eq!(parse_date("2026-09-17T12:00"), Some((2026, 9, 17)));
        assert_eq!(parse_date("2026/09/17"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("2026-09-1"), None);
        assert_eq!(parse_date(""), None);
    }

    #[test]
    fn civil_dates_round_trip() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 3, 29), (2026, 12, 31), (2027, 1, 1)] {
            let days = days_from_civil(y, m, d).unwrap_or(0);
            let iso = iso_date(days);
            assert_eq!(parse_date(&iso), Some((y, m, d)), "{iso}");
        }
        // 2026-09-17 is a Thursday.
        assert_eq!(weekday(days_from_civil(2026, 9, 17).unwrap_or(0)), 4);
    }

    #[test]
    fn weekday_and_month_read_the_calendar_head() {
        assert_eq!(weekday_short("2026-09-17"), Some("Thu"));
        assert_eq!(month_short("2026-09-17"), Some("Sep"));
        assert_eq!(weekday_short("nope"), None);
    }

    #[test]
    fn clock_extraction_needs_a_full_timestamp() {
        assert_eq!(clock_of("2026-09-17T06:55"), "06:55");
        assert_eq!(clock_of("2026-09-17"), "");
    }

    #[test]
    fn clock_shifts_wrap_within_the_day() {
        assert_eq!(shift_clock("06:55", -30).as_deref(), Some("06:25"));
        assert_eq!(shift_clock("06:55", 120).as_deref(), Some("08:55"));
        assert_eq!(shift_clock("23:30", 60).as_deref(), Some("00:30"));
        assert_eq!(shift_clock("00:10", -20).as_deref(), Some("23:50"));
        assert_eq!(shift_clock("bad", 10), None);
    }

    #[test]
    fn the_tenth_rounds_halves_up_like_the_site() {
        assert!((round_tenth(14.25) - 14.3).abs() < 1e-12);
        assert!((round_tenth(14.24) - 14.2).abs() < 1e-12);
    }

    #[test]
    fn sea_readings_land_on_their_own_days() {
        let mut forecast = Forecast {
            days: vec![day("2026-09-16"), day("2026-09-17")],
            ..Forecast::default()
        };
        forecast.merge_sea(&Sea {
            days: vec![SeaDay {
                date: "2026-09-17".to_owned(),
                temp: Some(24.1),
                wave: Some(0.6),
            }],
        });
        assert_eq!(forecast.days[0].sea_temp, None);
        assert_eq!(forecast.days[1].sea_temp, Some(24.1));
        assert_eq!(forecast.days[1].wave, Some(0.6));
        forecast.clear_sea();
        assert_eq!(forecast.days[1].wave, None);
    }

    #[test]
    fn replies_parse_on_the_host() {
        let forecast = JsonDoc::parse(
            br#"{"elevation":40,"current":{"time":"2026-09-17T10:00"},
                "daily":{"time":["2026-09-16","2026-09-17"],
                "temperature_2m_max":[24.0,null],"temperature_2m_mean":[20.0,21.5],
                "sunrise":["2026-09-16T06:54","2026-09-17T06:55"]}}"#,
        );
        let forecast = Forecast::try_from(&forecast).expect("BUG: a valid reply");
        assert_eq!(forecast.today_index, 1);
        assert!(forecast.has_mean);
        assert_eq!(forecast.days[0].t_max, Some(24.0));
        assert_eq!(forecast.days[1].t_max, None);
        assert_eq!(forecast.days[1].sunrise, "06:55");

        let flow = JsonDoc::parse(
            br#"{"daily":{"time":["2026-09-16","2026-09-17"],"river_discharge":[3.5,null]}}"#,
        );
        let flow = Flow::try_from(&flow).expect("BUG: a valid reply");
        assert_eq!(flow.days[1], ("2026-09-17".to_owned(), None));

        let sea = JsonDoc::parse(
            br#"{"hourly":{"time":["2026-09-17T00:00","2026-09-17T01:00"],
                "sea_surface_temperature":[24.0,24.4]},
                "daily":{"time":["2026-09-17"],"wave_height_max":[0.8]}}"#,
        );
        let sea = Sea::try_from(&sea).expect("BUG: a valid reply");
        assert_eq!(sea.days[0].temp, Some(24.2));
        assert_eq!(sea.days[0].wave, Some(0.8));

        assert!(Forecast::try_from(&JsonDoc::parse(b"{}")).is_err());
        assert!(Flow::try_from(&JsonDoc::parse(b"nope")).is_err());
    }
}
