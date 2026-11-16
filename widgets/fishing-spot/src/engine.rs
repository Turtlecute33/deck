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

//! The fishing index: a port of `valuta()` in the dovepescare.com engine
//! (`assets/js/engine.js`).
//!
//! It crosses the forecast with each species' season, temperature band,
//! flow, clarity, pressure and light preferences, estimates the water
//! temperature and clarity, applies the spot modifiers and closures, and
//! compresses the result onto 0-100. The operations keep the order of the
//! source, so the floating-point results agree with it; see the parity tests.

use crate::model::{
    Day, Flow, Forecast, add_days, days_from_civil, days_in_month, iso_date, parse_date,
    shift_clock, weekday,
};
use crate::spot::{
    Category, Light, Season, Spot, SpotSpecies, WaterKind, WinterClosure, Window as Closed,
    season_of,
};

use core::f64::consts::PI;

/// Mean annual air temperature in Emilia-Romagna at sea level, and its lapse
/// with elevation. The water reverts towards this.
const ANNUAL_MEAN_C: f64 = 14.6;
const LAPSE_PER_M: f64 = 0.0058;

/// Running waters: plain numbers up to 100 m, spring numbers from 900 m.
const PLAIN_ELEVATION: f64 = 100.0;
const PLAIN_DAMPING: f64 = 0.85;
const PLAIN_OFFSET: f64 = 0.75;
const MOUNTAIN_ELEVATION: f64 = 900.0;
const MOUNTAIN_DAMPING: f64 = 0.55;
const MOUNTAIN_OFFSET: f64 = -1.2;

/// Wave height, m: from 0.5 to 1.5 the sea is choppy and good from the shore;
/// from 2 nobody fishes from the piers.
const CHOPPY_WAVE: (f64, f64) = (0.5, 1.5);
const ROUGH_WAVE: f64 = 2.0;

/// `Date.UTC(2000, 0, 6, 18, 14)`: a new moon. And the synodic month, days.
const NEW_MOON_EPOCH_MS: f64 = 947_182_440_000.0;
const SYNODIC_DAYS: f64 = 29.530_588_853;

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    // `Math.min(b, Math.max(a, v))`, which never panics on lo > hi.
    hi.min(lo.max(v))
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let count = u32::try_from(values.len()).unwrap_or(u32::MAX);
    values.iter().fold(0.0, |sum, v| sum + v) / f64::from(count)
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        f64::midpoint(sorted[middle - 1], sorted[middle])
    }
}

/// `Math.round`: the nearest integer, halves up.
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
}

/// The species' response to the water temperature: an asymmetric bell that
/// is 1 at the centre of the optimum and 0.88 at its edges, and decays to
/// nearly 0 past the survivable limits. A missing reading scores neutral.
#[must_use]
pub fn bell(x: Option<f64>, opt: (f64, f64), live: (f64, f64)) -> f64 {
    let Some(x) = x else {
        return 0.6;
    };
    let (o_min, o_max) = opt;
    let (v_min, v_max) = live;
    if x >= o_min && x <= o_max {
        let centre = f64::midpoint(o_min, o_max);
        let half = (o_max - o_min) / 2.0;
        let half = if half == 0.0 { 1.0 } else { half };
        let d = (x - centre).abs() / half;
        return 1.0 - 0.12 * d * d;
    }
    if x < o_min {
        if x <= v_min {
            return 0.03;
        }
        return 0.03 + 0.97 * ((x - v_min) / (o_min - v_min)).powf(1.5);
    }
    if x >= v_max {
        return 0.03;
    }
    0.03 + 0.97 * ((v_max - x) / (v_max - o_max)).powf(1.5)
}

/// Estimated water temperature: the mean air of the last days, damped
/// towards the local annual mean. An estimate, and the faces say so.
#[must_use]
pub fn water_temperature(kind: WaterKind, air: &[Option<f64>], elevation: f64) -> Option<f64> {
    let p = kind.inertia();
    let start = air.len().saturating_sub(p.days);
    let window: Vec<f64> = air[start..].iter().filter_map(|v| *v).collect();
    if window.is_empty() {
        return None;
    }
    let air = mean(&window);
    let annual = ANNUAL_MEAN_C - LAPSE_PER_M * elevation;
    let (damping, offset) = if p.running {
        let f = clamp(
            (elevation - PLAIN_ELEVATION) / (MOUNTAIN_ELEVATION - PLAIN_ELEVATION),
            0.0,
            1.0,
        );
        (
            PLAIN_DAMPING + (MOUNTAIN_DAMPING - PLAIN_DAMPING) * f,
            PLAIN_OFFSET + (MOUNTAIN_OFFSET - PLAIN_OFFSET) * f,
        )
    } else {
        (p.damping, p.offset)
    };
    Some(clamp(
        annual + damping * (air - annual) + offset,
        1.0,
        p.ceiling,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moon {
    /// Illuminated fraction of the disc, 0.0 to 1.0.
    pub illumination: f64,
    /// Position in the cycle, 0.0 at new moon.
    pub phase: f64,
    /// Days since the last new moon.
    pub age_days: u32,
    pub name: &'static str,
}

/// Hours Europe/Rome is ahead of UTC at noon on a day (days since 1970).
fn rome_offset_hours(days: i64, year: i64) -> f64 {
    let summer_from = last_sunday(year, 3);
    let summer_to = last_sunday(year, 10);
    if days >= summer_from && days < summer_to {
        2.0
    } else {
        1.0
    }
}

/// Moon phase at noon, Italian time, on a day (days since 1970).
#[must_use]
pub fn moon_phase(days: i64, year: i64) -> Moon {
    let offset = rome_offset_hours(days, year);
    // Through i32 so the widening to f64 is exact.
    let days_f = f64::from(i32::try_from(days).unwrap_or(0));
    let ms = days_f * 86_400_000.0 + (12.0 - offset) * 3_600_000.0;
    let age = ((ms - NEW_MOON_EPOCH_MS) / 86_400_000.0 % SYNODIC_DAYS + SYNODIC_DAYS) % SYNODIC_DAYS;
    let phase = age / SYNODIC_DAYS;
    let illumination = (1.0 - (2.0 * PI * phase).cos()) / 2.0;
    let name = if !(0.03..=0.97).contains(&phase) {
        "New moon"
    } else if phase < 0.22 {
        "Waxing crescent"
    } else if phase < 0.28 {
        "First quarter"
    } else if phase < 0.47 {
        "Waxing gibbous"
    } else if phase < 0.53 {
        "Full moon"
    } else if phase < 0.72 {
        "Waning gibbous"
    } else if phase < 0.78 {
        "Last quarter"
    } else {
        "Waning crescent"
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the age is inside the synodic month, so it fits a u32"
    )]
    let age_days = js_round(age) as u32;
    Moon {
        illumination,
        phase,
        age_days,
        name,
    }
}

/// The last Sunday of a month (1-based), as days since 1970.
fn last_sunday(year: i64, month: u32) -> i64 {
    let last = days_from_civil(year, month, days_in_month(year, month)).unwrap_or(0);
    last - weekday(last)
}

/// The first Sunday of a month (1-based), as days since 1970.
fn first_sunday(year: i64, month: u32) -> i64 {
    let first = days_from_civil(year, month, 1).unwrap_or(0);
    first + (7 - weekday(first)) % 7
}

/// `stagioneChiusa()`: `None` while the trout season is open, else the
/// Sunday it opens again. It closes after the first Sunday of October and
/// opens on the last Sunday of March, or on the first Sunday of May.
fn trout_season_reopens(days: i64, year: i64, may: bool) -> Option<i64> {
    let opening = |y: i64| {
        if may {
            first_sunday(y, 5)
        } else {
            last_sunday(y, 3)
        }
    };
    if days < opening(year) {
        return Some(opening(year));
    }
    let closes = first_sunday(year, 10) + 1;
    (days >= closes).then(|| opening(year + 1))
}

/// Whether `[m1, d1, m2, d2]` holds the date; the window can wrap the year.
fn in_window(month: u32, day: u32, w: Closed) -> bool {
    let v = month * 100 + day;
    let a = w[0] * 100 + w[1];
    let b = w[2] * 100 + w[3];
    if a <= b {
        v >= a && v <= b
    } else {
        v >= a || v <= b
    }
}

/// The monthly activity, without steps at the month boundaries: each month's
/// value holds at mid-month and fades towards the next. A zero stays a step.
fn seasonal(months: &[f64; 12], year: i64, month0: usize, day: u32) -> f64 {
    let v = months[month0];
    if v == 0.0 {
        return 0.0;
    }
    let month1 = u32::try_from(month0 + 1).unwrap_or(1);
    let days = f64::from(days_in_month(year, month1));
    let x = (f64::from(day) - 0.5) / days - 0.5;
    let near = months[(month0 + if x < 0.0 { 11 } else { 1 }) % 12];
    if near == 0.0 {
        v
    } else {
        v + (near - v) * x.abs()
    }
}

/// Why a spot is closed to fishing on a day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Closure {
    /// A temporary ban on the whole spot.
    Notice,
    /// The winter closure of trout waters; opens again on this day.
    Season { reopens: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modifier {
    Closed(Closure),
    Flood,
    LowWater,
    HeavyRain,
    Thunderstorms,
    RoughSea,
    StrongWind,
    ChoppySea,
    CoolWater,
    TopSeason,
    NothingKeepable,
}

impl Modifier {
    /// Whether it raised the index.
    #[must_use]
    pub fn positive(&self) -> bool {
        matches!(self, Self::ChoppySea | Self::CoolWater | Self::TopSeason)
    }

    /// A stable key for each kind, for the parity tests.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Closed(Closure::Notice) => "closed-notice".to_owned(),
            Self::Closed(Closure::Season { reopens }) => {
                let mut key = "closed-until-".to_owned();
                key.push_str(reopens);
                key
            }
            Self::Flood => "flood".to_owned(),
            Self::LowWater => "low-water".to_owned(),
            Self::HeavyRain => "heavy-rain".to_owned(),
            Self::Thunderstorms => "thunderstorms".to_owned(),
            Self::RoughSea => "rough-sea".to_owned(),
            Self::StrongWind => "strong-wind".to_owned(),
            Self::ChoppySea => "choppy-sea".to_owned(),
            Self::CoolWater => "cool-water".to_owned(),
            Self::TopSeason => "top-season".to_owned(),
            Self::NothingKeepable => "nothing-keepable".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Dawn,
    Dusk,
    Night,
    AllDay,
}

impl WindowKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dawn => "Dawn",
            Self::Dusk => "Dusk",
            Self::Night => "Night",
            Self::AllDay => "All day",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub kind: WindowKind,
    /// A clock span (`06:25 - 08:55`) or a plain qualifier.
    pub span: String,
}

#[derive(Clone, Debug)]
pub struct SpeciesScore {
    pub entry: &'static SpotSpecies,
    /// Raw species score, 0.0 to 1.4.
    pub score: f64,
    /// In a closed season today.
    pub closed: bool,
    /// Fishable but not keepable today.
    pub release_only: bool,
}

#[derive(Clone, Debug)]
pub struct Evaluation {
    /// The index itself, 0-97.
    pub score: u32,
    pub closure: Option<Closure>,
    pub water_temp: Option<f64>,
    /// The water temperature is the measured sea surface, not an estimate.
    pub water_measured: bool,
    /// Highest wave of the day, m. Sea spots only.
    pub wave: Option<f64>,
    /// Estimated clarity, 0.0 clear to 1.0 muddy.
    pub turbidity: f64,
    /// River discharge, m³/s.
    pub flow: Option<f64>,
    /// Discharge against the median of the weeks before, 1.0 = normal.
    pub flow_ratio: Option<f64>,
    /// Rain in the three days before the scored day, mm.
    pub rain_72h: f64,
    /// Pressure change against the day before, hPa.
    pub pressure_delta: Option<f64>,
    pub moon: Moon,
    pub season: Season,
    /// Species ranked best first.
    pub species: Vec<SpeciesScore>,
    pub modifiers: Vec<Modifier>,
    pub windows: Vec<Window>,
}

impl Evaluation {
    /// The word under the gauge.
    #[must_use]
    pub fn label(&self) -> &'static str {
        if self.closure.is_some() {
            "Closed"
        } else {
            label(self.score)
        }
    }
}

/// Qualitative label for an index value. Thresholds are the source engine's.
#[must_use]
pub fn label(score: u32) -> &'static str {
    match score {
        78.. => "Excellent",
        62..=77 => "Very good",
        46..=61 => "Good",
        30..=45 => "Fair",
        16..=29 => "Poor",
        _ => "Avoid",
    }
}

/// The source engine's three colour bands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Good,
    Fair,
    Poor,
}

#[must_use]
pub fn band(score: u32) -> Band {
    match score {
        62.. => Band::Good,
        30..=61 => Band::Fair,
        _ => Band::Poor,
    }
}

/// What the flow series says about one day.
#[derive(Clone, Copy, Debug, Default)]
struct FlowReading {
    flow: Option<f64>,
    ratio: Option<f64>,
}

fn flow_reading(kind: WaterKind, flow: Option<&Flow>, date: &str) -> FlowReading {
    let Some(series) = flow else {
        return FlowReading::default();
    };
    let Some(j) = series.days.iter().position(|(d, _)| d == date) else {
        return FlowReading::default();
    };
    let Some(today) = series.days[j].1 else {
        return FlowReading::default();
    };
    if today < 0.02 {
        return FlowReading::default();
    }
    let before: Vec<f64> = series.days[..j.max(1)]
        .iter()
        .filter_map(|(_, q)| *q)
        .collect();
    let floor = kind.flow_floor();
    let ratio = (!before.is_empty()).then(|| (today + floor) / (median(&before) + floor));
    FlowReading {
        flow: Some(today),
        ratio,
    }
}

/// The day's closure, if any (`chiusura()`).
fn closure(spot: &Spot, date: &str, days: i64, year: i64) -> Option<Closure> {
    let noticed = spot.notices.iter().any(|n| {
        n.from.is_none_or(|from| date >= from) && n.to.is_none_or(|to| date <= to)
    });
    if noticed {
        return Some(Closure::Notice);
    }
    let may = match spot.winter_closure {
        WinterClosure::None => return None,
        WinterClosure::March => false,
        WinterClosure::May => true,
    };
    trout_season_reopens(days, year, may).map(|reopens| Closure::Season {
        reopens: iso_date(reopens),
    })
}

/// Everything about the day that does not depend on the species.
struct Conditions {
    water: Option<f64>,
    measured: bool,
    wave: Option<f64>,
    t_max: Option<f64>,
    t_min: Option<f64>,
    rain: f64,
    cloud: Option<f64>,
    wind: f64,
    pressure_delta: Option<f64>,
    rain_72h: f64,
    flow: FlowReading,
    turbidity: f64,
}

fn conditions(spot: &Spot, forecast: &Forecast, flow: Option<&Flow>, i: usize) -> Conditions {
    let days = &forecast.days;
    let day = &days[i];
    let air: Vec<Option<f64>> = days[..=i]
        .iter()
        .map(|d| {
            if forecast.has_mean {
                d.t_mean
            } else {
                match (d.t_max, d.t_min) {
                    (Some(max), Some(min)) => Some(f64::midpoint(max, min)),
                    _ => None,
                }
            }
        })
        .collect();

    let sea = spot.kind == WaterKind::Sea;
    let sea_temp = if sea { day.sea_temp } else { None };
    let wave = if sea { day.wave } else { None };
    let water = sea_temp.or_else(|| water_temperature(spot.kind, &air, forecast.elevation));

    let rain = day.precipitation.unwrap_or(0.0);
    let pressure_delta = match (day.pressure, i.checked_sub(1).and_then(|k| days[k].pressure)) {
        (Some(now), Some(before)) => Some(now - before),
        _ => None,
    };
    let rain_72h = [1, 2, 3].iter().fold(0.0, |sum, k| {
        sum + i
            .checked_sub(*k)
            .and_then(|n| days[n].precipitation)
            .unwrap_or(0.0)
    });
    let flow = flow_reading(spot.kind, flow, &day.date);

    let mut turbidity = clamp(rain_72h / 35.0, 0.0, 0.55);
    turbidity += clamp(rain / 25.0, 0.0, 0.25);
    if let Some(ratio) = flow.ratio {
        turbidity += clamp((ratio - 1.15) / 2.2, 0.0, 0.45);
    }
    if spot.kind.settles() {
        turbidity *= 0.6;
    }

    Conditions {
        water,
        measured: sea_temp.is_some(),
        wave,
        t_max: day.t_max,
        t_min: day.t_min,
        rain,
        cloud: day.cloud_cover,
        wind: day.wind.unwrap_or(0.0),
        pressure_delta,
        rain_72h,
        flow,
        turbidity: clamp(turbidity, 0.0, 1.0),
    }
}

/// The date parts the species scoring needs.
#[derive(Clone, Copy)]
struct When {
    days: i64,
    year: i64,
    month: u32,
    day: u32,
}

fn score_one(
    spot: &Spot,
    entry: &'static SpotSpecies,
    c: &Conditions,
    moon: &Moon,
    when: When,
) -> SpeciesScore {
    let sp = entry.info();
    let in_closed = if sp.salmonid_closure {
        trout_season_reopens(when.days, when.year, false).is_some()
    } else {
        sp.closed.iter().any(|w| in_window(when.month, when.day, *w))
    };
    let locally_closed = entry
        .local_closed
        .is_some_and(|w| in_window(when.month, when.day, w));
    let closed = in_closed || locally_closed;
    let release_only = closed || entry.always_release;

    let month0 = usize::try_from(when.month - 1).unwrap_or(0);
    let f_season = seasonal(&sp.months, when.year, month0, when.day);
    let f_temp = bell(c.water, sp.t_opt, sp.t_live);

    let mut f_flow = 1.0;
    if let Some(ratio) = c.flow.ratio {
        let above = ratio - 1.0;
        f_flow = 1.0 + sp.flow * clamp(above, -0.8, 1.2) * 0.35;
        if ratio > 2.6 {
            f_flow *= 0.45;
        } else if ratio > 1.8 {
            f_flow *= 0.75;
        } else if ratio < 0.45 {
            f_flow *= 0.82;
        }
        f_flow = clamp(f_flow, 0.2, 1.3);
    }

    let f_turbid = clamp(1.0 + sp.turbid * (c.turbidity - 0.32) * 1.25, 0.35, 1.25);

    let mut f_pressure = 1.0;
    if let Some(delta) = c.pressure_delta {
        let drop = clamp(-delta / 6.0, -1.0, 1.0);
        f_pressure = 1.0 + sp.pressure * drop * 0.28;
        if delta < -8.0 {
            f_pressure *= 0.88;
        }
        f_pressure = clamp(f_pressure, 0.7, 1.25);
    }

    let mut f_light = 1.0;
    if let Some(cloud) = c.cloud {
        let cover = cloud / 100.0;
        match sp.light {
            Light::Dusk | Light::Night | Light::Dawn => f_light = 0.86 + 0.28 * cover,
            Light::Day => f_light = 1.1 - 0.25 * cover,
            Light::Any => {}
        }
    }

    let f_moon = if sp.light == Light::Night {
        0.9 + 0.22 * moon.illumination
    } else {
        1.0
    };

    let mut f_swing = 1.0;
    if let (Some(max), Some(min)) = (c.t_max, c.t_min)
        && spot.kind != WaterKind::Sea
        && max - min > 16.0
        && spot.kind.standing()
    {
        f_swing = 0.93;
    }

    let mut s = f_season * f_temp * f_flow * f_turbid * f_pressure * f_light * f_moon * f_swing;
    if entry.rare {
        s *= 0.42;
    }
    if release_only {
        s *= 0.34;
    }
    SpeciesScore {
        entry,
        score: clamp(s, 0.0, 1.4),
        closed,
        release_only,
    }
}

fn windows(day: &Day, species: &[SpeciesScore], cloud: Option<f64>) -> Vec<Window> {
    let shifted = |clock: &str, minutes: i64| shift_clock(clock, minutes).unwrap_or_else(|| "-".to_owned());
    let span = |from: String, to: String| {
        let mut s = from;
        s.push_str(" - ");
        s.push_str(&to);
        s
    };
    let mut out = Vec::new();
    if !day.sunrise.is_empty() {
        out.push(Window {
            kind: WindowKind::Dawn,
            span: span(shifted(&day.sunrise, -30), shifted(&day.sunrise, 120)),
        });
    }
    if !day.sunset.is_empty() {
        out.push(Window {
            kind: WindowKind::Dusk,
            span: span(shifted(&day.sunset, -120), shifted(&day.sunset, 40)),
        });
    }
    if species
        .iter()
        .any(|s| s.entry.info().light == Light::Night && !s.release_only)
    {
        out.push(Window {
            kind: WindowKind::Night,
            span: span(shifted(&day.sunset, 60), "24:00".to_owned()),
        });
    }
    if cloud.is_some_and(|c| c > 65.0) {
        out.push(Window {
            kind: WindowKind::AllDay,
            span: "overcast".to_owned(),
        });
    }
    out
}

/// The spot modifiers, in the order of the source, on top of the species'
/// `base` score.
fn modified(
    spot: &Spot,
    day: &Day,
    c: &Conditions,
    season: Season,
    keepable: bool,
    base: f64,
) -> (f64, Vec<Modifier>) {
    let mut base = base;
    let mut modifiers = Vec::new();

    if let Some(ratio) = c.flow.ratio {
        if ratio > 2.6 {
            base *= 0.55;
            modifiers.push(Modifier::Flood);
        } else if ratio < 0.4 && spot.kind != WaterKind::Canal {
            base *= 0.8;
            modifiers.push(Modifier::LowWater);
        }
    }
    if c.rain > 18.0 {
        base *= 0.82;
        modifiers.push(Modifier::HeavyRain);
    }
    if matches!(day.weather_code, Some(95 | 96 | 99)) {
        base *= 0.7;
        modifiers.push(Modifier::Thunderstorms);
    }

    let rough = c.wave.is_some_and(|w| w >= ROUGH_WAVE);
    if c.wind > 38.0 || rough {
        base *= if spot.kind.open() { 0.72 } else { 0.9 };
        modifiers.push(if rough {
            Modifier::RoughSea
        } else {
            Modifier::StrongWind
        });
    } else if spot.kind == WaterKind::Sea
        && match c.wave {
            Some(w) => w >= CHOPPY_WAVE.0 && w < CHOPPY_WAVE.1,
            None => c.wind >= 18.0 && c.wind <= 36.0,
        }
    {
        base *= 1.12;
        modifiers.push(Modifier::ChoppySea);
    }

    if spot.category == Category::D
        && c.water.is_some_and(|t| t <= 16.0)
        && c.t_max.is_some_and(|t| t >= 30.0)
    {
        base *= 1.14;
        modifiers.push(Modifier::CoolWater);
    }
    if spot.top_seasons & season.bit() != 0 {
        base *= 1.08;
        modifiers.push(Modifier::TopSeason);
    }
    if !keepable {
        base *= 0.6;
        modifiers.push(Modifier::NothingKeepable);
    }

    (base, modifiers)
}

/// Score `spot` for the forecast day at `index`. `flow` is the river
/// discharge, when the spot has one and it arrived.
///
/// `None` when `index` is out of range or the day's date is unreadable.
#[must_use]
pub fn evaluate(
    spot: &Spot,
    forecast: &Forecast,
    flow: Option<&Flow>,
    index: usize,
) -> Option<Evaluation> {
    let day = forecast.days.get(index)?;
    let (year, month, day_of_month) = parse_date(&day.date)?;
    let days = days_from_civil(year, month, day_of_month)?;
    let when = When {
        days,
        year,
        month,
        day: day_of_month,
    };

    let c = conditions(spot, forecast, flow, index);
    let season = season_of(month - 1);
    let moon = moon_phase(days, year);

    let mut species: Vec<SpeciesScore> = spot
        .species
        .iter()
        .map(|entry| score_one(spot, entry, &c, &moon, when))
        .collect();
    // Stable, like Array.prototype.sort: ties keep the order of the data.
    species.sort_by(|a, b| b.score.total_cmp(&a.score));

    let keepable = species.iter().any(|s| !s.release_only);
    let top = species.first().map_or(0.0, |s| s.score);
    let three: Vec<f64> = species.iter().take(3).map(|s| s.score).collect();
    let base = top * 0.62 + mean(&three) * 0.38;
    let (mut base, mut modifiers) = modified(spot, day, &c, season, keepable, base);

    let closure = closure(spot, &day.date, days, year);
    if let Some(closed) = &closure {
        base = 0.0;
        modifiers.insert(0, Modifier::Closed(closed.clone()));
    }

    let compressed = clamp(1.0 - (-1.55 * base.max(0.0)).exp(), 0.0, 0.97);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the compressed index is clamped to 0..=97"
    )]
    let score = js_round(100.0 * compressed) as u32;

    let windows = if closure.is_some() {
        Vec::new()
    } else {
        windows(day, &species, c.cloud)
    };

    Some(Evaluation {
        score,
        closure,
        water_temp: c.water,
        water_measured: c.measured,
        wave: c.wave,
        turbidity: c.turbidity,
        flow: c.flow.flow,
        flow_ratio: c.flow.ratio,
        rain_72h: c.rain_72h,
        pressure_delta: c.pressure_delta,
        moon,
        season,
        species,
        modifiers,
        windows,
    })
}

/// The day `n` days after a `YYYY-MM-DD` date, for the tests and the faces.
#[must_use]
pub fn date_after(date: &str, n: i64) -> Option<String> {
    let (y, m, d) = parse_date(date)?;
    Some(iso_date(add_days(days_from_civil(y, m, d)?, n)))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod parity;
