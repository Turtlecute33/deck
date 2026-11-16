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

//! The 1280x480 face: the index, the readings behind it, and the species in
//! season.
//!
//! Two rows of four tiles and nothing below them. An earlier version carried a
//! seven-day strip along the bottom, which crowded the panel and ran off the
//! edge of the screen; the space it freed went back into the gauge and the
//! type sizes.

use crate::display::{self, NOT_AVAILABLE};
use crate::render::common::{self, BG, MUTED, PANEL, RADIUS, TEXT, TILE_FULL};
use crate::view::View;

use bmc_wasm_sdk::*;

const GAUGE_SIZE: f32 = 216.0;
const GAUGE_NUMBER: u32 = 74;
const PANEL_WIDTH: f32 = 280.0;
const SPECIES_SHOWN: usize = 4;
const WARNING_SIZE: u32 = 18;
/// More chips than this would push the spot name off the header.
const MAX_WARNINGS: usize = 2;

/// Outer margin, and the gap between the three bands of the right column.
const PAD: f32 = 24.0;
const BAND_GAP: f32 = 12.0;
const COLUMN_GAP: f32 = 16.0;

/// The gauge and the one thing to do with it: when to be on the bank.
fn index_panel(view: &View) -> Node {
    let (window_label, window_span) = match view.best_window() {
        Some(w) => (w.kind.label().to_owned(), w.span.clone()),
        None => (NOT_AVAILABLE.to_owned(), String::new()),
    };

    col(
        props!(
            background: PANEL,
            border_radius: RADIUS,
            padding: 20.0,
            gap: 16.0,
            width: PANEL_WIDTH,
            cross_align: CrossAlign::Center,
            justify_content: Justify::Center
        ),
        [
            common::gauge(
                view.evaluation.score,
                view.evaluation.label(),
                GAUGE_SIZE,
                GAUGE_NUMBER,
            ),
            common::divider(),
            col(
                props!(gap: 6.0, cross_align: CrossAlign::Center),
                [
                    common::caption("BEST WINDOW", 16),
                    common::value(window_label, 26, TEXT),
                    common::body(window_span, 18, MUTED),
                ],
            ),
        ],
    )
}

/// A tile caption with its unit trailing: `WATER °C`. The unit lives here so
/// the reading stays a bare number that cannot outgrow its box.
fn unit(label: &str, unit: &str) -> String {
    fmt!("{} {}", label, unit)
}

/// The four readings that move the index most.
fn primary_readings(view: &View) -> Node {
    let day = &view.day;
    let degrees = display::temperature_unit();
    row(
        props!(gap: BAND_GAP, flex: 1.0),
        [
            common::tile(
                common::water_caption(view),
                display::water_number(view.evaluation.water_temp),
                TILE_FULL,
                TEXT,
            ),
            common::tile(
                unit("AIR", degrees),
                display::air_range(day.t_max, day.t_min),
                TILE_FULL,
                TEXT,
            ),
            common::tile(
                unit("WIND", display::speed_unit()),
                display::wind_number(day.wind),
                TILE_FULL,
                TEXT,
            ),
            common::tile(
                unit("PRESSURE", "mbar"),
                display::pressure_number(day.pressure),
                TILE_FULL,
                TEXT,
            ),
        ],
    )
}

/// The rain, the sky, the water itself, and the moon.
fn water_readings(view: &View) -> Node {
    let evaluation = &view.evaluation;
    row(
        props!(gap: BAND_GAP, flex: 1.0),
        [
            common::tile(
                unit("RAIN", "mm"),
                display::rain_number(view.day.precipitation),
                TILE_FULL,
                TEXT,
            ),
            common::tile(
                unit("CLOUD", "%"),
                display::percent_number(view.day.cloud_cover),
                TILE_FULL,
                TEXT,
            ),
            common::water_reading_tile(view, TILE_FULL),
            common::tile(
                unit("MOON", "%"),
                display::percent_number(Some(evaluation.moon.illumination * 100.0)),
                TILE_FULL,
                TEXT,
            ),
        ],
    )
}

/// The species the day suits best, ranked.
///
/// Carried on a panel like the tiles above it, so the right column reads as
/// three bands of one material rather than two tiled rows with a loose strip
/// underneath.
fn species(view: &View) -> Node {
    let cells: Vec<Node> = view
        .evaluation
        .species
        .iter()
        .take(SPECIES_SHOWN)
        .map(|s| common::species_cell(s, 19, 12.0))
        .collect();
    col(
        props!(
            background: PANEL,
            border_radius: RADIUS,
            padding: TILE_FULL.padding,
            gap: 14.0,
            flex: 1.0,
            justify_content: Justify::Center
        ),
        [
            common::caption("IN SEASON NOW", TILE_FULL.caption),
            row(props!(gap: 24.0), cells),
        ],
    )
}

#[must_use]
pub fn full(view: &View) -> Node {
    // The warnings ride on the header line rather than taking one of their
    // own: they are usually absent, and a reserved empty row would push the
    // whole face down for nothing.
    let chips: Vec<Node> = view
        .warnings()
        .iter()
        .take(MAX_WARNINGS)
        .map(|w| common::warning_chip(&display::modifier(w), WARNING_SIZE))
        .collect();

    col(
        props!(background: BG, padding: PAD, gap: COLUMN_GAP),
        [
            common::header(
                view.spot.name,
                &view.day.date,
                view.evaluation.season.label(),
                true,
                chips,
            ),
            row(
                props!(gap: COLUMN_GAP, flex: 1.0),
                [
                    index_panel(view),
                    // Three equal bands: the two reading rows and the species
                    // panel each take a third of the height, so the column has
                    // one rhythm instead of two tight rows and a gap.
                    col(
                        props!(gap: BAND_GAP, flex: 1.0),
                        [primary_readings(view), water_readings(view), species(view)],
                    ),
                ],
            ),
        ],
    )
}
