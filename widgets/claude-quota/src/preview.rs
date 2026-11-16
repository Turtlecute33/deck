//! Screens from made-up data, for the README screenshots. Host only.
//!
//! All values are fictional, and the clock is fixed, so the shots do not
//! change from run to run.

use crate::widget::{Quota, State, Window, dashboard};

/// One screen to render, with its size in pixels.
pub struct Shot {
    pub name: &'static str,
    pub caption: &'static str,
    pub width: u32,
    pub height: u32,
    pub node: bmc_wasm_sdk::Node,
}

/// The fixed clock of every shot: Wednesday 2026-06-10 10:42 UTC.
const NOW: i64 = 1_781_088_120;
/// The last weekly reset: Friday 2026-06-05 15:00 UTC.
const WEEK_START: i64 = 1_780_671_600;
const WEEK_S: i64 = 7 * 86_400;

/// Made-up weekly use, `(hours since WEEK_START, pct)`: work hours on
/// weekdays, a little at the weekend, nothing at night.
const WEEK: &[(i64, f64)] = &[
    // Friday
    (0, 0.0),
    (1, 1.5),
    (2, 3.5),
    (3, 5.0),
    (4, 7.0),
    // Saturday
    (19, 7.0),
    (20, 8.5),
    (21, 10.0),
    // Sunday
    (48, 10.0),
    (49, 11.0),
    (50, 12.0),
    // Monday
    (65, 12.0),
    (66, 14.0),
    (67, 16.5),
    (68, 18.5),
    (69, 20.0),
    (70, 20.5),
    (71, 22.5),
    (72, 24.5),
    (73, 26.5),
    (74, 28.5),
    (75, 30.0),
    // Tuesday
    (89, 30.0),
    (90, 32.5),
    (91, 35.0),
    (92, 37.5),
    (93, 39.5),
    (94, 40.0),
    (95, 42.5),
    (96, 45.0),
    (97, 47.0),
    (98, 49.0),
    (99, 50.5),
    (101, 52.0),
    // Wednesday, until 10:00
    (113, 52.0),
    (114, 55.0),
    (115, 58.5),
];

/// A fresh reply at `NOW`. `scale` multiplies the weekly curve.
fn state(five_pct: f64, five_left_s: i64, weekly_pct: f64, scale: f64) -> State {
    State {
        quota: Some(Quota {
            updated_at: NOW - 25,
            server_stale: false,
            five: Some(Window {
                pct: five_pct,
                resets_at: NOW + five_left_s,
            }),
            weekly: Some(Window {
                pct: weekly_pct,
                resets_at: WEEK_START + WEEK_S,
            }),
            week_start: WEEK_START,
            points: WEEK.iter().map(|&(h, p)| (h * 3600, p * scale)).collect(),
        }),
        last_ok: NOW - 25,
        failed: false,
    }
}

/// The shots for the README, at the size of the Deck screen.
#[must_use]
pub fn shots() -> Vec<Shot> {
    vec![
        Shot {
            name: "fullscreen",
            caption: "Mid-week: the 5-hour and weekly use on the left, the weekly use since the last reset on the right.",
            width: 1280,
            height: 480,
            node: dashboard(&state(38.0, 2 * 3600 + 10 * 60, 61.0, 1.0), NOW),
        },
        Shot {
            name: "near-limit",
            caption: "Near the limits: at 80 % or more, the percentage turns orange.",
            width: 1280,
            height: 480,
            node: dashboard(&state(92.0, 41 * 60, 87.0, 1.42), NOW),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shot_encodes() {
        for s in shots() {
            assert!(!bmc_wasm_sdk::encode(&s.node).is_empty(), "{}", s.name);
        }
    }

    /// Writes each shot as the encoded tree the Deck would receive, for
    /// `tools/screenshots`. Does nothing unless `DECK_SHOTS_DIR` is set.
    #[test]
    fn write_shots() {
        let Some(dir) = std::env::var_os("DECK_SHOTS_DIR") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        for s in shots() {
            let file = dir.join(format!("{}--{}--{}x{}.tree", env!("CARGO_PKG_NAME"), s.name, s.width, s.height));
            std::fs::write(&file, bmc_wasm_sdk::encode(&s.node)).expect("BUG: the shots dir is writable");
        }
    }
}
