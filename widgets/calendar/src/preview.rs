//! The screens with made-up data, for the README screenshots. Builds on the Mac only.
//!
//! The clock is fixed: Wednesday 10 June 2026, 10:42 in Europe/Prague.
//! All calendars, events and places are fictional.

use bmc_wasm_sdk::{Color, Node, Tz};

use crate::model::{self, AllDay, DAY, Event, Kind, Reply};
use crate::widget::{self, Cal, CalData, Local, PALETTE, Settings, State};

/// One screen, ready to encode.
pub struct Shot {
    pub name: &'static str,
    pub caption: &'static str,
    pub width: u32,
    pub height: u32,
    pub node: Node,
}

/// Wed 2026-06-10 08:42:00 UTC: 10:42 in Prague.
const NOW: i64 = 1_781_080_920;
/// Prague in summer (CEST), if the time zone does not resolve.
const UTC_OFFSET: i64 = 2 * 3600;

const WORK: usize = 0;
const HOME: usize = 1;
const FAMILY: usize = 2;

fn local() -> Local {
    Local { tz: Some(Tz::from_runtime("Europe/Prague")), fallback: UTC_OFFSET }
}

/// The calendars in the widget settings: blue, green and amber.
fn settings() -> Settings {
    let cal = |name: &str, (_, hex): (&str, u32)| Cal { name: name.to_string(), color: Color::from_hex(hex) };
    Settings {
        cals: vec![cal("Work", PALETTE[0]), cal("Home", PALETTE[1]), cal("Family", PALETTE[2])],
        all_day: true,
        from_h: 8,
        to_h: 20,
    }
}

/// A busy day, as the calendar polls leave it: the last good reply 90 s ago.
fn state() -> State {
    let today = local().at(NOW).div_euclid(DAY);
    let at = |(h, m): (i64, i64)| today * DAY + h * 3600 + m * 60;
    let ev = |cal, from, to, title: &str, kind, place: &str, guests| Event {
        start: at(from),
        end: at(to),
        title: title.to_string(),
        kind,
        place: place.to_string(),
        guests,
        reply: Reply::Yes,
        cal,
    };
    let all_day = |cal, title: &str| AllDay { first: today, until: today + 1, title: title.to_string(), cal };
    let data = |events, all_day| CalData { events, all_day, ok_at: NOW - 90, error: None };

    let work = vec![
        ev(WORK, (9, 0), (9, 15), "Team sync", Kind::Call, "Meet", 6),
        // Now, and the next call starts when it ends: "no break".
        ev(WORK, (10, 0), (11, 0), "Design review", Kind::Meeting, "Room 2", 4),
        ev(WORK, (11, 0), (11, 30), "1:1 with Sam", Kind::Call, "Meet", 1),
        ev(WORK, (14, 0), (15, 0), "Quarterly planning", Kind::Meeting, "Room 4", 6),
        ev(WORK, (15, 30), (16, 0), "Budget check-in", Kind::Call, "Teams", 2),
        // Not accepted yet: an outline bar, in a second lane.
        Event { reply: Reply::Open, ..ev(WORK, (15, 45), (16, 30), "Vendor demo", Kind::Call, "Zoom", 3) },
    ];
    let home = vec![ev(HOME, (12, 30), (13, 30), "Lunch", Kind::Event, "Corner café", 0)];
    let family = vec![
        ev(FAMILY, (17, 0), (17, 30), "School pick-up", Kind::Event, "", 0),
        ev(FAMILY, (18, 0), (19, 0), "Swimming lessons", Kind::Event, "City pool", 0),
    ];
    State {
        cals: vec![
            data(work, Vec::new()),
            data(home, vec![all_day(HOME, "Bin collection")]),
            data(family, vec![all_day(FAMILY, "School trip")]),
        ],
    }
}

/// A busy day at each size, and the details of a tapped event.
#[must_use]
pub fn shots() -> Vec<Shot> {
    let (s, set, local) = (state(), settings(), local());
    // All calendars on one day, as `widget::screen` does.
    let events: Vec<Event> = s.cals.iter().flat_map(|c| c.events.iter().cloned()).collect();
    let all_day: Vec<AllDay> = s.cals.iter().flat_map(|c| c.all_day.iter().cloned()).collect();
    let t = model::today(&events, &all_day, local.at(NOW));
    let tapped = t.timed.iter().position(|e| e.title == "Quarterly planning");
    vec![
        Shot {
            name: "fullscreen",
            caption: "The meeting now, the next call with no break, and the full day on the timeline.",
            width: 1280,
            height: 480,
            node: widget::full(&s, &set, &t, &local, NOW, None),
        },
        Shot {
            name: "large",
            caption: "The meeting now at the top, the next call below it, and the day timeline.",
            width: 638,
            height: 480,
            node: widget::large(&set, &t, None),
        },
        Shot {
            name: "medium",
            caption: "The meeting now, the time left, and the day timeline.",
            width: 638,
            height: 238,
            node: widget::medium(&set, &t),
        },
        Shot {
            name: "small",
            caption: "The meeting now, the time left, and the next event.",
            width: 317,
            height: 238,
            node: widget::small(&set, &t),
        },
        Shot {
            name: "detail",
            caption: "Tap a bar to see the time, place, guests and calendar of that event.",
            width: 1280,
            height: 480,
            node: widget::full(&s, &set, &t, &local, NOW, tapped),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock() {
        let local = local();
        assert_eq!(model::hhmm(local.at(NOW)), "10:42");
        assert_eq!(model::date_label(local.at(NOW).div_euclid(DAY)), "Wed 10 Jun");
    }

    #[test]
    fn shots_encode() {
        let shots = shots();
        for s in &shots {
            assert!(!bmc_wasm_sdk::encode(&s.node).is_empty(), "{}", s.name);
        }
        let mut sizes: Vec<(u32, u32)> = shots.iter().map(|s| (s.width, s.height)).collect();
        sizes.sort_unstable();
        sizes.dedup();
        assert_eq!(sizes, [(317, 238), (638, 238), (638, 480), (1280, 480)]);
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
