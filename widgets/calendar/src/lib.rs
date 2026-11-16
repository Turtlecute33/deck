//! Calendar widget for the Braiins Deck: today's events from Google Calendar on one line.
//!
//! The current or next event is large at the top. The full day is a timeline below it.

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod model;

#[cfg(target_arch = "wasm32")]
mod widget;
