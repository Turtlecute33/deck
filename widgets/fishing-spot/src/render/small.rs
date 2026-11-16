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

//! The 317x238 face: the index, the water, and when to go. At this size only
//! the decision fits, so everything that merely explains it is dropped.

use crate::display::{self, NOT_AVAILABLE};
use crate::render::common::{self, BG, LABEL, MUTED, TEXT};
use crate::view::View;

use bmc_wasm_sdk::*;

const GAUGE_SIZE: f32 = 124.0;
const GAUGE_NUMBER: u32 = 44;

/// A one-line reading: caption on the left, value hugging the right edge.
fn line(label: &str, reading: String, accent: Color) -> Node {
    row(
        props!(cross_align: CrossAlign::Center, gap: 8.0),
        [
            text(
                label.to_owned(),
                style!(size: 14, weight: FontWeight::REGULAR, color: LABEL, line_height: 1.0, flex: 1.0),
            ),
            text(
                reading,
                style!(size: 16, weight: FontWeight::SEMIBOLD, color: accent, line_height: 1.0, align: TextAlign::Right),
            ),
        ],
    )
}

#[must_use]
pub fn small(view: &View) -> Node {
    let day = &view.day;
    let evaluation = &view.evaluation;
    let water = evaluation
        .water_temp
        .map_or_else(|| NOT_AVAILABLE.to_owned(), display::temperature);
    let window = view.best_window();

    col(
        props!(background: BG, padding: 12.0, gap: 8.0, cross_align: CrossAlign::Center),
        [
            text(
                view.spot.name.to_owned(),
                style!(size: 16, weight: FontWeight::SEMIBOLD, color: TEXT, line_height: 1.0, text_overflow: TextOverflow::Ellipsis),
            ),
            common::gauge(evaluation.score, evaluation.label(), GAUGE_SIZE, GAUGE_NUMBER),
            col(
                props!(gap: 6.0, flex: 1.0, justify_content: Justify::End),
                [
                    line("Water", water, TEXT),
                    line("Air", display::air_range(day.t_max, day.t_min), TEXT),
                    line(
                        window.map_or("Window", |w| w.kind.label()),
                        window.map_or_else(|| NOT_AVAILABLE.to_owned(), |w| w.span.clone()),
                        MUTED,
                    ),
                ],
            ),
        ],
    )
}
