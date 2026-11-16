//! Data, settings and drawing. Runs only on the Deck (wasm32).
//!
//! Data flow: the token poll trades the refresh token for an access token
//! (the host puts the secrets into the request body; the widget never sees them).
//! One poll for each calendar in the settings then reads that calendar's events
//! for today and the next 7 days with the access token.

use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Duration;

use bmc_wasm_sdk::format::{local_unix_secs_or_system, resolve_tz_offset};
use bmc_wasm_sdk::*;

use crate::model::{self, AllDay, DAY, Event, Hero, Kind, Reply, SOON_MIN, Side, SideLabel, Today, hhmm, span};

// ── Colours ─────────────────────────────────────────────────────────

const BG: Color = Color::from_hex(0x0F_10_12);
const INK: Color = Color::from_hex(0xF3_F4_F6);
const SOFT: Color = Color::from_hex(0xD4_D6_DB);
const META: Color = Color::from_hex(0x8B_8E_96);
const CHIP: Color = Color::from_hex(0x9A_9D_A5);
const MUTE: Color = Color::from_hex(0x6F_72_7A);
const LABEL: Color = Color::from_hex(0x5C_5F_66);
const RULE: Color = Color::from_hex(0x24_26_2B);
const TRACK: Color = Color::from_hex(0x1A_1C_21);
const ELAPSED: Color = Color::from_hex(0x13_14_17);
const ACC: Color = Color::from_hex(0xFF_7A_1A);
const WARN: Color = Color::from_hex(0xE0_A3_3A);

/// The colour choices in the settings.
const PALETTE: [(&str, u32); 9] = [
    ("blue", 0x5B_8C_FF),
    ("green", 0x3D_D6_8C),
    ("amber", 0xF5_A5_24),
    ("red", 0xF2_55_5A),
    ("violet", 0x9D_7B_FF),
    ("teal", 0x2D_D4_BF),
    ("yellow", 0xF5_D5_47),
    ("pink", 0xF4_72_B6),
    ("gray", 0x9C_A3_AF),
];

// ── Google ──────────────────────────────────────────────────────────

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const API: &str = "https://www.googleapis.com/calendar/v3/calendars";
/// The account holds `client_id=…&client_secret=…&refresh_token=…` (from tools/google-token.py).
/// The host puts it in place of the placeholder as the request leaves.
const TOKEN_BODY: &str = "{{ credential.google.token }}&grant_type=refresh_token";
/// Only the fields the screen uses: no descriptions, no guest names.
const FIELDS: &str = "items(summary,status,eventType,start,end,hangoutLink,location,\
    conferenceData/conferenceSolution/name,attendees(self,resource,responseStatus))";

const MAX_CALS: usize = 8;
const CAL_POLL_MS: u32 = 5 * 60_000;
/// Access tokens last 60 min.
const TOKEN_POLL_MS: u32 = 45 * 60_000;
/// No good reply for this long: show the time of the last data.
const OFFLINE_AFTER_S: i64 = 15 * 60;
/// A tapped event shows this long.
const DETAIL_S: i64 = 20;

// ── Settings ────────────────────────────────────────────────────────

struct Cal {
    name: String,
    id: String,
    color: Color,
}

struct Settings {
    cals: Vec<Cal>,
    all_day: bool,
    declined: bool,
    from_h: i64,
    to_h: i64,
}

impl Settings {
    fn read() -> Self {
        let p = params::current();
        let cals = p
            .get_list("calendars")
            .map(|list| {
                list.iter()
                    .filter_map(|v| {
                        let o = v.as_object()?;
                        let id = o.get("id")?.as_str()?.trim().to_string();
                        let name = o.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                        let hex = o
                            .get("color")
                            .and_then(|v| v.as_str())
                            .and_then(|c| PALETTE.iter().find(|(key, _)| *key == c))
                            .map_or(PALETTE[0].1, |&(_, hex)| hex);
                        (!id.is_empty()).then(|| Cal { name, id, color: Color::from_hex(hex) })
                    })
                    .take(MAX_CALS)
                    .collect()
            })
            .unwrap_or_default();
        let from_h = p.get_i32("day_start").unwrap_or(8).clamp(0, 23);
        let to_h = p.get_i32("day_end").unwrap_or(20).clamp(1, 24);
        let (from_h, to_h) = if to_h > from_h { (from_h, to_h) } else { (8, 20) };
        Self {
            cals,
            all_day: p.get_bool("all_day").unwrap_or(true),
            declined: p.get_bool("declined").unwrap_or(false),
            from_h: i64::from(from_h),
            to_h: i64::from(to_h),
        }
    }

    /// What the fetched data depends on.
    fn fetch_key(&self) -> String {
        let ids: Vec<&str> = self.cals.iter().map(|c| c.id.as_str()).collect();
        format!("{}|{}", ids.join("\n"), self.declined)
    }

    fn color(&self, slot: usize) -> Color {
        self.cals.get(slot).map_or(Color::from_hex(PALETTE[8].1), |c| c.color)
    }
}

// ── Time ────────────────────────────────────────────────────────────

fn now() -> i64 {
    SystemTime::now().unix_secs
}

/// Unix seconds to local wall-clock seconds, in the Deck's time zone.
struct Local {
    tz: Option<Tz>,
    fallback: i64,
}

impl Local {
    fn get() -> Self {
        let n = now();
        Self {
            tz: system::current().timezone().map(Tz::from_runtime),
            fallback: local_unix_secs_or_system(&SystemTime { unix_secs: n }, None) - n,
        }
    }

    fn at(&self, unix: i64) -> i64 {
        let offset = self.tz.as_ref().and_then(|tz| resolve_tz_offset(tz, unix)).map_or(self.fallback, i64::from);
        unix + offset
    }
}

// ── State ───────────────────────────────────────────────────────────

#[derive(Default)]
struct CalData {
    events: Vec<Event>,
    all_day: Vec<AllDay>,
    /// Unix time of the last good reply, 0 before the first.
    ok_at: i64,
    error: Option<&'static str>,
}

#[derive(Default)]
struct State {
    /// Access token and its unix expiry.
    token: Option<(String, i64)>,
    error: Option<&'static str>,
    /// Google refused the client or the refresh token.
    auth_failed: bool,
    cals: Vec<CalData>,
    fetch_key: String,
    /// Tapped event: its start, title, and the unix time of the tap.
    detail: Option<(i64, String, i64)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static TOKEN: RefCell<Option<PollHandle>> = const { RefCell::new(None) };
    static CALS: RefCell<Vec<PollHandle>> = const { RefCell::new(Vec::new()) };
}

fn bound() -> bool {
    credentials::current().is_bound("google")
}

fn slot_of(handle: &PollHandle) -> Option<usize> {
    CALS.with_borrow(|polls| polls.iter().position(|p| p.index() == handle.index()))
}

fn refresh_calendars() {
    CALS.with_borrow(|polls| polls.iter().for_each(PollHandle::invalidate));
}

fn refresh_token() {
    TOKEN.with_borrow(|t| t.iter().for_each(PollHandle::invalidate));
}

// ── Data ────────────────────────────────────────────────────────────

fn token_request(_handle: &PollHandle) -> Option<FetchSpec> {
    bound().then(|| {
        FetchSpec::post(TOKEN_URL)
            .headers("Content-Type: application/x-www-form-urlencoded\nAccept: application/json")
            .body(TOKEN_BODY)
            .timeout(Duration::from_secs(15))
    })
}

fn on_token(handle: &PollHandle, response: &FetchResponse) {
    let doc = response.json();
    let token = if response.ok() { doc.str("/access_token").zip(doc.i64("/expires_in")) } else { None };
    if let Some((token, secs)) = token {
        STATE.with_borrow_mut(|s| {
            s.token = Some((token, now() + secs - 120));
            s.error = None;
            s.auth_failed = false;
        });
        refresh_calendars();
    } else {
        log_warn!("calendar: token status {}", response.status);
        let (why, auth) = token_refusal(response, &doc);
        STATE.with_borrow_mut(|s| {
            s.error = Some(why);
            s.auth_failed = auth;
        });
        if auth {
            handle.retry_after(10 * 60_000);
        } else {
            handle.retry();
        }
    }
    request_frame();
}

fn token_refusal(response: &FetchResponse, doc: &JsonDoc) -> (&'static str, bool) {
    if response.outcome() == Some(FetchOutcome::Refused) {
        return ("The Deck blocked the request. Allow oauth2.googleapis.com for the Google account.", true);
    }
    match doc.str("/error").as_deref() {
        Some("invalid_grant") => ("Google refused the refresh token. Make a new one with tools/google-token.py.", true),
        Some("invalid_client" | "unauthorized_client") => ("Google refused the account value. Make a new one with tools/google-token.py.", true),
        _ => ("Cannot connect to Google.", false),
    }
}

fn cal_request(handle: &PollHandle) -> Option<FetchSpec> {
    let slot = slot_of(handle)?;
    let id = Settings::read().cals.get(slot)?.id.clone();
    let n = now();
    // A request is built when it is scheduled, up to one poll interval before it is sent.
    let token = STATE.with_borrow(|s| s.token.as_ref().filter(|(_, exp)| *exp > n + 360).map(|(t, _)| t.clone()))?;
    let local = Local::get();
    let midnight = local.at(n).div_euclid(DAY) * DAY - (local.at(n) - n);
    let url = format!(
        "{}?singleEvents=true&orderBy=startTime&maxResults=250&timeMin={}&timeMax={}&fields={FIELDS}",
        bmc_wasm_sdk::url::join_path_segments(API, &[&id, "events"]),
        model::rfc3339_utc(midnight),
        model::rfc3339_utc(midnight + 8 * DAY),
    );
    Some(
        FetchSpec::get(url)
            .headers(format!("Authorization: Bearer {token}\nAccept: application/json"))
            .timeout(Duration::from_secs(15)),
    )
}

fn on_cal(handle: &PollHandle, response: &FetchResponse) {
    let Some(slot) = slot_of(handle) else {
        return;
    };
    if response.status == 401 {
        // The access token expired early: get a new one; it restarts this poll.
        STATE.with_borrow_mut(|s| s.token = None);
        refresh_token();
        handle.retry();
        return;
    }
    let declined = Settings::read().declined;
    let parsed = if response.ok() { parse_events(&response.json(), slot, declined, &Local::get()) } else { None };
    let ok = parsed.is_some();
    if !ok {
        log_warn!("calendar: slot {} status {}", slot, response.status);
    }
    STATE.with_borrow_mut(|s| {
        let Some(c) = s.cals.get_mut(slot) else {
            return;
        };
        if let Some((events, all_day)) = parsed {
            c.events = events;
            c.all_day = all_day;
            c.ok_at = now();
            c.error = None;
        } else {
            c.error = Some(match response.status {
                404 => "not found",
                403 => "no access",
                _ => "cannot connect",
            });
        }
    });
    if !ok {
        handle.retry();
    }
    request_frame();
}

/// The events of one reply. `None` when the reply is not an events list.
fn parse_events(doc: &JsonDoc, slot: usize, declined: bool, local: &Local) -> Option<(Vec<Event>, Vec<AllDay>)> {
    if doc.kind("") != Some(JsonKind::Object) {
        return None;
    }
    let (mut events, mut all_day) = (Vec::new(), Vec::new());
    // With the `fields` filter, a day with no events has no `items` key.
    for i in 0..doc.len("/items").unwrap_or(0) {
        let at = |field: &str| doc.str(&format!("/items/{i}/{field}"));
        if at("status").as_deref() == Some("cancelled") || at("eventType").as_deref() == Some("workingLocation") {
            continue;
        }
        let (mut others, mut reply) = (0, Reply::Yes);
        for j in 0..doc.len(&format!("/items/{i}/attendees")).unwrap_or(0) {
            let field = |f: &str| format!("/items/{i}/attendees/{j}/{f}");
            if doc.bool(&field("self")) == Some(true) {
                reply = match doc.str(&field("responseStatus")).as_deref() {
                    Some("declined") => Reply::No,
                    Some("tentative" | "needsAction") => Reply::Open,
                    _ => Reply::Yes,
                };
            } else if doc.bool(&field("resource")) != Some(true) {
                others += 1;
            }
        }
        if reply == Reply::No && !declined {
            continue;
        }
        let title = at("summary").map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Busy".to_string());
        let date = |field: &str| at(field).and_then(|d| model::parse_date(&d));
        if let (Some(first), Some(until)) = (date("start/date"), date("end/date")) {
            all_day.push(AllDay { first, until: until.max(first + 1), title, cal: slot });
            continue;
        }
        let time = |field: &str| at(field).and_then(|t| parse_datetime(&t));
        let (Some(start), Some(end)) = (time("start/dateTime"), time("end/dateTime")) else {
            continue;
        };
        let (kind, place) = model::classify(
            at("hangoutLink").is_some(),
            at("conferenceData/conferenceSolution/name").as_deref(),
            &at("location").unwrap_or_default(),
            others,
        );
        events.push(Event { start: local.at(start), end: local.at(end), title, kind, place, guests: others, reply, cal: slot });
    }
    Some((events, all_day))
}

// ── Drawing helpers ─────────────────────────────────────────────────

/// Corner points of a rounded rectangle; `rl` rounds the left corners, `rr` the right ones.
fn rrect(x: f32, y: f32, w: f32, h: f32, rl: f32, rr: f32) -> Vec<(f32, f32)> {
    let limit = (w / 2.0).min(h / 2.0).max(0.0);
    let (rl, rr) = (rl.min(limit), rr.min(limit));
    let mut points = Vec::with_capacity(28);
    let mut corner = |cx: f32, cy: f32, r: f32, from: f32| {
        for i in 0..=6 {
            #[expect(clippy::cast_precision_loss, reason = "i is at most 6")]
            let a = from + FRAC_PI_2 * i as f32 / 6.0;
            points.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    };
    corner(x + rl, y + rl, rl, PI);
    corner(x + w - rr, y + rr, rr, 1.5 * PI);
    corner(x + w - rr, y + h - rr, rr, 0.0);
    corner(x + rl, y + h - rl, rl, 0.5 * PI);
    points
}

#[derive(Clone, Copy)]
enum Icon {
    Call,
    People,
    Pin,
    Clock,
    Overlap,
    Offline,
}

/// Icons from the design, in a 24-unit box, drawn with shapes:
/// `include_svg!` adds an asset section that the Deck refuses.
fn icon(which: Icon, size: f32, color: Color) -> Vec<Draw> {
    let s = size / 24.0;
    let p = |x: f32, y: f32| (x * s, y * s);
    let circle = |x: f32, y: f32, r: f32, c: Color| Draw::circle(x * s, y * s, r * s, c);
    let half_disc = |x: f32, y: f32, r: f32| {
        let mut points: Vec<(f32, f32)> = (0..=12)
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "i is at most 12")]
                let a = PI + PI * i as f32 / 12.0;
                ((x + r * a.cos()) * s, (y + r * a.sin()) * s)
            })
            .collect();
        points.push(p(x - r, y));
        points
    };
    let square = |x: f32, y: f32, w: f32| vec![p(x, y), p(x + w, y), p(x + w, y + w), p(x, y + w)];
    let line = |points: Vec<(f32, f32)>, closed: bool| Draw::path(points, 2.0 * s, color, closed, Interpolation::Linear);
    match which {
        Icon::Call => vec![
            Draw::fill_path(rrect(2.5 * s, 5.5 * s, 13.0 * s, 13.0 * s, 2.0 * s, 2.0 * s), color, false),
            Draw::fill_path(vec![p(17.0, 10.2), p(21.5, 7.5), p(21.5, 16.5), p(17.0, 13.8)], color, false),
        ],
        Icon::People => vec![
            circle(16.8, 8.0, 3.2, color),
            Draw::fill_path(half_disc(18.4, 20.5, 4.6), color, false),
            circle(9.0, 7.75, 3.75, color),
            Draw::fill_path(half_disc(9.0, 20.5, 7.5), color, false),
        ],
        Icon::Pin => vec![
            circle(12.0, 9.2, 7.4, color),
            Draw::fill_path(vec![p(5.4, 11.8), p(18.6, 11.8), p(12.0, 22.2)], color, false),
            circle(12.0, 9.2, 2.8, BG),
        ],
        Icon::Clock => vec![
            circle(12.0, 12.0, 10.0, color),
            circle(12.0, 12.0, 7.8, BG),
            line(vec![p(12.0, 6.8), p(12.0, 12.0), p(15.6, 14.1)], false),
        ],
        Icon::Overlap => vec![
            line(square(3.0, 3.0, 12.0), true),
            Draw::fill_path(square(9.0, 9.0, 12.0), BG, false),
            line(square(9.0, 9.0, 12.0), true),
        ],
        Icon::Offline => vec![
            circle(12.0, 12.0, 9.5, color),
            circle(12.0, 12.0, 7.5, BG),
            Draw::path(vec![p(4.5, 4.5), p(19.5, 19.5)], 2.4 * s, color, false, Interpolation::Linear),
        ],
    }
}

fn icon_node(which: Icon, size: f32, color: Color) -> Node {
    canvas(props!(width: size, height: size), icon(which, size, color))
}

fn dot(size: f32, color: Color, pulse_ms: Option<u32>) -> Node {
    let r = size / 2.0;
    let mut d = Draw::circle(r, r, r, color);
    if let Some(ms) = pulse_ms {
        d = d.animate(AnimProperty::Alpha, 1.0, 0.25, ms, Easing::EaseInOut, LoopMode::PingPong);
    }
    canvas(props!(width: size, height: size), [d])
}

fn gap(w: f32) -> Node {
    col(props!(width: w, height: 1.0), Vec::new())
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "widths are small and positive")]
fn label(s: impl Into<String>, size: u32, weight: FontWeight, color: Color, max_w: f32) -> Node {
    text(
        s.into(),
        style!(size: size, weight: weight, color: color, max_width: max_w as u32, text_overflow: TextOverflow::Ellipsis, line_height: 1.15),
    )
}

/// A node placed at `(x, y)`.
fn at(x: f32, y: f32, w: f32, children: impl IntoIterator<Item = Node>) -> Node {
    col(props!(inset_left: x, inset_top: y, width: w), children)
}

#[expect(clippy::cast_precision_loss, reason = "font sizes are small")]
fn em(size: u32) -> f32 {
    size as f32
}

// ── Screen pieces ───────────────────────────────────────────────────

struct Accent {
    text: String,
    /// Pulse period of the dot; no dot when `None`.
    pulse: Option<u32>,
}

fn accent(hero: &Hero) -> Option<Accent> {
    match hero {
        Hero::Now { left, .. } => Some(Accent { text: format!("{} left", span(*left)), pulse: Some(1000) }),
        Hero::Next { in_min, .. } => Some(Accent {
            text: format!("in {}", span(*in_min)),
            pulse: (*in_min <= SOON_MIN).then_some(450),
        }),
        Hero::Free { .. } => None,
    }
}

fn accent_node(a: &Accent, size: u32) -> Node {
    let f = em(size);
    let mut items = Vec::new();
    if let Some(ms) = a.pulse {
        items.push(dot(f * 0.42, ACC, Some(ms)));
    }
    items.push(label(a.text.as_str(), size, FontWeight::REGULAR, ACC, 300.0));
    row(props!(cross_align: CrossAlign::Center, gap: f * 0.38), items)
}

fn kind_icon(kind: Kind) -> Option<Icon> {
    match kind {
        Kind::Call => Some(Icon::Call),
        Kind::Meeting => Some(Icon::People),
        Kind::Event => None,
    }
}

/// `[icon] 10:30 – 11:00  [pin] Room 3.12  ● 18 min left`
fn meta_row(e: &Event, accent: Option<&Accent>, size: u32, place: bool) -> Node {
    let f = em(size);
    let mut items = Vec::new();
    if let Some(i) = kind_icon(e.kind) {
        items.push(icon_node(i, f, META));
    }
    items.push(label(format!("{} – {}", hhmm(e.start), hhmm(e.end)), size, FontWeight::REGULAR, META, 400.0));
    if place && e.kind != Kind::Call && !e.place.is_empty() {
        items.push(gap(f * 0.2));
        items.push(icon_node(Icon::Pin, f * 0.9, META));
        items.push(label(e.place.as_str(), size, FontWeight::REGULAR, META, 18.0 * f));
    }
    if let Some(a) = accent {
        items.push(gap(f * 0.3));
        items.push(accent_node(a, size));
    }
    row(props!(cross_align: CrossAlign::Center, gap: f * 0.42), items)
}

/// The title: one line at `size`, or two lines at `small` when it is long.
fn title(t: &str, size: u32, small: u32, width: f32) -> Node {
    // Braiins Sans SemiBold is about 0.53 em wide on average.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a small positive character count")]
    let per_line = |s: u32| (width / (em(s) * 0.53)) as usize;
    if t.chars().count() <= per_line(size) {
        return label(t, size, FontWeight::SEMIBOLD, INK, width);
    }
    let (a, b) = model::two_lines(t, per_line(small).saturating_sub(1));
    let mut lines = vec![label(a, small, FontWeight::SEMIBOLD, INK, width)];
    if !b.is_empty() {
        lines.push(label(b, small, FontWeight::SEMIBOLD, INK, width));
    }
    col(props!(width: width), lines)
}

/// Where the next event is, for the side block.
fn place_row(e: &Event, size: u32, width: f32) -> Option<Node> {
    if e.place.is_empty() {
        return None;
    }
    let f = em(size);
    let which = if e.kind == Kind::Call { Icon::Call } else { Icon::Pin };
    Some(row(
        props!(cross_align: CrossAlign::Center, gap: f * 0.4),
        [icon_node(which, f, MUTE), label(e.place.as_str(), size, FontWeight::REGULAR, MUTE, width - 1.5 * f)],
    ))
}

fn side_head(side: &Side, size: u32) -> Node {
    let name = match &side.label {
        SideLabel::Next => "Next".to_string(),
        SideLabel::Then => "Then".to_string(),
        SideLabel::Day(d) => d.clone(),
    };
    let mut items = vec![label(format!("{name} · {}", hhmm(side.event.start)), size, FontWeight::REGULAR, MUTE, 300.0)];
    if side.no_break {
        items.push(label("· no break", size, FontWeight::REGULAR, ACC, 200.0));
    }
    row(props!(cross_align: CrossAlign::Center, gap: em(size) * 0.3), items)
}

/// Overlapping events, below the title.
fn also_rows(also: &[&Event], size: u32, width: f32) -> Vec<Node> {
    let f = em(size);
    also.iter()
        .take(1)
        .map(|a| {
            let open = if a.reply == Reply::Yes { "" } else { " · not accepted" };
            row(
                props!(cross_align: CrossAlign::Center, gap: f * 0.5),
                [
                    icon_node(Icon::Overlap, f, MUTE),
                    label(format!("{} · {} – {}{open}", a.title, hhmm(a.start), hhmm(a.end)), size, FontWeight::REGULAR, MUTE, width - 1.5 * f),
                ],
            )
        })
        .collect()
}

// ── Timeline ────────────────────────────────────────────────────────

struct Track {
    x0: f32,
    x1: f32,
    pad: f32,
    y: f32,
    h: f32,
    r: f32,
    inset: f32,
    bar_r: f32,
    gap: f32,
    over: f32,
    line: f32,
    head: f32,
    label: u32,
    label_dy: f32,
    /// Hours between labels; `None`: only the first and the last.
    step: Option<i64>,
}

const FULL_TRACK: Track = Track {
    x0: 56.0, x1: 1224.0, pad: 8.0, y: 316.0, h: 56.0, r: 12.0, inset: 8.0, bar_r: 6.0, gap: 4.0,
    over: 14.0, line: 3.0, head: 6.0, label: 18, label_dy: 44.0, step: Some(2),
};
const LARGE_TRACK: Track = Track {
    x0: 40.0, x1: 598.0, pad: 6.0, y: 350.0, h: 48.0, r: 10.0, inset: 7.0, bar_r: 5.0, gap: 3.0,
    over: 12.0, line: 3.0, head: 5.0, label: 16, label_dy: 38.0, step: Some(4),
};
const MEDIUM_TRACK: Track = Track {
    x0: 32.0, x1: 606.0, pad: 6.0, y: 130.0, h: 40.0, r: 9.0, inset: 6.0, bar_r: 4.0, gap: 3.0,
    over: 10.0, line: 2.5, head: 4.0, label: 15, label_dy: 32.0, step: Some(4),
};
const SMALL_TRACK: Track = Track {
    x0: 24.0, x1: 293.0, pad: 4.0, y: 146.0, h: 30.0, r: 7.0, inset: 5.0, bar_r: 3.0, gap: 2.0,
    over: 7.0, line: 2.0, head: 0.0, label: 14, label_dy: 28.0, step: None,
};

/// The track, the bars, the now line and the hour labels.
/// Returns the draws and one touch area for each bar.
fn timeline(t: &Today, set: &Settings, g: &Track, selected: Option<usize>, touch: bool) -> (Vec<Draw>, Vec<Node>) {
    let r0 = t.today * DAY + set.from_h * 3600;
    let r1 = t.today * DAY + set.to_h * 3600;
    #[expect(clippy::cast_precision_loss, reason = "seconds in one day")]
    let x = |s: i64| g.x0 + (s.clamp(r0, r1) - r0) as f32 * (g.x1 - g.x0) / (r1 - r0) as f32;
    let (tx, tw) = (g.x0 - g.pad, g.x1 - g.x0 + 2.0 * g.pad);
    let mut d = vec![Draw::fill_path(rrect(tx, g.y, tw, g.h, g.r, g.r), TRACK, false)];
    if t.now > r0 {
        let full = t.now >= r1;
        let w = if full { tw } else { x(t.now) - tx };
        d.push(Draw::fill_path(rrect(tx, g.y, w, g.h, g.r, if full { g.r } else { 0.0 }), ELAPSED, false));
    }

    let mut touches = Vec::new();
    let lanes = model::lanes(&t.timed);
    let inner = g.h - 2.0 * g.inset;
    for (i, (e, &(lane, count))) in t.timed.iter().zip(&lanes).enumerate() {
        if e.end <= r0 || e.start >= r1 {
            continue;
        }
        #[expect(clippy::cast_precision_loss, reason = "at most 3 lanes")]
        let (lane, count) = (lane as f32, count as f32);
        let bh = (inner - g.gap * (count - 1.0)) / count;
        let by = g.y + g.inset + lane * (bh + g.gap);
        let bx = x(e.start) + 1.5;
        let bw = (x(e.end) - x(e.start) - 3.0).max(3.0);
        let base = set.color(e.cal);
        let color = match selected {
            Some(s) if s != i => base.mix(TRACK, 0.78),
            None if e.end <= t.now => base.mix(ELAPSED, 0.68),
            Some(_) | None => base,
        };
        let shape = rrect(bx, by, bw, bh, g.bar_r, g.bar_r);
        if e.reply == Reply::Yes {
            d.push(Draw::fill_path(shape, color, false));
        } else {
            d.push(Draw::fill_path(shape.clone(), color.mix(BG, 0.84), false));
            d.push(Draw::path(shape, 2.0, color, true, Interpolation::Linear));
        }
        if touch {
            let w = bw.max(44.0);
            touches.push(touchable(
                &format!("e{i}"),
                props!(inset_left: bx + bw / 2.0 - w / 2.0, inset_top: g.y - 20.0, width: w, height: g.h + 40.0),
                Vec::new(),
            ));
        }
    }

    if (r0..=r1).contains(&t.now) {
        let nx = x(t.now);
        d.push(Draw::rect(nx - g.line / 2.0, g.y - g.over, g.line, g.h + 2.0 * g.over, ACC));
        if g.head > 0.0 {
            d.push(Draw::circle(nx, g.y - g.over, g.head, ACC));
        }
    }

    let step = g.step.unwrap_or(set.to_h - set.from_h).max(1);
    let step = if g.step.is_some() && set.to_h - set.from_h > 14 { step * 2 } else { step };
    let mut hour = set.from_h;
    while hour <= set.to_h {
        let align = if hour == set.from_h {
            TextAlign::Left
        } else if hour == set.to_h {
            TextAlign::Right
        } else {
            TextAlign::Center
        };
        d.push(Draw::text(
            x(t.today * DAY + hour * 3600),
            g.y + g.h + g.label_dy,
            format!("{hour:02}"),
            style!(size: g.label, color: LABEL, align: align, valign: VerticalAlign::Baseline),
        ));
        hour += step;
    }
    (d, touches)
}

// ── Screens ─────────────────────────────────────────────────────────

/// What the top right shows: a calendar error, the offline time, the all-day events.
fn top_right(s: &State, set: &Settings, t: &Today, local: &Local, n: i64) -> Option<Node> {
    let mut items = Vec::new();
    if let Some((cal, why)) = set
        .cals
        .iter()
        .zip(&s.cals)
        .find_map(|(cal, data)| data.error.filter(|_| data.ok_at == 0).map(|why| (cal, why)))
    {
        items.push(label(format!("{}: {why}", cal.name), 20, FontWeight::REGULAR, WARN, 360.0));
    }
    let last = s.cals.iter().map(|c| c.ok_at).max().unwrap_or(0);
    if last > 0 && n - last > OFFLINE_AFTER_S {
        items.push(row(
            props!(cross_align: CrossAlign::Center, gap: 8.0),
            [icon_node(Icon::Offline, 20.0, WARN), label(hhmm(local.at(last)), 20, FontWeight::REGULAR, WARN, 100.0)],
        ));
    }
    if set.all_day {
        for a in t.all_day.iter().take(2) {
            items.push(row(
                props!(cross_align: CrossAlign::Center, gap: 10.0),
                [dot(8.0, set.color(a.cal), None), label(a.title.as_str(), 20, FontWeight::REGULAR, CHIP, 300.0)],
            ));
        }
        if t.all_day.len() > 2 {
            items.push(label(format!("+{}", t.all_day.len() - 2), 20, FontWeight::REGULAR, CHIP, 80.0));
        }
    }
    (!items.is_empty()).then(|| row(props!(inset_right: 56.0, inset_top: 44.0, gap: 28.0, cross_align: CrossAlign::Center), items))
}

fn root(w: f32, h: f32, children: Vec<Node>) -> Node {
    col(props!(width: w, height: h, background: BG), children)
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "the screen width in pixels")]
fn message(w: f32, h: f32, msg: &str) -> Node {
    let size = if w < 640.0 { 18 } else { 26 };
    col(
        props!(width: w, height: h, background: BG, justify_content: Justify::Center, cross_align: CrossAlign::Center, padding: 24.0),
        [text(msg, style!(size: size, color: META, max_width: (w as u32).saturating_sub(48).min(900), align: TextAlign::Center))],
    )
}

/// The left block on the full and large screens.
#[expect(clippy::too_many_arguments, reason = "font sizes, width and options")]
fn hero_nodes(hero: &Hero, detail: Option<(&Event, &Settings, i64)>, size: u32, small: u32, meta: u32, width: f32, t: &Today, overlaps: bool) -> Vec<Node> {
    if let Some((e, set, n)) = detail {
        let f = em(meta);
        let mut items = vec![icon_node(Icon::Clock, f, META), label(format!("{} – {}", hhmm(e.start), hhmm(e.end)), meta, FontWeight::REGULAR, META, 300.0)];
        if !e.place.is_empty() {
            items.push(gap(f * 0.2));
            items.push(icon_node(if e.kind == Kind::Call { Icon::Call } else { Icon::Pin }, f, META));
            items.push(label(e.place.as_str(), meta, FontWeight::REGULAR, META, 300.0));
        }
        if e.guests > 0 {
            items.push(gap(f * 0.2));
            items.push(icon_node(Icon::People, f, META));
            items.push(label(e.guests.to_string(), meta, FontWeight::REGULAR, META, 80.0));
        }
        if let Some(cal) = set.cals.get(e.cal).filter(|c| !c.name.is_empty()) {
            items.push(gap(f * 0.2));
            items.push(dot(f * 0.42, cal.color, None));
            items.push(label(cal.name.as_str(), meta, FontWeight::REGULAR, META, 200.0));
        }
        let when = if e.start > n {
            Some(format!("in {}", span(model::minutes(n, e.start))))
        } else if e.end > n {
            Some(format!("{} left", span(model::minutes(n, e.end))))
        } else {
            None
        };
        if let Some(when) = when {
            items.push(gap(f * 0.3));
            items.push(label(when, meta, FontWeight::REGULAR, ACC, 200.0));
        }
        return vec![title(&e.title, size, small, width), row(props!(cross_align: CrossAlign::Center, gap: f * 0.42), items)];
    }
    let a = accent(hero);
    match hero {
        Hero::Now { event, also, .. } => {
            let mut v = vec![title(&event.title, size, small, width), meta_row(event, a.as_ref(), meta, true)];
            if overlaps {
                v.extend(also_rows(also, meta * 5 / 6, width));
            }
            v
        }
        Hero::Next { event, .. } => vec![title(&event.title, size, small, width), meta_row(event, a.as_ref(), meta, true)],
        Hero::Free { until, empty_day } => {
            let sub = model::free_text(*until, *empty_day, t.today);
            let mut v = vec![label("Free", size, FontWeight::SEMIBOLD, INK, width)];
            if !sub.is_empty() {
                v.push(label(sub, meta, FontWeight::REGULAR, META, width));
            }
            v
        }
    }
}

fn full(s: &State, set: &Settings, t: &Today, local: &Local, n: i64, selected: Option<usize>) -> Node {
    let (hero, side) = model::plan(t);
    let detail = selected.map(|i| (t.timed[i], set, n));
    let (draws, touches) = timeline(t, set, &FULL_TRACK, selected, true);
    let mut nodes = vec![canvas(props!(inset_left: 0.0, inset_top: 0.0, width: 1280.0, height: 480.0), draws)];
    if selected.is_some() {
        nodes.push(touchable("back", props!(inset_left: 0.0, inset_top: 0.0, width: 1280.0, height: 480.0), Vec::new()));
    }
    nodes.push(at(56.0, 44.0, 600.0, [label(model::date_label(t.today), 22, FontWeight::REGULAR, MUTE, 600.0)]));
    nodes.extend(top_right(s, set, t, local, n));
    let side = side.filter(|_| detail.is_none());
    let width = if side.is_some() { 704.0 } else { 1168.0 };
    nodes.push(col(
        props!(inset_left: 56.0, inset_top: 112.0, width: width, gap: 14.0),
        hero_nodes(&hero, detail, 64, 46, 24, width, t, true),
    ));
    if let Some(side) = side {
        let mut block = vec![side_head(&side, 20), label(side.event.title.as_str(), 34, FontWeight::SEMIBOLD, SOFT, 384.0)];
        block.extend(place_row(side.event, 20, 384.0));
        nodes.push(col(props!(inset_left: 840.0, inset_top: 120.0, width: 384.0, gap: 10.0), block));
    }
    nodes.extend(touches);
    root(1280.0, 480.0, nodes)
}

fn large(set: &Settings, t: &Today, n: i64, selected: Option<usize>) -> Node {
    let (hero, side) = model::plan(t);
    let detail = selected.map(|i| (t.timed[i], set, n));
    let (draws, touches) = timeline(t, set, &LARGE_TRACK, selected, true);
    let mut draws = draws;
    let side = side.filter(|_| detail.is_none());
    if side.is_some() {
        draws.push(Draw::rect(40.0, 196.0, 558.0, 1.0, RULE));
    }
    let mut nodes = vec![canvas(props!(inset_left: 0.0, inset_top: 0.0, width: 638.0, height: 480.0), draws)];
    if selected.is_some() {
        nodes.push(touchable("back", props!(inset_left: 0.0, inset_top: 0.0, width: 638.0, height: 480.0), Vec::new()));
    }
    nodes.push(at(40.0, 32.0, 558.0, [label(model::date_label(t.today), 18, FontWeight::REGULAR, MUTE, 558.0)]));
    nodes.push(col(props!(inset_left: 40.0, inset_top: 76.0, width: 558.0, gap: 10.0), hero_nodes(&hero, detail, 46, 34, 20, 558.0, t, false)));
    if let Some(side) = side {
        let mut block = vec![side_head(&side, 18), label(side.event.title.as_str(), 28, FontWeight::SEMIBOLD, SOFT, 558.0)];
        block.extend(place_row(side.event, 18, 558.0));
        nodes.push(col(props!(inset_left: 40.0, inset_top: 218.0, width: 558.0, gap: 8.0), block));
    }
    nodes.extend(touches);
    root(638.0, 480.0, nodes)
}

/// One line: what is running or next, or when the day is free.
fn short_lines(hero: &Hero, t: &Today) -> (String, Option<Accent>, Option<String>) {
    match hero {
        Hero::Now { event, .. } | Hero::Next { event, .. } => (event.title.clone(), accent(hero), None),
        Hero::Free { until, empty_day } => ("Free".to_string(), None, Some(model::free_text(*until, *empty_day, t.today))),
    }
}

fn medium(set: &Settings, t: &Today) -> Node {
    let (hero, _) = model::plan(t);
    let (draws, _) = timeline(t, set, &MEDIUM_TRACK, None, false);
    let mut block = vec![];
    match &hero {
        Hero::Now { event, .. } | Hero::Next { event, .. } => {
            block.push(label(event.title.as_str(), 36, FontWeight::SEMIBOLD, INK, 574.0));
            block.push(meta_row(event, accent(&hero).as_ref(), 18, false));
        }
        Hero::Free { .. } => {
            let (head, _, sub) = short_lines(&hero, t);
            block.push(label(head, 36, FontWeight::SEMIBOLD, INK, 574.0));
            block.extend(sub.filter(|s| !s.is_empty()).map(|s| label(s, 18, FontWeight::REGULAR, META, 574.0)));
        }
    }
    root(
        638.0,
        238.0,
        vec![
            canvas(props!(inset_left: 0.0, inset_top: 0.0, width: 638.0, height: 238.0), draws),
            col(props!(inset_left: 32.0, inset_top: 24.0, width: 574.0, gap: 8.0), block),
        ],
    )
}

fn small(set: &Settings, t: &Today) -> Node {
    let (hero, side) = model::plan(t);
    let (draws, _) = timeline(t, set, &SMALL_TRACK, None, false);
    let (head, acc, sub) = short_lines(&hero, t);
    let mut block = vec![label(head, 26, FontWeight::SEMIBOLD, INK, 273.0)];
    if let Some(a) = acc {
        block.push(accent_node(&a, 16));
    }
    if let Some(sub) = sub.filter(|s| !s.is_empty()) {
        block.push(label(sub, 16, FontWeight::REGULAR, META, 273.0));
    }
    if let Some(side) = side.filter(|_| matches!(hero, Hero::Now { .. } | Hero::Next { .. })) {
        block.push(label(format!("{}  {}", hhmm(side.event.start), side.event.title), 15, FontWeight::REGULAR, MUTE, 273.0));
    }
    root(
        317.0,
        238.0,
        vec![
            canvas(props!(inset_left: 0.0, inset_top: 0.0, width: 317.0, height: 238.0), draws),
            col(props!(inset_left: 22.0, inset_top: 20.0, width: 273.0, gap: 8.0), block),
        ],
    )
}

/// Before the first data: the date and an empty track.
fn loading(set: &Settings, variant: SizeVariant, t: &Today) -> Node {
    let (track, w, h) = match variant {
        SizeVariant::Full => (&FULL_TRACK, 1280.0, 480.0),
        SizeVariant::Large => (&LARGE_TRACK, 638.0, 480.0),
        SizeVariant::Medium => (&MEDIUM_TRACK, 638.0, 238.0),
        SizeVariant::Small => (&SMALL_TRACK, 317.0, 238.0),
    };
    let (draws, _) = timeline(t, set, track, None, false);
    let mut nodes = vec![canvas(props!(inset_left: 0.0, inset_top: 0.0, width: w, height: h), draws)];
    match variant {
        SizeVariant::Full => nodes.push(at(56.0, 44.0, 600.0, [label(model::date_label(t.today), 22, FontWeight::REGULAR, MUTE, 600.0)])),
        SizeVariant::Large => nodes.push(at(40.0, 32.0, 558.0, [label(model::date_label(t.today), 18, FontWeight::REGULAR, MUTE, 558.0)])),
        SizeVariant::Medium | SizeVariant::Small => {}
    }
    root(w, h, nodes)
}

/// The screen with no calendar in the settings.
fn no_calendar(variant: SizeVariant, w: f32, h: f32) -> Node {
    match variant {
        SizeVariant::Full => root(
            w,
            h,
            vec![col(
                props!(inset_left: 56.0, inset_top: 112.0, width: 1168.0, gap: 14.0),
                [
                    label("Add a calendar", 64, FontWeight::SEMIBOLD, INK, 1168.0),
                    label("Widget settings › Calendars", 24, FontWeight::REGULAR, META, 1168.0),
                ],
            )],
        ),
        _ => message(w, h, "Add a calendar in the widget settings."),
    }
}

fn screen(s: &State, set: &Settings, variant: SizeVariant, local: &Local, n: i64) -> Node {
    #[expect(clippy::cast_precision_loss, reason = "the screen size in pixels")]
    let (w, h) = (variant.width() as f32, variant.height() as f32);
    if !bound() {
        return message(w, h, "Add the Google account in the widget settings.");
    }
    if set.cals.is_empty() {
        return no_calendar(variant, w, h);
    }
    if s.auth_failed {
        return message(w, h, s.error.unwrap_or("Google refused the account."));
    }
    if s.cals.iter().all(|c| c.ok_at == 0)
        && let Some(why) = s.error
    {
        return message(w, h, why);
    }
    let mut events = Vec::new();
    let mut all_day = Vec::new();
    for c in s.cals.iter().take(set.cals.len()) {
        events.extend(c.events.iter().cloned());
        all_day.extend(c.all_day.iter().cloned());
    }
    let t = model::today(&events, &all_day, local.at(n));
    if s.cals.iter().take(set.cals.len()).all(|c| c.ok_at == 0) {
        return loading(set, variant, &t);
    }
    let selected = s
        .detail
        .as_ref()
        .filter(|(_, _, opened)| n - opened < DETAIL_S)
        .and_then(|(start, title, _)| t.timed.iter().position(|e| e.start == *start && &e.title == title));
    match variant {
        SizeVariant::Full => full(s, set, &t, local, n, selected),
        SizeVariant::Large => large(set, &t, n, selected),
        SizeVariant::Medium => medium(set, &t),
        SizeVariant::Small => small(set, &t),
    }
}

/// The tapped event, to show its details, from the bar keys `e<index>`.
/// `None`: no tap. `Some(None)`: close the details. `Some(Some(..))`: show this event.
#[expect(clippy::option_option, reason = "three outcomes, documented above")]
fn tapped(result: &TreeRenderResult, s: &State, set: &Settings, local: &Local, n: i64) -> Option<Option<(i64, String, i64)>> {
    let bar = result.clicks.keys().find_map(|k| k.strip_prefix('e')?.parse::<usize>().ok());
    if bar.is_none() && !result.clicks.contains_key("back") {
        return None;
    }
    let Some(i) = bar else {
        return Some(None);
    };
    let mut events = Vec::new();
    for c in s.cals.iter().take(set.cals.len()) {
        events.extend(c.events.iter().cloned());
    }
    let t = model::today(&events, &[], local.at(n));
    let e = t.timed.get(i)?;
    let same = s.detail.as_ref().is_some_and(|(start, title, _)| *start == e.start && *title == e.title);
    // A second tap on the same bar goes back.
    Some((!same).then(|| (e.start, e.title.clone(), n)))
}

// ── Widget lifecycle ────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn init() {
    let token = register_poll(
        token_request,
        on_token,
        PollConfig { interval_ms: Some(TOKEN_POLL_MS), retry_ms: 60_000, ..Default::default() },
    );
    // One poll for each settings slot. A slot with no calendar sends nothing.
    let cals: Vec<PollHandle> = (0..MAX_CALS)
        .map(|_| {
            register_poll(
                cal_request,
                on_cal,
                PollConfig { interval_ms: Some(CAL_POLL_MS), retry_ms: 60_000, ..Default::default() },
            )
        })
        .collect();
    TOKEN.set(Some(token));
    CALS.set(cals);
    STATE.with_borrow_mut(|s| {
        s.cals = (0..MAX_CALS).map(|_| CalData::default()).collect();
        s.fetch_key = Settings::read().fetch_key();
    });
    // The calendar polls start when the first access token arrives.
}

#[unsafe(no_mangle)]
pub extern "C" fn on_params_update() {
    let key = Settings::read().fetch_key();
    let changed = STATE.with_borrow_mut(|s| {
        s.detail = None;
        if s.fetch_key == key {
            return false;
        }
        s.fetch_key = key;
        s.cals.iter_mut().for_each(|c| *c = CalData::default());
        true
    });
    if changed {
        refresh_calendars();
    }
    request_frame();
}

#[unsafe(no_mangle)]
pub extern "C" fn on_credentials_update() {
    STATE.with_borrow_mut(|s| {
        s.token = None;
        s.error = None;
        s.auth_failed = false;
        s.cals.iter_mut().for_each(|c| *c = CalData::default());
    });
    refresh_token();
    request_frame();
}

#[unsafe(no_mangle)]
pub extern "C" fn on_system_update() {
    request_frame();
}

#[unsafe(no_mangle)]
pub extern "C" fn on_touch() {
    request_frame();
}

#[unsafe(no_mangle)]
pub extern "C" fn render(_delta_ms: u32) {
    let vp = widget_viewport();
    let variant = widget_size().variant;
    let set = Settings::read();
    let local = Local::get();
    let n = now();
    let node = STATE.with_borrow(|s| screen(s, &set, variant, &local, n));
    let result = render_ui(vp.width, vp.height, node);

    let tap = STATE.with_borrow(|s| tapped(&result, s, &set, &local, n));
    if let Some(detail) = tap {
        STATE.with_borrow_mut(|s| s.detail = detail);
        request_frame();
        return;
    }
    // Times show whole minutes: draw again at the next minute, or when the details close.
    let mut wait = (60 - n.rem_euclid(60)) * 1000 + 50;
    if let Some((_, _, opened)) = STATE.with_borrow(|s| s.detail.clone()) {
        let left = (opened + DETAIL_S - n) * 1000;
        if left > 0 {
            wait = wait.min(left + 50);
        } else {
            STATE.with_borrow_mut(|s| s.detail = None);
        }
    }
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "at most about 60 000 ms")]
    request_frame_after(wait.max(50) as u32);
}
