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

//! The requests this widget makes: the ones the dovepescare.com site sends
//! to Open-Meteo for one spot (`assets/js/api.js`).
//!
//! The coordinates go in as the catalogue writes them, as text. That is
//! deliberate: the provider needs a decimal point, and a number formatted
//! on the Deck could follow a comma-decimal locale.

use crate::spot::Spot;

/// Days of history. The longest water window (a lake) is 14 days.
pub const PAST_DAYS: usize = 14;
/// Days of forecast.
pub const FORECAST_DAYS: usize = 7;

const DAILY: &str = "weather_code,temperature_2m_max,temperature_2m_min,temperature_2m_mean,\
precipitation_sum,wind_speed_10m_max,wind_direction_10m_dominant,\
pressure_msl_mean,cloud_cover_mean,sunrise,sunset";

fn coordinates(url: &mut String, lat: &str, lon: &str) {
    url.push_str("?latitude=");
    url.push_str(lat);
    url.push_str("&longitude=");
    url.push_str(lon);
}

/// The daily forecast. `current=temperature_2m` is not displayed: it is
/// there for `current.time`, which says which day is today in Italian time.
#[must_use]
pub fn forecast(spot: &Spot) -> String {
    let mut url = String::from("https://api.open-meteo.com/v1/forecast");
    coordinates(&mut url, spot.lat, spot.lon);
    url.push_str("&daily=");
    url.push_str(DAILY);
    url.push_str("&current=temperature_2m&past_days=14&forecast_days=7&timezone=Europe%2FRome");
    url
}

/// The river discharge, at the model cell the river runs through. Rivers and
/// torrents only.
#[must_use]
pub fn flow(spot: &Spot) -> Option<String> {
    let (lat, lon) = spot.flow_cell?;
    let mut url = String::from("https://flood-api.open-meteo.com/v1/flood");
    coordinates(&mut url, lat, lon);
    url.push_str("&daily=river_discharge&past_days=45&forecast_days=7");
    Some(url)
}

/// The sea surface temperature and waves. Sea spots only.
#[must_use]
pub fn sea(spot: &Spot) -> Option<String> {
    if spot.kind != crate::spot::WaterKind::Sea {
        return None;
    }
    let mut url = String::from("https://marine-api.open-meteo.com/v1/marine");
    coordinates(&mut url, spot.lat, spot.lon);
    url.push_str(
        "&hourly=sea_surface_temperature&daily=wave_height_max\
&past_days=14&forecast_days=7&timezone=Europe%2FRome&cell_selection=sea",
    );
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spot::{SPOTS, WaterKind, resolve};

    fn spot(address: &str) -> &'static Spot {
        resolve(address).expect("BUG: the catalogue has this spot")
    }

    #[test]
    fn the_forecast_asks_for_the_spot_and_every_variable_the_engine_reads() {
        let url = forecast(spot("panaro-a-vignola"));
        assert!(url.starts_with("https://api.open-meteo.com/v1/forecast?latitude=44.4745&longitude=11.0078&"));
        for variable in DAILY.split(',') {
            assert!(url.contains(variable), "{variable} is not requested");
        }
        assert!(url.contains("past_days=14"));
        assert!(url.contains("forecast_days=7"));
        assert!(url.contains("current=temperature_2m"));
        assert!(url.contains("timezone=Europe%2FRome"));
        assert_eq!(PAST_DAYS, 14);
        assert_eq!(FORECAST_DAYS, 7);
    }

    #[test]
    fn the_history_covers_the_longest_water_window_and_the_rain_total() {
        for kind in [
            WaterKind::Torrent,
            WaterKind::River,
            WaterKind::Canal,
            WaterKind::Lake,
            WaterKind::Reservoir,
            WaterKind::Quarry,
            WaterKind::Sea,
        ] {
            assert!(kind.inertia().days <= PAST_DAYS + 1);
        }
    }

    #[test]
    fn only_rivers_and_torrents_ask_for_the_discharge() {
        let river = spot("panaro-a-vignola");
        let url = flow(river).expect("BUG: a river has a flow cell");
        assert!(url.starts_with("https://flood-api.open-meteo.com/v1/flood?latitude=44.525&longitude=10.975&"));
        assert!(url.contains("daily=river_discharge&past_days=45&forecast_days=7"));
        for s in SPOTS {
            assert_eq!(flow(s).is_some(), s.kind.runs(), "{}", s.id);
        }
    }

    #[test]
    fn only_sea_spots_ask_for_the_sea() {
        let url = sea(spot("portocanale-di-rimini")).expect("BUG: a sea spot");
        assert!(url.starts_with("https://marine-api.open-meteo.com/v1/marine?latitude="));
        assert!(url.contains("cell_selection=sea"));
        for s in SPOTS {
            assert_eq!(sea(s).is_some(), s.kind == WaterKind::Sea, "{}", s.id);
        }
    }

    #[test]
    fn every_request_goes_to_open_meteo_only() {
        for s in SPOTS {
            for url in [Some(forecast(s)), flow(s), sea(s)].into_iter().flatten() {
                let host = url
                    .strip_prefix("https://")
                    .and_then(|u| u.split('/').next())
                    .unwrap_or_default();
                assert!(
                    matches!(
                        host,
                        "api.open-meteo.com" | "flood-api.open-meteo.com" | "marine-api.open-meteo.com"
                    ),
                    "{url}"
                );
                assert!(!url.contains(' '), "{url}");
            }
        }
    }
}
