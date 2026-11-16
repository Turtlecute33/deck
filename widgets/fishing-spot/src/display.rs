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

//! Every string the widget puts on screen.
//!
//! Numbers go through the SDK format macros so the deck's own number,
//! temperature and speed settings apply; this module only composes sign,
//! unit and wording around them. Anything unavailable renders as
//! [`NOT_AVAILABLE`] rather than a zero.
//!
//! Tile readings are **bare numbers** and the unit rides in the tile's
//! caption instead. One reading — the pressure — carries a unit long enough
//! ("1 013 mbar") to overrun a quarter-width tile and wrap onto a second
//! line, which makes its row taller than the row beneath it and breaks the
//! grid. Splitting the unit off keeps every value short by construction
//! rather than by choosing a small enough font.

use bmc_wasm_sdk::*;

use crate::engine::{Closure, Modifier};
use crate::model::{month_short, parse_date, weekday_short};

pub const NOT_AVAILABLE: &str = "N/A";
pub const LOADING: &str = "Reading the water...";
pub const CANNOT_LOAD: &str = "Forecast unavailable";
pub const NO_SPOT: &str = "Add a spot in the widget settings";
pub const UNKNOWN_SPOT: &str = "Unknown spot: check the address";

// ── Units ───────────────────────────────────────────────────────────────

/// The temperature unit the deck is set to.
#[must_use]
pub fn temperature_unit() -> &'static str {
    if system::current().temperature_unit() == Some(system::TemperatureUnit::Fahrenheit) {
        "°F"
    } else {
        "°C"
    }
}

/// The speed unit the deck is set to.
#[must_use]
pub fn speed_unit() -> &'static str {
    if system::current().unit_system() == Some(system::UnitSystem::Imperial) {
        "mph"
    } else {
        "km/h"
    }
}

/// Kilometres per hour in the deck's preferred unit.
fn to_preferred_speed(kph: f64) -> f64 {
    if system::current().unit_system() == Some(system::UnitSystem::Imperial) {
        kph * 0.621_371
    } else {
        kph
    }
}

// ── Bare readings, for tiles whose caption carries the unit ─────────────

/// Temperature as a number only, converted to the deck's unit.
///
/// The SDK's `bare` arm yields the degree-only form ("21,6°"); the glyph
/// comes off here because the caption already says which scale it is.
#[must_use]
pub fn temperature_number(value: f64, decimals: u32) -> String {
    let formatted = if decimals == 0 {
        format_temperature!(value, 0, bare)
    } else {
        format_temperature!(value, 1, bare)
    };
    formatted.trim_end_matches('°').to_owned()
}

/// The day's air range as two bare numbers, e.g. `23 / 18`.
#[must_use]
pub fn air_range(max: Option<f64>, min: Option<f64>) -> String {
    match (max, min) {
        (Some(max), Some(min)) => fmt!(
            "{} / {}",
            temperature_number(max, 0).as_str(),
            temperature_number(min, 0).as_str()
        ),
        (Some(max), None) => temperature_number(max, 0),
        (None, Some(min)) => temperature_number(min, 0),
        (None, None) => NOT_AVAILABLE.to_owned(),
    }
}

/// Water temperature as a bare number, one decimal.
#[must_use]
pub fn water_number(value: Option<f64>) -> String {
    value.map_or_else(|| NOT_AVAILABLE.to_owned(), |v| temperature_number(v, 1))
}

/// Wind speed as a bare number in the deck's unit.
///
/// How hard it blows matters more than where from, so no bearing is shown.
#[must_use]
pub fn wind_number(kph: Option<f64>) -> String {
    kph.map_or_else(
        || NOT_AVAILABLE.to_owned(),
        |v| format_number!(to_preferred_speed(v), 0),
    )
}

/// Pressure as a bare number. Millibar and the hectopascal the provider
/// sends are the same quantity; the caption spells it the way the barometers
/// here do.
#[must_use]
pub fn pressure_number(millibar: Option<f64>) -> String {
    millibar.map_or_else(|| NOT_AVAILABLE.to_owned(), |v| format_number!(v, 0))
}

/// Rainfall as a bare number, one decimal.
#[must_use]
pub fn rain_number(mm: Option<f64>) -> String {
    mm.map_or_else(|| NOT_AVAILABLE.to_owned(), |v| format_number!(v, 1))
}

/// A whole-percent value, bare.
#[must_use]
pub fn percent_number(value: Option<f64>) -> String {
    value.map_or_else(|| NOT_AVAILABLE.to_owned(), |v| format_number!(v, 0))
}

/// Water clarity as a bare percentage, from the modelled turbidity.
///
/// The engine models how *dirty* the water is; the tile is labelled CLARITY,
/// so the reading is inverted to match its own label. Showing the turbidity
/// there would read as "8" for water that is in fact almost clean.
#[must_use]
pub fn clarity_number(turbidity: f64) -> String {
    percent_number(Some((1.0 - turbidity.clamp(0.0, 1.0)) * 100.0))
}

/// River discharge against its recent median, as a signed whole percent:
/// `+35` is 35% above normal, `-20` is 20% below.
#[must_use]
pub fn flow_number(ratio: f64) -> String {
    let percent = (ratio - 1.0) * 100.0;
    let sign = if percent.round() > 0.0 {
        "+"
    } else if percent.round() < 0.0 {
        "-"
    } else {
        ""
    };
    fmt!("{}{}", sign, format_number!(percent.abs(), 0).as_str())
}

/// Wave height in metres, one decimal.
#[must_use]
pub fn wave_number(metres: f64) -> String {
    format_number!(metres, 1)
}

// ── Readings that carry their own unit, for the narrow faces ────────────

/// Temperature with its unit, for label/value rows rather than tiles.
#[must_use]
pub fn temperature(value: f64) -> String {
    format_temperature!(value, 1)
}

// ── Words and dates ─────────────────────────────────────────────────────

/// A time span, as shown in the fishing-window rows.
#[must_use]
pub fn span(from: &str, to: &str) -> String {
    fmt!("{} - {}", from, to)
}

/// Moon phase name with its illuminated fraction.
#[must_use]
pub fn moon(name: &str, illumination: f64) -> String {
    fmt!(
        "{} {}%",
        name,
        format_number!(illumination * 100.0, 0).as_str()
    )
}

/// `Thu 17 Sep` for a `YYYY-MM-DD` date.
#[must_use]
pub fn date_line(date: &str) -> String {
    let (Some(weekday), Some(month), Some((_, _, day))) =
        (weekday_short(date), month_short(date), parse_date(date))
    else {
        return date.to_owned();
    };
    fmt!("{} {} {}", weekday, format_number!(day, 0).as_str(), month)
}

/// The minimum size and bag limit a species carries here, or an empty
/// string when the rules set neither.
#[must_use]
pub fn species_rule(species: &crate::spot::SpotSpecies) -> String {
    match (species.min_size_cm, species.daily_limit.filter(|n| *n > 0)) {
        (Some(size), Some(limit)) => fmt!(
            "min {} cm, max {}/day",
            format_number!(size, 0).as_str(),
            format_number!(limit, 0).as_str()
        ),
        (Some(size), None) => fmt!("min {} cm", format_number!(size, 0).as_str()),
        (None, Some(limit)) => fmt!("max {}/day", format_number!(limit, 0).as_str()),
        (None, None) => String::new(),
    }
}

/// A warning, as the header shows it.
#[must_use]
pub fn modifier(m: &Modifier) -> String {
    match m {
        Modifier::Closed(Closure::Notice) => "Fishing banned here".to_owned(),
        Modifier::Closed(Closure::Season { reopens }) => {
            fmt!("Closed until {}", date_line(reopens).as_str())
        }
        Modifier::Flood => "River in flood".to_owned(),
        Modifier::LowWater => "Very low water".to_owned(),
        Modifier::HeavyRain => "Heavy rain forecast".to_owned(),
        Modifier::Thunderstorms => "Thunderstorms".to_owned(),
        Modifier::RoughSea => "Very rough sea".to_owned(),
        Modifier::StrongWind => "Strong wind".to_owned(),
        Modifier::ChoppySea => "Choppy sea: bass active".to_owned(),
        Modifier::CoolWater => "Cool water on a hot day".to_owned(),
        Modifier::TopSeason => "Recommended season here".to_owned(),
        Modifier::NothingKeepable => "Nothing keepable today".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The widest a tile reading is allowed to get.
    ///
    /// A full-size tile is 193px inside its padding, and at 40px type a digit
    /// runs about 21px — so ten glyphs is the point where a reading stops
    /// fitting. Nine is the real worst case ("-19 / -18", a January air
    /// range), and this pins it there so a future format change that pushes
    /// past it fails here rather than on the glass. The tile value is set to
    /// ellipsis rather than wrap, so exceeding this clips a reading instead of
    /// pulling the grid apart.
    const MAX_VALUE_CHARS: usize = 9;

    #[test]
    fn a_missing_reading_reads_as_unavailable_not_as_zero() {
        assert_eq!(pressure_number(None), NOT_AVAILABLE);
        assert_eq!(rain_number(None), NOT_AVAILABLE);
        assert_eq!(percent_number(None), NOT_AVAILABLE);
        assert_eq!(wind_number(None), NOT_AVAILABLE);
        assert_eq!(water_number(None), NOT_AVAILABLE);
        assert_eq!(air_range(None, None), NOT_AVAILABLE);
    }

    #[test]
    fn tile_readings_carry_no_unit() {
        // The unit belongs to the caption. A reading that smuggles one back in
        // is what overruns the tile.
        for reading in [
            water_number(Some(21.6)),
            air_range(Some(23.0), Some(18.0)),
            wind_number(Some(22.9)),
            pressure_number(Some(1013.2)),
            rain_number(Some(3.3)),
            percent_number(Some(79.0)),
            clarity_number(0.08),
        ] {
            for unit in ["°", "C", "F", "mbar", "hPa", "km/h", "mph", "mm", "%"] {
                assert!(
                    !reading.contains(unit),
                    "{reading} still carries the unit {unit}"
                );
            }
        }
    }

    #[test]
    fn the_widest_plausible_reading_still_fits_a_tile() {
        // The pressure is the long one, and it is what forced this split.
        for reading in [
            water_number(Some(-9.5)),
            air_range(Some(-19.0), Some(-18.0)),
            wind_number(Some(123.0)),
            pressure_number(Some(1013.2)),
            rain_number(Some(123.4)),
            percent_number(Some(100.0)),
            clarity_number(0.0),
        ] {
            assert!(
                reading.chars().count() <= MAX_VALUE_CHARS,
                "{reading} is {} characters, over the {MAX_VALUE_CHARS} a tile holds",
                reading.chars().count()
            );
        }
    }

    #[test]
    fn units_default_to_metric() {
        // The host snapshot is empty in tests, so these are the fallbacks.
        assert_eq!(temperature_unit(), "°C");
        assert_eq!(speed_unit(), "km/h");
    }

    #[test]
    fn clarity_is_the_inverse_of_turbidity() {
        assert!(clarity_number(0.0).starts_with("100"));
        assert!(clarity_number(0.08).starts_with("92"));
        assert!(clarity_number(1.0).starts_with('0'));
    }

    #[test]
    fn clarity_cannot_leave_the_percentage_range() {
        assert!(clarity_number(-0.5).starts_with("100"));
        assert!(clarity_number(2.0).starts_with('0'));
    }

    #[test]
    fn the_date_line_reads_as_a_day_and_falls_back_to_the_raw_value() {
        assert_eq!(date_line("2026-09-17"), "Thu 17 Sep");
        assert_eq!(date_line("rubbish"), "rubbish");
    }

    #[test]
    fn a_half_known_air_range_shows_the_half_it_has() {
        assert!(air_range(Some(27.0), None).contains("27"));
        assert!(air_range(None, Some(17.0)).contains("17"));
    }

    #[test]
    fn species_rules_render_only_the_limits_that_exist() {
        use crate::spot::SpotSpecies;
        let rule = |min_size_cm, daily_limit| {
            species_rule(&SpotSpecies {
                species: 0,
                rare: false,
                always_release: false,
                local_closed: None,
                min_size_cm,
                daily_limit,
            })
        };
        assert_eq!(rule(Some(30), None), "min 30 cm");
        assert_eq!(rule(None, Some(40)), "max 40/day");
        assert_eq!(rule(Some(22), Some(5)), "min 22 cm, max 5/day");
        assert_eq!(rule(None, None), "");
        // A limit of 0 means release only, which the cell says instead.
        assert_eq!(rule(None, Some(0)), "");
    }

    #[test]
    fn flow_and_waves_read_as_short_numbers() {
        assert_eq!(flow_number(1.35), "+35");
        assert_eq!(flow_number(0.8), "-20");
        assert_eq!(flow_number(1.0), "0");
        assert_eq!(flow_number(1.001), "0");
        assert!(flow_number(13.0).chars().count() <= MAX_VALUE_CHARS);
        assert!(wave_number(0.8).starts_with('0'));
    }

    #[test]
    fn warnings_read_in_english() {
        assert_eq!(modifier(&Modifier::Thunderstorms), "Thunderstorms");
        assert_eq!(
            modifier(&Modifier::Closed(Closure::Season {
                reopens: "2027-03-28".to_owned()
            })),
            "Closed until Sun 28 Mar"
        );
    }

    #[test]
    fn a_span_reads_as_a_range() {
        assert_eq!(span("06:25", "08:55"), "06:25 - 08:55");
    }
}
