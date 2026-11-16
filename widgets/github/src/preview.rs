//! Screens with made-up data, rendered off the Deck for the README screenshots.
//!
//! All names, titles and counts are fictional. The clock is fixed, never the real time.

use bmc_wasm_sdk::Node;

use crate::widget::{Notif, Stats, kind, notifications_screen, quiet_screen};

pub struct Shot {
    pub name: &'static str,
    pub caption: &'static str,
    pub width: u32,
    pub height: u32,
    pub node: Node,
}

/// Wednesday 2026-06-10 10:42 UTC.
const NOW: i64 = 1_781_088_120;
const MIN: i64 = 60;

fn notif(reason: &str, subject: &str, title: &str, place: &str, age: i64) -> Notif {
    let (label, hot) = kind(reason, subject);
    Notif {
        label,
        hot,
        title: title.to_string(),
        place: place.to_string(),
        updated_at: NOW - age,
    }
}

#[must_use]
pub fn shots() -> Vec<Shot> {
    let notifs = [
        notif("review_requested", "PullRequest", "Add rate limiting to the public endpoints", "api #142", 4 * MIN),
        notif("subscribed", "Issue", "Fix flaky login test", "website #87", 38 * MIN),
        notif("ci_activity", "CheckSuite", "CI workflow run failed for main branch", "toolkit", 2 * 60 * MIN + 10 * MIN),
    ];
    let stats = Stats {
        today: 12,
        total: 1284,
        followers: 356,
        followers_delta: Some(9),
        top: Some(("toolkit".to_string(), 612)),
    };
    vec![
        Shot {
            name: "notifications",
            caption: "It shows the unread count and the newest three notifications, with a failed CI run in orange.",
            width: 1280,
            height: 480,
            node: notifications_screen(&notifs, 7, NOW),
        },
        Shot {
            name: "stats",
            caption: "With no unread notifications, it shows stars today, total stars, followers and the top repo.",
            width: 1280,
            height: 480,
            node: quiet_screen(Some(&stats)),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shot_encodes() {
        for shot in shots() {
            assert!(!bmc_wasm_sdk::encode(&shot.node).is_empty(), "{} encodes empty", shot.name);
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
