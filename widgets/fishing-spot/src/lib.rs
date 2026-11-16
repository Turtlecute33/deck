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

//! Fishing Spot: the daily fishing index of a dovepescare.com spot.
//!
//! The scoring is a port of the dovepescare.com engine, and the spot comes
//! from the site's public catalogue. The spot is a widget setting: nothing
//! about which water you fish is built into the widget. The forecast comes
//! straight from Open-Meteo, the provider the site uses.

pub mod catalogue;
pub mod display;
pub mod engine;
mod manifest_params;
pub mod model;
#[cfg(any(target_arch = "wasm32", test))]
pub mod render;
pub mod spot;
pub mod url;
pub mod view;

use spot::Spot;

/// What a fetch attempt should do with the state the widget already holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchOutcome {
    /// A good payload: replace what is held.
    Store,
    /// The refresh failed but earlier data is still on screen; keep it and
    /// let the stale overlay say how old it is.
    Keep,
    /// Nothing held and nothing arrived.
    Fail,
}

/// Decide what to do with a response.
///
/// A failed refresh must never blank a screen that is already showing a
/// usable forecast: a day-old index is still worth reading, and the host
/// marks it stale on the widget's behalf.
#[must_use]
pub fn fetch_outcome(parsed_ok: bool, has_data: bool) -> FetchOutcome {
    if parsed_ok {
        FetchOutcome::Store
    } else if has_data {
        FetchOutcome::Keep
    } else {
        FetchOutcome::Fail
    }
}

/// How many days past today the selected day sits.
#[must_use]
pub fn day_offset(day: manifest_params::Day) -> usize {
    match day {
        manifest_params::Day::Today => 0,
        manifest_params::Day::Tomorrow => 1,
    }
}

/// What the Spot setting holds.
#[derive(Clone, Copy, Debug)]
pub enum SpotChoice {
    /// Nothing yet.
    Missing,
    /// Text that names no spot of the catalogue.
    Unknown,
    Found(&'static Spot),
}

#[must_use]
pub fn choose_spot(setting: Option<&str>) -> SpotChoice {
    match setting.map(str::trim) {
        None | Some("") => SpotChoice::Missing,
        Some(text) => spot::resolve(text).map_or(SpotChoice::Unknown, SpotChoice::Found),
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_glue {
    use super::{SpotChoice, choose_spot, day_offset, display, manifest_params, model, render, url, view};
    use crate::spot::Spot;
    use std::cell::RefCell;

    use bmc_wasm_sdk::*;

    /// Daily values from a model that runs hourly: a quarter of an hour keeps
    /// the index current and the request count small. It also decides how
    /// long after midnight the widget takes to roll onto the new day.
    const REFRESH_MS: u32 = 900_000;

    /// The three requests, in registration order.
    const FORECAST: usize = 0;
    const FLOW: usize = 1;
    const SEA: usize = 2;

    /// The replies for one spot.
    #[derive(Default)]
    struct Data {
        /// The id of the spot the replies below belong to.
        spot: Option<&'static str>,
        forecast: Option<model::Forecast>,
        /// The forecast request failed and there is nothing to show.
        failed: bool,
        flow: Option<model::Flow>,
        sea: Option<model::Sea>,
    }

    impl Data {
        /// Forget the replies of another spot.
        fn keep_only(&mut self, id: &'static str) {
            if self.spot != Some(id) {
                *self = Data {
                    spot: Some(id),
                    ..Data::default()
                };
            }
        }
    }

    thread_local! {
        static DATA: RefCell<Data> = RefCell::new(Data::default());
        static POLLS: RefCell<Vec<PollHandle>> = const { RefCell::new(Vec::new()) };
        /// The spot each request in flight was built for.
        static ASKED: RefCell<[Option<&'static str>; 3]> = const { RefCell::new([None; 3]) };
    }

    fn current_spot() -> Option<&'static Spot> {
        match choose_spot(manifest_params::Params::current().spot.as_deref()) {
            SpotChoice::Found(spot) => Some(spot),
            SpotChoice::Missing | SpotChoice::Unknown => None,
        }
    }

    /// The next request of one kind, for the spot set now. `None` skips it:
    /// no spot, or a request this water does not need.
    fn build(kind: usize, make: fn(&'static Spot) -> Option<String>) -> Option<FetchSpec> {
        let spot = current_spot()?;
        let target = make(spot)?;
        ASKED.with_borrow_mut(|asked| asked[kind] = Some(spot.id));
        Some(FetchSpec::get(target))
    }

    fn build_forecast(_handle: &PollHandle) -> Option<FetchSpec> {
        build(FORECAST, |spot| Some(url::forecast(spot)))
    }

    fn build_flow(_handle: &PollHandle) -> Option<FetchSpec> {
        build(FLOW, url::flow)
    }

    fn build_sea(_handle: &PollHandle) -> Option<FetchSpec> {
        build(SEA, url::sea)
    }

    /// The spot a reply belongs to, if that spot is still the one set. A
    /// reply for a spot the setting has since moved away from is dropped.
    fn still_current(kind: usize) -> Option<&'static str> {
        let asked = ASKED.with_borrow(|asked| asked[kind])?;
        (current_spot()?.id == asked).then_some(asked)
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn init() {
        let config = PollConfig {
            interval_ms: Some(REFRESH_MS),
            ..Default::default()
        };
        let forecast = register_poll(build_forecast, on_forecast, config);
        let flow = register_poll(build_flow, on_flow, config);
        let sea = register_poll(build_sea, on_sea, config);
        POLLS.set(vec![forecast, flow, sea]);
    }

    fn on_forecast(handle: &PollHandle, response: &FetchResponse) {
        let Some(id) = still_current(FORECAST) else {
            return;
        };
        let parsed = if response.ok() {
            model::Forecast::try_from(&response.json()).ok()
        } else {
            log_warn!("fishing-spot: forecast failed (status {})", response.status);
            None
        };
        let retry = DATA.with_borrow_mut(|data| {
            data.keep_only(id);
            match super::fetch_outcome(parsed.is_some(), data.forecast.is_some()) {
                super::FetchOutcome::Store => {
                    let mut forecast = parsed.expect("BUG: Store outcome implies a parsed payload");
                    if let Some(sea) = &data.sea {
                        forecast.merge_sea(sea);
                    }
                    data.forecast = Some(forecast);
                    data.failed = false;
                    false
                }
                // A 2xx whose body would not parse is worth another try before
                // the next quarter-hour; a network failure waits for the SDK.
                super::FetchOutcome::Keep => response.ok(),
                super::FetchOutcome::Fail => {
                    data.failed = true;
                    response.ok()
                }
            }
        });
        if retry {
            handle.retry();
        }
        request_frame();
    }

    /// Without a discharge reply the index is still computed, as on the site.
    /// A failed refresh keeps the series already held for this spot.
    fn on_flow(_handle: &PollHandle, response: &FetchResponse) {
        let Some(id) = still_current(FLOW) else {
            return;
        };
        if !response.ok() {
            log_warn!("fishing-spot: discharge failed (status {})", response.status);
            return;
        }
        if let Ok(flow) = model::Flow::try_from(&response.json()) {
            DATA.with_borrow_mut(|data| {
                data.keep_only(id);
                data.flow = Some(flow);
            });
            request_frame();
        }
    }

    /// Without a sea reply the water temperature is estimated from the air
    /// and the sea state from the wind, as on the site.
    fn on_sea(_handle: &PollHandle, response: &FetchResponse) {
        let Some(id) = still_current(SEA) else {
            return;
        };
        if !response.ok() {
            log_warn!("fishing-spot: sea failed (status {})", response.status);
            return;
        }
        if let Ok(sea) = model::Sea::try_from(&response.json()) {
            DATA.with_borrow_mut(|data| {
                data.keep_only(id);
                if let Some(forecast) = &mut data.forecast {
                    forecast.merge_sea(&sea);
                }
                data.sea = Some(sea);
            });
            request_frame();
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn on_params_update() {
        // A new spot: drop the old replies and ask again now. The day only
        // re-reads the replies already in hand.
        let spot = current_spot().map(|s| s.id);
        let moved = DATA.with_borrow(|data| data.spot != spot);
        if moved {
            DATA.set(Data {
                spot,
                ..Data::default()
            });
            POLLS.with_borrow(|polls| {
                for poll in polls {
                    poll.reset_staleness();
                    poll.invalidate();
                }
            });
        }
        request_frame();
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn on_system_update() {
        request_frame();
    }

    /// The last good forecast, but only while the poll considers it stale.
    fn stale_anchor() -> Option<SystemTime> {
        POLLS.with_borrow(|polls| {
            let forecast = polls.get(FORECAST)?;
            if forecast.is_stale() {
                forecast.last_success_time()
            } else {
                None
            }
        })
    }

    fn spot_face(spot: &'static Spot, size: WidgetSize, offset: usize) -> Node {
        DATA.with_borrow(|data| {
            if data.spot != Some(spot.id) {
                return render::message(display::LOADING);
            }
            let Some(forecast) = &data.forecast else {
                return render::message(if data.failed {
                    display::CANNOT_LOAD
                } else {
                    display::LOADING
                });
            };
            let flow = data.flow.as_ref();
            // Asking for tomorrow at the end of the forecast window falls
            // back to today rather than showing an empty face.
            let built = view::View::build(spot, forecast, flow, offset)
                .or_else(|| view::View::build(spot, forecast, flow, 0));
            let Some(v) = built else {
                return render::message(display::CANNOT_LOAD);
            };
            let node = render::view(&v, size);
            match stale_anchor() {
                Some(anchor) => with_stale_overlay(node, anchor, widget_viewport().shape),
                None => node,
            }
        })
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn render(_delta_ms: u32) {
        let size = widget_size();
        let params = manifest_params::Params::current();
        let node = match choose_spot(params.spot.as_deref()) {
            SpotChoice::Missing => render::message(display::NO_SPOT),
            SpotChoice::Unknown => render::message(display::UNKNOWN_SPOT),
            SpotChoice::Found(spot) => spot_face(spot, size, day_offset(params.day)),
        };
        let _ = render_ui(size.width, size.height, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_payload_replaces_what_is_held() {
        assert_eq!(fetch_outcome(true, false), FetchOutcome::Store);
        assert_eq!(fetch_outcome(true, true), FetchOutcome::Store);
    }

    #[test]
    fn a_failed_refresh_keeps_a_forecast_and_only_errors_when_empty() {
        assert_eq!(fetch_outcome(false, true), FetchOutcome::Keep);
        assert_eq!(fetch_outcome(false, false), FetchOutcome::Fail);
    }

    #[test]
    fn the_day_param_maps_onto_a_forecast_offset() {
        assert_eq!(day_offset(manifest_params::Day::Today), 0);
        assert_eq!(day_offset(manifest_params::Day::Tomorrow), 1);
    }

    #[test]
    fn the_spot_setting_is_missing_unknown_or_found() {
        assert!(matches!(choose_spot(None), SpotChoice::Missing));
        assert!(matches!(choose_spot(Some("  ")), SpotChoice::Missing));
        assert!(matches!(choose_spot(Some("nowhere")), SpotChoice::Unknown));
        assert!(matches!(
            choose_spot(Some("https://dovepescare.com/spot/panaro-a-vignola/")),
            SpotChoice::Found(s) if s.id == "mo-panaro-vignola"
        ));
    }
}
