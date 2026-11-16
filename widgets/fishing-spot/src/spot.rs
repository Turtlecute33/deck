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

//! The shape of the catalogue, and how a Spot setting finds its spot.
//!
//! The rows themselves are in `catalogue.rs`, generated from the public
//! dovepescare.com data by `tools/catalogue.mjs`.

pub use crate::catalogue::{SPECIES, SPOTS};

/// Water body class, as the source data names it (`tipo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaterKind {
    Torrent,
    River,
    Canal,
    Lake,
    Reservoir,
    Quarry,
    Sea,
}

/// How the water temperature follows the air (`INERZIA` in engine.js).
#[derive(Clone, Copy, Debug)]
pub struct Inertia {
    /// Days of mean air temperature the water follows.
    pub days: usize,
    /// Running water: damping and offset come from the elevation instead.
    pub running: bool,
    pub damping: f64,
    pub offset: f64,
    /// The water never gets warmer than this.
    pub ceiling: f64,
}

impl WaterKind {
    #[must_use]
    pub const fn inertia(self) -> Inertia {
        const fn still(days: usize, damping: f64, offset: f64, ceiling: f64) -> Inertia {
            Inertia {
                days,
                running: false,
                damping,
                offset,
                ceiling,
            }
        }
        const fn running(days: usize, ceiling: f64) -> Inertia {
            Inertia {
                days,
                running: true,
                damping: 0.0,
                offset: 0.0,
                ceiling,
            }
        }
        match self {
            Self::Torrent => running(3, 31.0),
            Self::River => running(5, 31.0),
            Self::Canal => still(3, 0.95, 2.25, 33.0),
            Self::Lake | Self::Reservoir => still(14, 0.85, 1.25, 30.0),
            Self::Quarry => still(9, 0.90, 1.75, 31.0),
            Self::Sea => still(14, 1.00, 2.25, 31.0),
        }
    }

    /// Rivers and torrents: the only waters with a modelled discharge.
    #[must_use]
    pub const fn runs(self) -> bool {
        matches!(self, Self::Torrent | Self::River)
    }

    /// Still waters, where the same rain stains the water less.
    #[must_use]
    pub const fn settles(self) -> bool {
        matches!(self, Self::Canal | Self::Lake | Self::Quarry)
    }

    /// Open waters, where a strong blow costs more of the score.
    #[must_use]
    pub const fn open(self) -> bool {
        matches!(self, Self::Lake | Self::Reservoir | Self::Quarry | Self::Sea)
    }

    /// Standing waters, where a wide day-night swing in the air costs a little.
    #[must_use]
    pub const fn standing(self) -> bool {
        matches!(self, Self::Lake | Self::Reservoir | Self::Quarry | Self::Canal)
    }

    /// Discharge added to both sides of the flow ratio, m³/s, so that two
    /// tiny numbers do not read as a flood (`PORTATA_MINIMA`).
    #[must_use]
    pub const fn flow_floor(self) -> f64 {
        match self {
            Self::Torrent => 0.1,
            _ => 0.5,
        }
    }
}

/// Fishing zone of the regional regulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    A,
    B,
    C,
    /// Trout and grayling waters.
    D,
    Sea,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    Autumn,
}

impl Season {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Winter => "Winter",
            Self::Spring => "Spring",
            Self::Summer => "Summer",
            Self::Autumn => "Autumn",
        }
    }

    /// The bit for this season in [`Spot::top_seasons`].
    #[must_use]
    pub const fn bit(self) -> u8 {
        match self {
            Self::Winter => 1,
            Self::Spring => 2,
            Self::Summer => 4,
            Self::Autumn => 8,
        }
    }
}

/// Month is 0-based, as in the source engine.
#[must_use]
pub const fn season_of(month: u32) -> Season {
    match month {
        0 | 1 | 11 => Season::Winter,
        2..=4 => Season::Spring,
        5..=7 => Season::Summer,
        _ => Season::Autumn,
    }
}

/// Light preference, which decides how cloud cover and the moon move a
/// species' score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    Dawn,
    Dusk,
    Night,
    Day,
    Any,
}

/// A closed season as `[from_month, from_day, to_month, to_day]`, 1-based.
/// It can wrap the year end.
pub type Window = [u32; 4];

#[derive(Debug)]
pub struct Species {
    /// The id in the source data, such as `carpa`.
    pub key: &'static str,
    /// English common name shown on the Deck.
    pub name: &'static str,
    /// Water temperature band of peak activity, °C.
    pub t_opt: (f64, f64),
    /// Water temperature band in which the fish still feeds, °C.
    pub t_live: (f64, f64),
    /// Relative activity per month, January first.
    pub months: [f64; 12],
    pub light: Light,
    /// Preference for stained (+) or clear (-) water.
    pub turbid: f64,
    /// Preference for high (+) or low (-) flow.
    pub flow: f64,
    /// Reaction to falling pressure; positive means it switches on.
    pub pressure: f64,
    /// Closed seasons of the regional regulation.
    pub closed: &'static [Window],
    /// Closed with the trout season (first Sunday of October to the last
    /// Sunday of March) instead of on fixed dates.
    pub salmonid_closure: bool,
}

/// A species as it lives in one spot, with the province rules applied.
#[derive(Debug)]
pub struct SpotSpecies {
    /// Index into [`SPECIES`].
    pub species: usize,
    /// Present, but rarely: not the reason to go.
    pub rare: bool,
    /// Protected, or never keepable in this province: release only, all year.
    pub always_release: bool,
    /// An extra closed season of the province calendar.
    pub local_closed: Option<Window>,
    pub min_size_cm: Option<u32>,
    /// Daily bag limit.
    pub daily_limit: Option<u32>,
}

impl SpotSpecies {
    #[must_use]
    pub fn info(&self) -> &'static Species {
        &SPECIES[self.species]
    }
}

/// A temporary closure of the whole spot, from and to (`YYYY-MM-DD`,
/// inclusive). An open end means no limit on that side.
#[derive(Debug)]
pub struct Notice {
    pub from: Option<&'static str>,
    pub to: Option<&'static str>,
}

/// The winter closure of category D waters, to all species.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinterClosure {
    None,
    /// Opens again on the last Sunday of March.
    March,
    /// Opens again on the first Sunday of May.
    May,
}

#[derive(Debug)]
pub struct Spot {
    /// The id in the source data.
    pub id: &'static str,
    /// The page address on dovepescare.com: `/spot/<slug>/`.
    pub slug: &'static str,
    pub name: &'static str,
    pub kind: WaterKind,
    pub category: Category,
    /// Coordinates as the source data writes them, for the request URLs.
    pub lat: &'static str,
    pub lon: &'static str,
    /// Where to read the river discharge. Rivers and torrents only.
    pub flow_cell: Option<(&'static str, &'static str)>,
    /// Bits of [`Season::bit`]: the seasons the regional guide recommends.
    pub top_seasons: u8,
    pub species: &'static [SpotSpecies],
    pub notices: &'static [Notice],
    pub winter_closure: WinterClosure,
}

/// The spot a Spot setting names, if any.
///
/// Accepts the page address on dovepescare.com, with or without the rest of
/// the URL (`https://dovepescare.com/spot/<slug>/`), or the id of the source
/// data. Case and surrounding spaces do not matter.
#[must_use]
pub fn resolve(setting: &str) -> Option<&'static Spot> {
    let key = setting_key(setting)?;
    SPOTS
        .iter()
        .find(|s| s.slug == key)
        .or_else(|| SPOTS.iter().find(|s| s.id == key))
}

/// The part of a Spot setting that names the spot, lower case.
fn setting_key(setting: &str) -> Option<String> {
    let mut rest = setting.trim().to_ascii_lowercase();
    for scheme in ["https://", "http://"] {
        if let Some(stripped) = rest.strip_prefix(scheme) {
            rest = stripped.to_owned();
        }
    }
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let mut parts = rest.split('/').filter(|p| !p.is_empty()).peekable();
    // A host name has a dot; a spot address never does.
    if parts.peek().is_some_and(|p| p.contains('.')) {
        parts.next();
    }
    if parts.peek() == Some(&"spot") {
        parts.next();
    }
    parts.next().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasons_follow_the_source_engine_month_bands() {
        assert_eq!(season_of(0), Season::Winter);
        assert_eq!(season_of(1), Season::Winter);
        assert_eq!(season_of(2), Season::Spring);
        assert_eq!(season_of(4), Season::Spring);
        assert_eq!(season_of(5), Season::Summer);
        assert_eq!(season_of(7), Season::Summer);
        assert_eq!(season_of(8), Season::Autumn);
        assert_eq!(season_of(10), Season::Autumn);
        assert_eq!(season_of(11), Season::Winter);
    }

    #[test]
    fn a_spot_is_found_by_address_url_or_id() {
        let spot = resolve("panaro-a-vignola").expect("BUG: the catalogue has this spot");
        assert_eq!(spot.id, "mo-panaro-vignola");
        for setting in [
            "  Panaro-a-Vignola ",
            "https://dovepescare.com/spot/panaro-a-vignola/",
            "dovepescare.com/spot/panaro-a-vignola",
            "https://www.dovepescare.com/spot/panaro-a-vignola/?x=1#top",
            "/spot/panaro-a-vignola/",
            "mo-panaro-vignola",
        ] {
            assert_eq!(resolve(setting).map(|s| s.id), Some(spot.id), "{setting}");
        }
    }

    #[test]
    fn an_empty_or_unknown_setting_finds_nothing() {
        for setting in ["", "   ", "https://dovepescare.com/", "no-such-spot", "spot"] {
            assert!(resolve(setting).is_none(), "{setting}");
        }
    }

    #[test]
    fn every_address_and_id_is_unique() {
        for (n, a) in SPOTS.iter().enumerate() {
            for b in &SPOTS[n + 1..] {
                assert_ne!(a.slug, b.slug);
                assert_ne!(a.id, b.id);
            }
            assert_eq!(resolve(a.slug).map(|s| s.id), Some(a.id));
            assert_eq!(resolve(a.id).map(|s| s.id), Some(a.id));
        }
    }

    #[test]
    fn only_running_waters_carry_a_flow_cell() {
        for s in SPOTS {
            assert_eq!(s.flow_cell.is_some(), s.kind.runs(), "{}", s.id);
        }
    }

    #[test]
    fn every_species_carries_a_full_year_and_sane_bands() {
        for s in SPECIES {
            assert!(s.months.iter().all(|m| (0.0..=1.0).contains(m)), "{}", s.key);
            assert!(s.t_live.0 <= s.t_opt.0, "{} optimum starts too low", s.key);
            assert!(s.t_opt.1 <= s.t_live.1, "{} optimum ends too high", s.key);
        }
    }

    #[test]
    fn every_spot_lists_known_species() {
        for s in SPOTS {
            assert!(!s.species.is_empty(), "{}", s.id);
            assert!(s.species.iter().all(|x| x.species < SPECIES.len()));
        }
    }
}
