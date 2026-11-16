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

//! The 638x480 face: the gauge across the top, six readings and two species.
//! Narrower than Full, so the gauge and its window share a row and the
//! readings drop to the compact tile rhythm.

use crate::display::{self, NOT_AVAILABLE};
use crate::render::common::{self, BG, MUTED, PANEL, RADIUS, TEXT, TILE_COMPACT};
use crate::view::View;

use bmc_wasm_sdk::*;

const GAUGE_SIZE: f32 = 150.0;
const GAUGE_NUMBER: u32 = 54;

fn index_panel(view: &View) -> Node {
    let evaluation = &view.evaluation;
    let (window_label, window_span) = match view.best_window() {
        Some(w) => (w.kind.label().to_owned(), w.span.clone()),
        None => (NOT_AVAILABLE.to_owned(), String::new()),
    };
    row(
        props!(
            background: PANEL,
            border_radius: RADIUS,
            padding: 14.0,
            gap: 16.0,
            cross_align: CrossAlign::Center
        ),
        [
            common::gauge(evaluation.score, evaluation.label(), GAUGE_SIZE, GAUGE_NUMBER),
            col(
                props!(gap: 10.0, flex: 1.0),
                [
                    common::caption("BEST WINDOW", 14),
                    common::value(window_label, 24, TEXT),
                    common::body(window_span, 17, MUTED),
                    common::divider(),
                    common::caption("MOON", 14),
                    common::body(
                        display::moon(evaluation.moon.name, evaluation.moon.illumination),
                        17,
                        MUTED,
                    ),
                ],
            ),
        ],
    )
}

fn readings(view: &View) -> Node {
    let day = &view.day;
    let evaluation = &view.evaluation;
    let degrees = display::temperature_unit();
    let unit = |label: &str, unit: &str| fmt!("{} {}", label, unit);
    col(
        props!(gap: 10.0),
        [
            row(
                props!(gap: 10.0),
                [
                    common::tile(
                        common::water_caption(view),
                        display::water_number(evaluation.water_temp),
                        TILE_COMPACT,
                        TEXT,
                    ),
                    common::tile(
                        unit("AIR", degrees),
                        display::air_range(day.t_max, day.t_min),
                        TILE_COMPACT,
                        TEXT,
                    ),
                    common::tile(
                        unit("WIND", display::speed_unit()),
                        display::wind_number(day.wind),
                        TILE_COMPACT,
                        TEXT,
                    ),
                ],
            ),
            row(
                props!(gap: 10.0),
                [
                    common::tile(
                        unit("PRESSURE", "mbar"),
                        display::pressure_number(day.pressure),
                        TILE_COMPACT,
                        TEXT,
                    ),
                    common::tile(
                        unit("RAIN", "mm"),
                        display::rain_number(day.precipitation),
                        TILE_COMPACT,
                        TEXT,
                    ),
                    common::water_reading_tile(view, TILE_COMPACT),
                ],
            ),
        ],
    )
}

fn species(view: &View) -> Node {
    let cells: Vec<Node> = view
        .evaluation
        .species
        .iter()
        .take(2)
        .map(|s| common::species_cell(s, 17, 9.0))
        .collect();
    col(
        props!(gap: 9.0, flex: 1.0, justify_content: Justify::End),
        [
            common::caption("IN SEASON NOW", 14),
            row(props!(gap: 16.0), cells),
        ],
    )
}

#[must_use]
pub fn large(view: &View) -> Node {
    col(
        props!(background: BG, padding: 16.0, gap: 12.0),
        [
            common::header(
                view.spot.name,
                &view.day.date,
                view.evaluation.season.label(),
                false,
                Vec::new(),
            ),
            index_panel(view),
            readings(view),
            species(view),
        ],
    )
}
