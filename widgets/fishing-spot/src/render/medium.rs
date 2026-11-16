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

//! The 638x238 face: the gauge beside the four readings that matter most.
//! Too short for the week strip or the species bars, so neither is drawn.

use crate::display::{self, NOT_AVAILABLE};
use crate::render::common::{self, BG, MUTED, TEXT, TILE_COMPACT};
use crate::view::View;

use bmc_wasm_sdk::*;

const GAUGE_SIZE: f32 = 132.0;
const GAUGE_NUMBER: u32 = 46;

#[must_use]
pub fn medium(view: &View) -> Node {
    let day = &view.day;
    let evaluation = &view.evaluation;
    let water = display::water_number(evaluation.water_temp);
    let degrees = display::temperature_unit();
    let window = view
        .best_window()
        .map_or_else(String::new, |w| w.span.clone());
    let window_label = view
        .best_window()
        .map_or(NOT_AVAILABLE, |w| w.kind.label());

    col(
        props!(background: BG, padding: 14.0, gap: 10.0),
        [
            common::header(
                view.spot.name,
                &day.date,
                evaluation.season.label(),
                false,
                Vec::new(),
            ),
            row(
                props!(gap: 12.0, flex: 1.0, cross_align: CrossAlign::Center),
                [
                    col(
                        props!(gap: 6.0, cross_align: CrossAlign::Center),
                        [
                            common::gauge(
                                evaluation.score,
                                evaluation.label(),
                                GAUGE_SIZE,
                                GAUGE_NUMBER,
                            ),
                            common::body(display::span(window_label, &window), 14, MUTED),
                        ],
                    ),
                    col(
                        props!(gap: 8.0, flex: 1.0),
                        [
                            row(
                                props!(gap: 8.0),
                                [
                                    common::tile(
                                        common::water_caption(view),
                                        water,
                                        TILE_COMPACT,
                                        TEXT,
                                    ),
                                    common::tile(
                                        fmt!("AIR {}", degrees),
                                        display::air_range(day.t_max, day.t_min),
                                        TILE_COMPACT,
                                        TEXT,
                                    ),
                                ],
                            ),
                            row(
                                props!(gap: 8.0),
                                [
                                    common::tile(
                                        fmt!("WIND {}", display::speed_unit()),
                                        display::wind_number(day.wind),
                                        TILE_COMPACT,
                                        TEXT,
                                    ),
                                    common::tile(
                                        "PRESSURE mbar",
                                        display::pressure_number(day.pressure),
                                        TILE_COMPACT,
                                        TEXT,
                                    ),
                                ],
                            ),
                        ],
                    ),
                ],
            ),
        ],
    )
}
