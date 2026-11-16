//! Pure logic: dates, event kinds, what the screen shows and the timeline lanes.
//! No host calls, so `cargo test` runs it on the Mac.
//!
//! All times here are local wall-clock seconds: unix seconds plus the UTC offset.
//! A day number is local seconds divided by `DAY`.

pub const DAY: i64 = 86_400;

/// Less than this many minutes before the start, the dot pulses.
pub const SOON_MIN: i64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Has a Meet, Zoom or other video link.
    Call,
    /// Has other guests and no link.
    Meeting,
    /// All other items.
    Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Yes,
    /// Maybe, or not answered yet.
    Open,
    No,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub start: i64,
    pub end: i64,
    pub title: String,
    pub kind: Kind,
    /// "Meet", "Zoom", a room or a place. Empty when unknown.
    pub place: String,
    /// Other guests, rooms not counted.
    pub guests: u32,
    pub reply: Reply,
    /// Index of the calendar in the settings list.
    pub cal: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllDay {
    pub first: i64,
    /// The day after the last day.
    pub until: i64,
    pub title: String,
    pub cal: usize,
}

// ── Dates ───────────────────────────────────────────────────────────

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Day number of a date (1970-01-01 is 0).
#[must_use]
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `(year, month, day)` of a day number.
#[must_use]
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "month and day are small and positive")]
    let md = (m as u32, d as u32);
    (y, md.0, md.1)
}

/// 0 is Monday.
#[must_use]
pub fn weekday(day: i64) -> usize {
    #[expect(clippy::cast_possible_truncation, reason = "rem_euclid(7) is 0..7")]
    let wd = (day + 3).rem_euclid(7) as usize;
    wd
}

/// `Mon 16 Nov`
#[must_use]
pub fn date_label(day: i64) -> String {
    let (_, m, d) = civil_from_days(day);
    format!("{} {d} {}", WEEKDAYS[weekday(day)], MONTHS[m as usize - 1])
}

/// `Tue` within the next week, `23 Nov` after it.
#[must_use]
pub fn day_label(day: i64, today: i64) -> String {
    if day - today < 7 {
        WEEKDAYS[weekday(day)].to_string()
    } else {
        let (_, m, d) = civil_from_days(day);
        format!("{d} {}", MONTHS[m as usize - 1])
    }
}

/// Day number of `YYYY-MM-DD`.
#[must_use]
pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.get(..10)?.split('-');
    let y = it.next()?.parse().ok()?;
    let m = it.next()?.parse().ok()?;
    let d = it.next()?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m, d))
}

/// `2026-11-16T00:00:00Z`
#[must_use]
pub fn rfc3339_utc(unix: i64) -> String {
    let (y, m, d) = civil_from_days(unix.div_euclid(DAY));
    let s = unix.rem_euclid(DAY);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s % 3600 / 60, s % 60)
}

/// `09:05`
#[must_use]
pub fn hhmm(local: i64) -> String {
    let s = local.rem_euclid(DAY);
    format!("{:02}:{:02}", s / 3600, s % 3600 / 60)
}

/// Whole minutes from `from` to `to`, rounded up.
#[must_use]
pub fn minutes(from: i64, to: i64) -> i64 {
    (to - from + 59).div_euclid(60).max(0)
}

/// `18 min`, `1h`, `1h 18m`
#[must_use]
pub fn span(min: i64) -> String {
    match min {
        ..60 => format!("{min} min"),
        _ if min % 60 == 0 => format!("{}h", min / 60),
        _ => format!("{}h {:02}m", min / 60, min % 60),
    }
}

// ── Event kinds ─────────────────────────────────────────────────────

/// The kind of an event and the place to show for it.
#[must_use]
pub fn classify(meet_link: bool, conference: Option<&str>, location: &str, others: u32) -> (Kind, String) {
    let link = if meet_link { Some("Meet") } else { None }
        .or_else(|| conference.map(conference_label))
        .or_else(|| link_label(&location.to_ascii_lowercase()));
    if let Some(label) = link {
        return (Kind::Call, label.to_string());
    }
    let kind = if others > 0 { Kind::Meeting } else { Kind::Event };
    (kind, short_place(location))
}

fn conference_label(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("zoom") {
        "Zoom"
    } else if n.contains("teams") {
        "Teams"
    } else if n.contains("webex") {
        "Webex"
    } else if n.contains("meet") {
        "Meet"
    } else {
        "Call"
    }
}

fn link_label(location: &str) -> Option<&'static str> {
    [
        ("meet.google.com", "Meet"),
        ("zoom.us", "Zoom"),
        ("teams.microsoft", "Teams"),
        ("teams.live", "Teams"),
        ("webex.com", "Webex"),
        ("whereby.com", "Whereby"),
        ("meet.jit.si", "Jitsi"),
    ]
    .iter()
    .find(|(host, _)| location.contains(host))
    .map(|&(_, label)| label)
}

/// The first part of a location: `Room 3.12, Main St 1, Prague` → `Room 3.12`.
fn short_place(location: &str) -> String {
    let first = location.split([',', '\n']).next().unwrap_or("").trim();
    if first.starts_with("http") {
        return String::new();
    }
    first.chars().take(40).collect()
}

// ── What the screen shows ───────────────────────────────────────────

/// The events of one day, as seen at `now`.
#[derive(Debug)]
#[expect(clippy::struct_field_names, reason = "`today` is the day number, `now` the time")]
pub struct Today<'a> {
    pub now: i64,
    pub today: i64,
    /// Timed events that touch today, by start.
    pub timed: Vec<&'a Event>,
    pub all_day: Vec<&'a AllDay>,
    /// Running now. An accepted event comes before one that is not.
    pub current: Vec<&'a Event>,
    /// Today, not started yet.
    pub upcoming: Vec<&'a Event>,
    /// The first event after today.
    pub later: Option<&'a Event>,
}

#[must_use]
pub fn today<'a>(events: &'a [Event], all_day: &'a [AllDay], now: i64) -> Today<'a> {
    let today = now.div_euclid(DAY);
    let (from, to) = (today * DAY, (today + 1) * DAY);
    let mut sorted: Vec<&Event> = events.iter().filter(|e| e.end > e.start).collect();
    sorted.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    let timed: Vec<&Event> = sorted.iter().copied().filter(|e| e.start < to && e.end > from).collect();
    let mut current: Vec<&Event> = timed.iter().copied().filter(|e| e.start <= now && now < e.end).collect();
    current.sort_by_key(|e| (e.reply != Reply::Yes, e.start));
    Today {
        now,
        today,
        upcoming: timed.iter().copied().filter(|e| e.start > now).collect(),
        later: sorted.iter().copied().find(|e| e.start >= to),
        current,
        timed,
        all_day: all_day.iter().filter(|a| a.first <= today && today < a.until).collect(),
    }
}

/// The large block at the left.
#[derive(Debug, PartialEq, Eq)]
pub enum Hero<'a> {
    Now { event: &'a Event, left: i64, also: Vec<&'a Event> },
    Next { event: &'a Event, in_min: i64 },
    Free { until: Option<&'a Event>, empty_day: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SideLabel {
    Next,
    Then,
    /// The weekday or date of a later day.
    Day(String),
}

/// The smaller block at the right.
#[derive(Debug, PartialEq, Eq)]
pub struct Side<'a> {
    pub label: SideLabel,
    pub event: &'a Event,
    /// Starts when the current event ends.
    pub no_break: bool,
}

#[must_use]
pub fn plan<'a>(t: &Today<'a>) -> (Hero<'a>, Option<Side<'a>>) {
    let later = t.later.map(|event| Side {
        label: SideLabel::Day(day_label(event.start.div_euclid(DAY), t.today)),
        event,
        no_break: false,
    });
    if let Some((&event, also)) = t.current.split_first() {
        let side = match t.upcoming.first() {
            Some(&next) => Some(Side {
                label: SideLabel::Next,
                event: next,
                no_break: (0..60).contains(&(next.start - event.end)),
            }),
            None => later,
        };
        return (Hero::Now { event, left: minutes(t.now, event.end), also: also.to_vec() }, side);
    }
    if let Some(&event) = t.upcoming.first() {
        let side = match t.upcoming.get(1) {
            Some(&then) => Some(Side { label: SideLabel::Then, event: then, no_break: false }),
            None => later,
        };
        return (Hero::Next { event, in_min: minutes(t.now, event.start) }, side);
    }
    (Hero::Free { until: t.later, empty_day: t.timed.is_empty() }, later)
}

/// `until Tue 09:00`, `all day`, or nothing.
#[must_use]
pub fn free_text(until: Option<&Event>, empty_day: bool, today: i64) -> String {
    if empty_day {
        return "all day".to_string();
    }
    until.map_or_else(String::new, |e| format!("until {} {}", day_label(e.start.div_euclid(DAY), today), hhmm(e.start)))
}

// ── Timeline ────────────────────────────────────────────────────────

/// `(lane, lanes)` for each event: events that overlap share the bar height.
/// `events` must be sorted by start. At most 3 lanes.
#[must_use]
pub fn lanes(events: &[&Event]) -> Vec<(usize, usize)> {
    let mut out = vec![(0, 1); events.len()];
    let mut i = 0;
    while i < events.len() {
        let mut ends: Vec<i64> = Vec::new();
        let mut members = Vec::new();
        let mut cluster_end = events[i].end;
        let mut j = i;
        while j < events.len() && (j == i || events[j].start < cluster_end) {
            let e = events[j];
            let lane = ends.iter().position(|&end| end <= e.start).unwrap_or_else(|| {
                ends.push(i64::MIN);
                ends.len() - 1
            });
            ends[lane] = e.end;
            members.push((j, lane));
            cluster_end = cluster_end.max(e.end);
            j += 1;
        }
        let count = ends.len().min(3);
        for (k, lane) in members {
            out[k] = (lane.min(2), count);
        }
        i = j;
    }
    out
}

/// Words on at most two lines of `per_line` characters. A title that does not fit ends with "…".
#[must_use]
pub fn two_lines(title: &str, per_line: usize) -> (String, String) {
    let mut lines = [String::new(), String::new()];
    let mut line = 0;
    let mut cut = false;
    for word in title.split_whitespace() {
        let need = word.chars().count() + usize::from(!lines[line].is_empty());
        if lines[line].chars().count() + need > per_line {
            if line == 1 || lines[0].is_empty() {
                cut = true;
                break;
            }
            line = 1;
        }
        if !lines[line].is_empty() {
            lines[line].push(' ');
        }
        lines[line].push_str(word);
    }
    if lines[0].is_empty() {
        // One word longer than a line.
        lines[0] = title.chars().take(per_line).collect();
        cut = title.chars().count() > per_line;
    }
    if cut {
        let last = usize::from(!lines[1].is_empty());
        let keep = per_line.saturating_sub(1);
        let text: String = lines[last].chars().take(keep).collect();
        lines[last] = format!("{}…", text.trim_end());
    }
    let [a, b] = lines;
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(start: &str, end: &str, title: &str) -> Event {
        let t = |s: &str| {
            let (h, m) = s.split_once(':').unwrap();
            DAY * 20_773 + h.parse::<i64>().unwrap() * 3600 + m.parse::<i64>().unwrap() * 60
        };
        Event {
            start: t(start),
            end: t(end),
            title: title.into(),
            kind: Kind::Event,
            place: String::new(),
            guests: 0,
            reply: Reply::Yes,
            cal: 0,
        }
    }

    fn at(s: &str) -> i64 {
        ev(s, s, "").start
    }

    #[test]
    fn dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 11, 16), 20_773);
        assert_eq!(civil_from_days(20_773), (2026, 11, 16));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(date_label(20_773), "Mon 16 Nov");
        assert_eq!(day_label(20_774, 20_773), "Tue");
        assert_eq!(day_label(20_780, 20_773), "23 Nov");
        assert_eq!(parse_date("2026-11-16"), Some(20_773));
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(rfc3339_utc(20_773 * DAY + 3_723), "2026-11-16T01:02:03Z");
        assert_eq!(hhmm(at("09:05")), "09:05");
        assert_eq!(span(18), "18 min");
        assert_eq!(span(60), "1h");
        assert_eq!(span(243), "4h 03m");
        assert_eq!(minutes(0, 61), 2);
        assert_eq!(minutes(0, 60), 1);
    }

    #[test]
    fn kinds() {
        assert_eq!(classify(true, None, "", 0), (Kind::Call, "Meet".into()));
        assert_eq!(classify(false, Some("Zoom Meeting"), "", 3), (Kind::Call, "Zoom".into()));
        assert_eq!(classify(false, None, "https://us02web.zoom.us/j/1", 0), (Kind::Call, "Zoom".into()));
        assert_eq!(classify(false, None, "Room 3.12, Main St 1", 5), (Kind::Meeting, "Room 3.12".into()));
        assert_eq!(classify(false, None, "Lokál Dlouhá", 0), (Kind::Event, "Lokál Dlouhá".into()));
    }

    fn day() -> Vec<Event> {
        let mut v = vec![
            ev("09:00", "09:15", "Standup"),
            ev("10:30", "11:00", "Marketing sync"),
            ev("12:00", "13:00", "Lunch with Marco"),
            ev("14:00", "14:45", "Pool roadmap review"),
            ev("14:45", "15:15", "Partner call"),
            ev("15:30", "16:00", "1:1 Jan"),
            ev("15:45", "16:30", "Vendor demo"),
            ev("18:30", "20:00", "Climbing"),
        ];
        v[6].reply = Reply::Open;
        let mut tomorrow = ev("09:00", "09:15", "Standup");
        tomorrow.start += DAY;
        tomorrow.end += DAY;
        v.push(tomorrow);
        v
    }

    #[test]
    fn states() {
        let events = day();
        let t = today(&events, &[], at("10:42"));
        let (hero, side) = plan(&t);
        assert!(matches!(hero, Hero::Now { event, left: 18, .. } if event.title == "Marketing sync"));
        assert!(matches!(side, Some(Side { label: SideLabel::Next, no_break: false, event }) if event.title == "Lunch with Marco"));

        let t = today(&events, &[], at("11:42"));
        let (hero, side) = plan(&t);
        assert!(matches!(hero, Hero::Next { in_min: 18, .. }));
        assert!(matches!(side, Some(Side { label: SideLabel::Then, .. })));

        let t = today(&events, &[], at("14:20"));
        let (_, side) = plan(&t);
        assert!(matches!(side, Some(Side { no_break: true, .. })));

        let t = today(&events, &[], at("15:50"));
        let (hero, _) = plan(&t);
        assert!(matches!(hero, Hero::Now { event, ref also, .. } if event.title == "1:1 Jan" && also[0].title == "Vendor demo"));

        let t = today(&events, &[], at("21:10"));
        let (hero, side) = plan(&t);
        assert!(matches!(hero, Hero::Free { empty_day: false, until: Some(_) }));
        assert_eq!(free_text(Some(t.later.unwrap()), false, t.today), "until Tue 09:00");
        assert!(matches!(side, Some(Side { label: SideLabel::Day(ref d), .. }) if d == "Tue"));

        let t = today(&[], &[], at("10:00"));
        assert_eq!(plan(&t), (Hero::Free { until: None, empty_day: true }, None));
    }

    #[test]
    fn overlap_lanes() {
        let events = day();
        let t = today(&events, &[], at("08:00"));
        let l = lanes(&t.timed);
        assert_eq!(l[3], (0, 1));
        assert_eq!(l[4], (0, 1));
        assert_eq!(l[5], (0, 2));
        assert_eq!(l[6], (1, 2));
    }

    #[test]
    fn long_titles() {
        let (a, b) = two_lines("Quarterly business review with the mining partners and the Pool team", 27);
        assert_eq!(a, "Quarterly business review");
        assert!(b.ends_with('…') && b.chars().count() <= 27, "{b}");
        assert_eq!(two_lines("Short", 27), ("Short".into(), String::new()));
        let (a, _) = two_lines("Supercalifragilisticexpialidocious", 10);
        assert_eq!(a.chars().count(), 10);
    }
}
