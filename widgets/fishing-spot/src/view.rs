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

//! Everything one frame needs, assembled once from the replies.
//!
//! Keeping this between the engine and the render modules means each layout
//! reads prepared values instead of re-scoring the week, and the assembly
//! stays unit-testable on the host.

use crate::engine::{self, Evaluation, Modifier};
use crate::model::{Day, Flow, Forecast};
use crate::spot::{Spot, WaterKind};

pub struct View {
    pub spot: &'static Spot,
    /// The day being described.
    pub day: Day,
    pub evaluation: Evaluation,
}

/// The reading that tells most about this water, after the weather.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaterReading {
    /// Discharge against the median of the weeks before, 1.0 = normal.
    Flow(f64),
    /// Highest wave of the day, m.
    Waves(f64),
    /// Estimated turbidity, 0.0 clear to 1.0 muddy.
    Clarity(f64),
}

impl View {
    /// Score the spot for the day `offset` days after today.
    ///
    /// `None` when the forecast does not reach that day, which lets the
    /// caller fall back to today rather than render a blank panel.
    #[must_use]
    pub fn build(
        spot: &'static Spot,
        forecast: &Forecast,
        flow: Option<&Flow>,
        offset: usize,
    ) -> Option<Self> {
        let index = forecast.index_of(offset)?;
        let day = forecast.days.get(index)?.clone();
        let evaluation = engine::evaluate(spot, forecast, flow, index)?;
        Some(View {
            spot,
            day,
            evaluation,
        })
    }

    /// Species worth naming: the best few that are keepable today, falling
    /// back to the best overall when the rules leave nothing to keep.
    #[must_use]
    pub fn headline_species(&self, count: usize) -> Vec<&engine::SpeciesScore> {
        let keepable: Vec<&engine::SpeciesScore> = self
            .evaluation
            .species
            .iter()
            .filter(|s| !s.release_only)
            .take(count)
            .collect();
        if keepable.is_empty() {
            self.evaluation.species.iter().take(count).collect()
        } else {
            keepable
        }
    }

    /// The best fishing window to lead with, if the day has one.
    #[must_use]
    pub fn best_window(&self) -> Option<&engine::Window> {
        self.evaluation.windows.first()
    }

    /// The warnings worth showing: the modifiers that cost the day score.
    ///
    /// A positive modifier still moves the index; it is just not something
    /// to act on, so it stays out of the layout.
    #[must_use]
    pub fn warnings(&self) -> Vec<&Modifier> {
        self.evaluation
            .modifiers
            .iter()
            .filter(|m| !m.positive())
            .collect()
    }

    /// Flow on a river with a discharge reading, waves on the sea with a
    /// wave reading, the estimated clarity everywhere else.
    #[must_use]
    pub fn water_reading(&self) -> WaterReading {
        let e = &self.evaluation;
        match (self.spot.kind, e.flow_ratio, e.wave) {
            (WaterKind::River | WaterKind::Torrent, Some(ratio), _) => WaterReading::Flow(ratio),
            (WaterKind::Sea, _, Some(wave)) => WaterReading::Waves(wave),
            _ => WaterReading::Clarity(e.turbidity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{days_from_civil, iso_date};
    use crate::spot::resolve;

    fn spot(address: &str) -> &'static Spot {
        resolve(address).expect("BUG: the catalogue has this spot")
    }

    fn day(date: &str) -> Day {
        Day {
            date: date.to_string(),
            weather_code: Some(2),
            t_max: Some(27.0),
            t_min: Some(17.0),
            t_mean: Some(22.0),
            precipitation: Some(0.0),
            wind: Some(12.0),
            pressure: Some(1015.0),
            cloud_cover: Some(30.0),
            sunrise: "06:55".to_string(),
            sunset: "19:21".to_string(),
            ..Day::default()
        }
    }

    /// 14 past days, today (2026-09-17) and 6 days ahead.
    fn forecast() -> Forecast {
        let start = days_from_civil(2026, 9, 3).unwrap_or(0);
        Forecast {
            elevation: 40.0,
            has_mean: true,
            days: (0..21).map(|n| day(&iso_date(start + n))).collect(),
            today_index: 14,
        }
    }

    #[test]
    fn building_for_today_and_tomorrow_describes_those_days() {
        let s = spot("panaro-a-vignola");
        let v = View::build(s, &forecast(), None, 0).expect("BUG: today is in range");
        assert_eq!(v.day.date, "2026-09-17");
        assert!(v.evaluation.score > 0);
        let v = View::build(s, &forecast(), None, 1).expect("BUG: tomorrow is in range");
        assert_eq!(v.day.date, "2026-09-18");
    }

    #[test]
    fn a_day_beyond_the_forecast_declines_to_build() {
        assert!(View::build(spot("panaro-a-vignola"), &forecast(), None, 7).is_none());
    }

    #[test]
    fn warnings_carry_the_costs_and_drop_the_bonuses() {
        let mut f = forecast();
        f.days[14].weather_code = Some(95);
        let v = View::build(spot("panaro-a-vignola"), &f, None, 0).expect("BUG: today is in range");
        assert!(v.warnings().contains(&&Modifier::Thunderstorms));
        // Autumn is a recommended season here: it moves the index, but it
        // is not something to warn about.
        assert!(v.evaluation.modifiers.contains(&Modifier::TopSeason));
        assert!(!v.warnings().contains(&&Modifier::TopSeason));
    }

    #[test]
    fn headline_species_prefer_the_keepable_ones() {
        let v = View::build(spot("panaro-a-vignola"), &forecast(), None, 0)
            .expect("BUG: today is in range");
        let named = v.headline_species(3);
        assert!(!named.is_empty());
        assert!(named.len() <= 3);
        assert!(named.iter().all(|s| !s.release_only));
        for pair in named.windows(2) {
            assert!(pair[0].score >= pair[1].score);
        }
    }

    #[test]
    fn the_best_window_leads_with_dawn_when_there_is_a_sunrise() {
        let v = View::build(spot("panaro-a-vignola"), &forecast(), None, 0)
            .expect("BUG: today is in range");
        assert_eq!(v.best_window().map(|w| w.kind), Some(engine::WindowKind::Dawn));
    }

    #[test]
    fn the_water_reading_fits_the_water() {
        let f = forecast();
        let river = spot("panaro-a-vignola");
        let flow = Flow {
            days: f.days.iter().map(|d| (d.date.clone(), Some(5.0))).collect(),
        };
        let v = View::build(river, &f, Some(&flow), 0).expect("BUG: today is in range");
        assert!(matches!(v.water_reading(), WaterReading::Flow(_)));
        let v = View::build(river, &f, None, 0).expect("BUG: today is in range");
        assert!(matches!(v.water_reading(), WaterReading::Clarity(_)));

        let mut sea = forecast();
        sea.days[14].wave = Some(0.8);
        sea.days[14].sea_temp = Some(19.4);
        let v = View::build(spot("portocanale-di-rimini"), &sea, None, 0)
            .expect("BUG: today is in range");
        assert_eq!(v.water_reading(), WaterReading::Waves(0.8));
        assert!(v.evaluation.water_measured);
    }
}
