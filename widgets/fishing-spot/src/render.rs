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

//! Size dispatch. Each variant is its own layout rather than one layout that
//! scales: font sizes stay put across the bands and the narrower faces drop
//! fields instead of shrinking them.

pub mod common;
pub mod full;
pub mod large;
pub mod medium;
pub mod small;

use crate::view::View;

use bmc_wasm_sdk::*;

#[must_use]
pub fn view(view: &View, size: WidgetSize) -> Node {
    match size.variant {
        SizeVariant::Full => full::full(view),
        SizeVariant::Large => large::large(view),
        SizeVariant::Medium => medium::medium(view),
        SizeVariant::Small => small::small(view),
    }
}

#[must_use]
pub fn message(content: &str) -> Node {
    common::message(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Day, Flow, Forecast, days_from_civil, iso_date};
    use crate::spot::{SPOTS, Spot, resolve};

    const SIZES: [SizeVariant; 4] = [
        SizeVariant::Full,
        SizeVariant::Large,
        SizeVariant::Medium,
        SizeVariant::Small,
    ];

    fn size(variant: SizeVariant) -> WidgetSize {
        WidgetSize {
            variant,
            width: variant.width(),
            height: variant.height(),
        }
    }

    fn day(date: &str) -> Day {
        Day {
            date: date.to_owned(),
            weather_code: Some(95),
            t_max: Some(22.4),
            t_min: Some(17.7),
            t_mean: Some(20.4),
            precipitation: Some(2.7),
            wind: Some(22.9),
            pressure: Some(1013.2),
            cloud_cover: Some(86.0),
            sunrise: "06:55".to_owned(),
            sunset: "19:21".to_owned(),
            sea_temp: Some(21.3),
            wave: Some(0.7),
        }
    }

    /// 14 past days, today (2026-09-17) and 6 days ahead.
    fn forecast() -> Forecast {
        let start = days_from_civil(2026, 9, 3).unwrap_or(0);
        Forecast {
            elevation: 12.0,
            has_mean: true,
            days: (0..21).map(|n| day(&iso_date(start + n))).collect(),
            today_index: 14,
        }
    }

    fn flow(f: &Forecast) -> Flow {
        Flow {
            days: f.days.iter().map(|d| (d.date.clone(), Some(12.5))).collect(),
        }
    }

    fn spot(address: &str) -> &'static Spot {
        resolve(address).expect("BUG: the catalogue has this spot")
    }

    /// The layouts are built from fetched values, so the guard that matters is
    /// that constructing and serialising the tree never panics, which is what
    /// would take the widget down on the device. Every spot of the catalogue
    /// goes through every face: names, species counts and readings differ.
    #[test]
    fn every_spot_builds_every_size_into_a_tree_that_serialises() {
        let forecast = forecast();
        let flow = flow(&forecast);
        for s in SPOTS {
            let built = View::build(s, &forecast, Some(&flow), 0).expect("BUG: today is in range");
            for variant in SIZES {
                let bytes = encode(&view(&built, size(variant)));
                assert!(!bytes.is_empty(), "{} {variant:?} serialised to an empty tree", s.id);
            }
        }
    }

    /// A forecast whose every reading is missing is the shape a bad upstream
    /// day takes. It must still render, with placeholders rather than a panic.
    #[test]
    fn every_size_survives_a_forecast_with_no_readings() {
        let forecast = Forecast {
            elevation: 0.0,
            has_mean: true,
            days: vec![Day {
                date: "2026-09-17".to_owned(),
                ..Day::default()
            }],
            today_index: 0,
        };
        for address in ["panaro-a-vignola", "portocanale-di-rimini"] {
            let built = View::build(spot(address), &forecast, None, 0)
                .expect("BUG: a dated day always builds");
            for variant in SIZES {
                assert!(!encode(&view(&built, size(variant))).is_empty());
            }
        }
    }

    #[test]
    fn tomorrow_renders_the_same_four_faces() {
        let forecast = forecast();
        let built = View::build(spot("panaro-a-vignola"), &forecast, None, 1)
            .expect("BUG: tomorrow is in range");
        assert_eq!(built.day.date, "2026-09-18");
        for variant in SIZES {
            assert!(!encode(&view(&built, size(variant))).is_empty());
        }
    }

    #[test]
    fn the_message_screens_serialise() {
        for content in [
            crate::display::LOADING,
            crate::display::CANNOT_LOAD,
            crate::display::NO_SPOT,
            crate::display::UNKNOWN_SPOT,
        ] {
            assert!(!encode(&message(content)).is_empty());
        }
    }
}
