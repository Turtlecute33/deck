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

//! Design tokens and the pieces every layout is built from.

use crate::display;
use crate::engine::{Band, SpeciesScore, band};
use crate::view::{View, WaterReading};

use bmc_wasm_sdk::*;

// ── Design tokens ───────────────────────────────────────────────────────
// A dark deck face, warm rather than neutral: the greys carry a little red
// and green over blue so the panel reads like lamplight instead of moonlight,
// and the accents run gold to lime rather than teal to mint.
//
// The SDK greys are pure neutral, so these are spelled out rather than taken
// from the palette. They stay close in value to GRAY_100/90/80/40/50 — the
// shift is in hue, not brightness.

pub const BG: Color = Color::from_hex(0x19_16_12);
pub const PANEL: Color = Color::from_hex(0x2A_25_1F);
pub const TRACK: Color = Color::from_hex(0x3D_37_2F);
pub const HAIRLINE: Color = Color::from_hex(0x3D_37_2F);
pub const TEXT: Color = Color::from_hex(0xF6_F2_EA);
pub const MUTED: Color = Color::from_hex(0xB0_A7_9A);
pub const LABEL: Color = Color::from_hex(0x91_88_7B);

pub const RADIUS: f32 = 14.0;

/// The index's colour, from the source engine's three bands.
#[must_use]
pub fn band_color(score: u32) -> Color {
    match band(score) {
        Band::Good => LIME_30,
        Band::Fair => YELLOW_30,
        Band::Poor => RED_40,
    }
}

/// The darker end of the gauge gradient for a band. The good band sweeps gold
/// into lime, which is where most of the warmth on the face comes from — the
/// gauge is the largest coloured thing on it.
#[must_use]
pub fn band_color_deep(score: u32) -> Color {
    match band(score) {
        Band::Good => YELLOW_30,
        Band::Fair => ORANGE_40,
        Band::Poor => RED_60,
    }
}

/// Colour for a single species' bar. The engine scores a species on 0.0-1.4,
/// a different scale from the 0-100 index, so it gets its own thresholds
/// rather than being pushed through [`band_color`].
#[must_use]
pub fn species_color(score: f64) -> Color {
    if score >= 0.55 {
        LIME_30
    } else if score >= 0.30 {
        YELLOW_30
    } else {
        RED_40
    }
}

/// A small caption above a value. `line_height: 1.0` keeps the SDK's default
/// 1.4 leading from inflating every stack.
#[must_use]
pub fn caption(content: impl Into<String>, size: u32) -> Node {
    text(
        content,
        style!(size: size, weight: FontWeight::REGULAR, color: LABEL, line_height: 1.0),
    )
}

/// A value line.
#[must_use]
pub fn value(content: impl Into<String>, size: u32, color: Color) -> Node {
    text(
        content,
        style!(size: size, weight: FontWeight::SEMIBOLD, color: color, line_height: 1.0),
    )
}

/// A value that must stay on one line whatever it holds.
///
/// Text wraps by default. Inside a tile that is a trap: a reading one glyph
/// too wide becomes two lines, the tile grows, and its whole row is then
/// taller than the row beneath it. Ellipsis is the wrong answer visually but
/// the right one structurally — a clipped reading is a bug to fix in the
/// formatting, not a reason to let the grid come apart.
#[must_use]
pub fn value_one_line(content: impl Into<String>, size: u32, color: Color) -> Node {
    text(
        content,
        style!(size: size, weight: FontWeight::SEMIBOLD, color: color, line_height: 1.0, text_overflow: TextOverflow::Ellipsis),
    )
}

/// Plain body text.
#[must_use]
pub fn body(content: impl Into<String>, size: u32, color: Color) -> Node {
    text(
        content,
        style!(size: size, weight: FontWeight::REGULAR, color: color, line_height: 1.0),
    )
}

/// Sizes for a stat tile, so each layout band can set its own rhythm.
#[derive(Clone, Copy, Debug)]
pub struct TileStyle {
    pub caption: u32,
    pub value: u32,
    pub padding: f32,
    pub gap: f32,
}

/// Readings are bare numbers now (the unit sits in the caption), so the
/// widest a tile has to hold is about five glyphs and the value can be set
/// large without risking a wrap.
pub const TILE_FULL: TileStyle = TileStyle {
    caption: 17,
    value: 40,
    padding: 18.0,
    gap: 10.0,
};

pub const TILE_COMPACT: TileStyle = TileStyle {
    caption: 14,
    value: 28,
    padding: 12.0,
    gap: 6.0,
};

/// One stat: a caption and a reading, nothing else.
///
/// Tiles used to carry a qualifier line underneath ("today", "mean cover").
/// It said what the caption already said, so it went; every reading here now
/// has to stand up under its own label without a footnote.
///
/// The content is centred on the tile's main axis, so a tile stretched by its
/// row sits its two lines in the middle of the box rather than hanging from
/// the top edge with the slack pooled underneath.
#[must_use]
pub fn tile(label: impl Into<String>, reading: String, style: TileStyle, accent: Color) -> Node {
    col(
        props!(
            background: PANEL,
            border_radius: RADIUS,
            padding: style.padding,
            gap: style.gap,
            flex: 1.0,
            justify_content: Justify::Center
        ),
        [
            caption(label, style.caption),
            value_one_line(reading, style.value, accent),
        ],
    )
}

/// A proportional bar. The filled part and the remainder are flex weights, so
/// the bar fills whatever width the parent gives it without the widget having
/// to know the pixel geometry.
#[must_use]
pub fn bar(fraction: f64, height: f32, fill: Color) -> Node {
    let filled = fraction.clamp(0.0, 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the fraction is clamped to 0.0..=1.0 before the cast"
    )]
    let filled = filled as f32;
    row(
        props!(height: height, background: TRACK, border_radius: height / 2.0),
        [
            col(
                props!(flex: filled.max(0.001), background: fill, border_radius: height / 2.0),
                [],
            ),
            spacer((1.0 - filled).max(0.0)),
        ],
    )
}

/// The index gauge: an open ring with the score in the middle.
///
/// Drawn as a single canvas so the number sits dead centre of the ring
/// without depending on how the surrounding column distributes its space.
#[must_use]
pub fn gauge(score: u32, label: &str, size: f32, number_size: u32) -> Node {
    const START_DEG: f32 = 225.0;
    const SWEEP_DEG: f32 = 270.0;

    let centre = size / 2.0;
    let stroke = (size * 0.075).clamp(8.0, 18.0);
    let radius = size / 2.0 - stroke / 2.0 - 2.0;
    let start = START_DEG.to_radians();
    let sweep = SWEEP_DEG.to_radians();
    #[expect(
        clippy::cast_precision_loss,
        reason = "the score is 0..=97, well inside f32's exact integer range"
    )]
    let fraction = (score as f32 / 100.0).clamp(0.0, 1.0);

    let mut draws = vec![Draw::arc(
        centre,
        centre,
        radius,
        start,
        start + sweep,
        stroke,
        TRACK,
        ArcSegments::Continuous,
        ArcCap::Round,
    )];
    if fraction > 0.0 {
        draws.push(Draw::arc(
            centre,
            centre,
            radius,
            start,
            start + sweep * fraction,
            stroke,
            ArcFill::gradient(band_color_deep(score), band_color(score)),
            ArcSegments::Continuous,
            ArcCap::Round,
        ));
    }
    #[expect(clippy::cast_precision_loss, reason = "font sizes are small integers")]
    let number_offset = number_size as f32 * 0.12;
    draws.push(Draw::text(
        centre,
        centre - number_offset,
        format_number!(score, 0),
        style!(
            size: number_size,
            weight: FontWeight::BOLD,
            color: TEXT,
            align: TextAlign::Center,
            valign: VerticalAlign::Center
        ),
    ));
    let label_size: u32 = if size > 150.0 { 18 } else { 14 };
    draws.push(Draw::text(
        centre,
        centre + size * 0.24,
        label.to_owned(),
        style!(
            size: label_size,
            weight: FontWeight::SEMIBOLD,
            color: band_color(score),
            align: TextAlign::Center,
            valign: VerticalAlign::Center
        ),
    ));

    canvas(props!(width: size, height: size), draws)
}

/// A species and how well the day suits it, as a named bar.
///
/// The bar is the species' own score against the engine's 1.4 ceiling, so the
/// widths compare across species and across days rather than being normalised
/// to whichever fish happens to lead today.
#[must_use]
pub fn species_cell(entry: &SpeciesScore, name_size: u32, bar_height: f32) -> Node {
    let species = entry.entry.info();
    let fraction = entry.score / 1.4;
    let fill = if entry.release_only {
        MUTED
    } else {
        species_color(entry.score)
    };
    let note = if entry.closed {
        "closed - release".to_owned()
    } else if entry.release_only {
        "release only".to_owned()
    } else {
        display::species_rule(entry.entry)
    };
    col(
        props!(gap: 7.0, flex: 1.0),
        [
            row(
                props!(cross_align: CrossAlign::Center, gap: 8.0),
                [
                    text(
                        species.name.to_owned(),
                        style!(size: name_size, weight: FontWeight::SEMIBOLD, color: TEXT, line_height: 1.0, flex: 1.0, text_overflow: TextOverflow::Ellipsis),
                    ),
                    body(note, name_size.saturating_sub(3).max(12), MUTED),
                ],
            ),
            bar(fraction, bar_height, fill),
        ],
    )
}

/// A warning worth reading before setting off — a thunderstorm, heavy rain, a
/// blow. Drawn as a bordered box rather than a pill so it reads as a notice.
///
/// Only the negatives are shown: a positive modifier tells you nothing you
/// would act on, and the index already carries it.
#[must_use]
pub fn warning_chip(text_content: &str, size: u32) -> Node {
    let tint = ORANGE_40;
    col(
        props!(
            border_radius: 6.0,
            border_width: 2.0,
            border_color: tint,
            background: tint.with_alpha(0.14),
            padding: 10.0
        ),
        [text(
            text_content.to_owned(),
            style!(size: size, weight: FontWeight::SEMIBOLD, color: tint, line_height: 1.0),
        )],
    )
}

/// A full-width hairline, for separating bands within a panel.
#[must_use]
pub fn divider() -> Node {
    col(props!(height: 1.0, background: HAIRLINE), [])
}

/// The header every layout leads with: the spot, and which day it is.
///
/// `extra` is dropped in between the title and the date, where the wide
/// layout puts its warning chips; the narrow ones pass nothing.
#[must_use]
pub fn header(name: &str, date: &str, season: &str, wide: bool, extra: Vec<Node>) -> Node {
    let (name_size, meta_size) = if wide { (30, 20) } else { (21, 15) };
    // The title is wrapped rather than flexed directly: a flexed *text* node
    // keeps its natural width as its basis, while a flexed container starts
    // from zero. Both sides have to start from zero for their grow shares to
    // come out equal, which is what centres whatever sits between them.
    let mut children: Vec<Node> = vec![col(
        props!(flex: 1.0),
        [text(
            name.to_owned(),
            style!(size: name_size, weight: FontWeight::SEMIBOLD, color: TEXT, line_height: 1.0, text_overflow: TextOverflow::Ellipsis),
        )],
    )];
    children.extend(extra);
    // The title and the date block both take a flex share, so they claim equal
    // halves of whatever the middle does not use and `extra` lands centred on
    // the face rather than shoved up against the date.
    children.push(col(
        props!(gap: 5.0, flex: 1.0, cross_align: CrossAlign::End),
        [
            value(display::date_line(date), meta_size + 3, TEXT),
            body(season.to_owned(), meta_size, LABEL),
        ],
    ));
    row(props!(cross_align: CrossAlign::Center, gap: 14.0), children)
}

/// A centred one-line screen, for loading and failure.
#[must_use]
pub fn message(content: &str) -> Node {
    col(
        props!(background: BG, flex: 1.0),
        [center(
            props!(flex: 1.0),
            [body(content.to_owned(), 30, MUTED)],
        )],
    )
}

/// The caption of the water temperature tile: the sea surface is measured,
/// every other water is an estimate.
#[must_use]
pub fn water_caption(view: &View) -> String {
    let what = if view.evaluation.water_measured {
        "SEA"
    } else {
        "WATER"
    };
    fmt!("{} {}", what, display::temperature_unit())
}

/// The tile for the reading that tells most about this water: flow on a
/// river, waves on the sea, clarity elsewhere.
#[must_use]
pub fn water_reading_tile(view: &View, style: TileStyle) -> Node {
    let (label, reading) = match view.water_reading() {
        WaterReading::Flow(ratio) => ("FLOW %", display::flow_number(ratio)),
        WaterReading::Waves(metres) => ("WAVES m", display::wave_number(metres)),
        WaterReading::Clarity(turbidity) => ("CLARITY %", display::clarity_number(turbidity)),
    };
    tile(label, reading, style, TEXT)
}
