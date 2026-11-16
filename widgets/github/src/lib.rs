//! GitHub widget for the Braiins Deck.
//!
//! Reads the unread notifications every 60 s. Left: their count. Right: the newest three.
//! With no notifications it shows the change in total stars since local midnight, the total
//! stars, the followers and the top repo, read with one GraphQL request every 15 min.

mod widget {
    #[cfg(target_arch = "wasm32")]
    use std::cell::RefCell;
    #[cfg(target_arch = "wasm32")]
    use std::time::Duration;

    #[cfg(target_arch = "wasm32")]
    use bmc_wasm_sdk::format::{FormatTimeOpts, format_time, local_unix_secs_or_system};
    use bmc_wasm_sdk::*;

    const BG: Color = Color::from_hex(0x13_12_11);
    const BOX: Color = Color::from_hex(0x1D_1C_1A);
    const INK: Color = Color::from_hex(0xF2_ED_E4);
    const MUTE: Color = Color::from_hex(0x9C_95_8A);
    const ACC: Color = Color::from_hex(0xFF_6A_1A);
    const BLK: Color = Color::from_hex(0x0A_0A_0A);
    #[cfg(target_arch = "wasm32")]
    const PILL: Color = Color::from_hex(0x0A_0A_0A);

    #[derive(Clone, Copy)]
    enum Icon {
        Bell,
        Star,
    }

    #[cfg(target_arch = "wasm32")]
    const API: &str = "https://api.github.com";
    /// The host puts the bound token in place of the placeholder as the request leaves;
    /// the widget never sees it. GitHub refuses requests that have no User-Agent.
    #[cfg(target_arch = "wasm32")]
    const HEADERS: &str = "Accept: application/vnd.github+json\n\
        Authorization: Bearer {{ credential.github.token }}\n\
        X-GitHub-Api-Version: 2022-11-28\n\
        User-Agent: braiins-deck-github-widget";
    /// Stars and followers. Must not contain two opening braces in a row:
    /// the host reads that as a credential placeholder.
    /// Forks are included: a fork can have its own stars.
    /// No stargazer list: since July 2026 GitHub gives it only to tokens with write access.
    #[cfg(target_arch = "wasm32")]
    const STATS_QUERY: &str = r#"{"query":"query { viewer { followers { totalCount } repositories(first: 100, ownerAffiliations: OWNER, orderBy: { field: STARGAZERS, direction: DESC }) { nodes { name stargazerCount } } } }"}"#;

    #[cfg(target_arch = "wasm32")]
    const NOTIF_POLL_MS: u32 = 60_000;
    #[cfg(target_arch = "wasm32")]
    const STATS_POLL_MS: u32 = 15 * 60_000;
    /// One page of notifications. A full page shows as "50+".
    const PAGE: usize = 50;
    /// No good reply for this long: dim the screen and say how old the data is.
    #[cfg(target_arch = "wasm32")]
    const OFFLINE_AFTER_S: i64 = 300;

    // Screen layout (the manifest allows only the 1280 × 480 full screen).
    const SCREEN_W: f32 = 1280.0;
    const SCREEN_H: f32 = 480.0;
    const BORDER: f32 = 3.0;
    const BLOCK_X: f32 = 56.0;
    const BLOCK_Y: f32 = 56.0;
    const BLOCK_W: f32 = 300.0;
    const BLOCK_H: f32 = 360.0;
    const BLOCK_SHADOW: f32 = 10.0;
    const ROWS_X: f32 = 400.0;
    const ROWS_W: f32 = 808.0;
    const ROWS_TOP: f32 = 56.0;
    const ROWS_H: f32 = 360.0;
    const ROW_H: f32 = 106.0;
    const SOLO_H: f32 = 200.0;
    const ROW_GAP: f32 = 20.0;
    const ROW_SHADOW: f32 = 8.0;
    const ROW_PAD: f32 = 24.0;
    const CONTENT_X: f32 = ROWS_X + BORDER + ROW_PAD;
    const CONTENT_W: f32 = ROWS_W - 2.0 * (BORDER + ROW_PAD);
    /// `CONTENT_W` for text styles, which take whole pixels.
    const TEXT_W: u32 = 754;

    pub(crate) struct Notif {
        pub(crate) label: &'static str,
        /// CI and security: the label is orange.
        pub(crate) hot: bool,
        pub(crate) title: String,
        /// `repo #number`
        pub(crate) place: String,
        pub(crate) updated_at: i64,
    }

    pub(crate) struct Stats {
        /// Change in total stars since local midnight.
        pub(crate) today: i64,
        pub(crate) total: i64,
        pub(crate) followers: i64,
        pub(crate) followers_delta: Option<i64>,
        pub(crate) top: Option<(String, i64)>,
    }

    #[cfg(target_arch = "wasm32")]
    #[derive(Default)]
    struct State {
        /// The newest three unread notifications.
        notifs: Option<Vec<Notif>>,
        count: usize,
        /// Device time of the last good notifications reply.
        notifs_ok: i64,
        stats: Option<Stats>,
        /// Why the last reply failed, when it failed.
        error: Option<&'static str>,
        /// GitHub refused the token.
        auth_failed: bool,
    }

    #[cfg(target_arch = "wasm32")]
    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
        static POLLS: RefCell<Vec<PollHandle>> = const { RefCell::new(Vec::new()) };
    }

    #[cfg(target_arch = "wasm32")]
    fn now() -> i64 {
        SystemTime::now().unix_secs
    }

    #[cfg(target_arch = "wasm32")]
    fn token_bound() -> bool {
        credentials::current().is_bound("github")
    }

    // ── Data ────────────────────────────────────────────────────────────

    #[cfg(target_arch = "wasm32")]
    fn notifs_request(_handle: &PollHandle) -> Option<FetchSpec> {
        token_bound().then(|| {
            FetchSpec::get(format!("{API}/notifications?per_page={PAGE}"))
                .headers(HEADERS)
                .timeout(Duration::from_secs(10))
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn stats_request(_handle: &PollHandle) -> Option<FetchSpec> {
        token_bound().then(|| {
            FetchSpec::post(format!("{API}/graphql"))
                .headers(format!("{HEADERS}\nContent-Type: application/json"))
                .body(STATS_QUERY)
                .timeout(Duration::from_secs(15))
        })
    }

    /// The text for a refused reply, and whether the token is the cause.
    #[cfg(target_arch = "wasm32")]
    fn refusal(response: &FetchResponse) -> (&'static str, bool) {
        if response.outcome() == Some(FetchOutcome::Refused) {
            return ("The Deck blocked the request. Allow api.github.com for the token account.", true);
        }
        match response.status {
            401 => ("GitHub refused the token.", true),
            403 | 429 if response.header("x-ratelimit-remaining").as_deref() == Some("0") => {
                ("GitHub rate limit reached. Waiting.", false)
            }
            403 | 404 => ("The token cannot read notifications. Use a classic token with the notifications scope.", true),
            _ => ("Cannot reach GitHub.", false),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn on_notifs(handle: &PollHandle, response: &FetchResponse) {
        let parsed = if response.ok() { parse_notifs(&response.json()) } else { None };
        let ok = parsed.is_some();
        let (why, auth) = if ok { ("", false) } else { refusal(response) };
        STATE.with_borrow_mut(|s| {
            if let Some((list, count)) = parsed {
                s.notifs = Some(list);
                s.count = count;
                s.notifs_ok = now();
                s.error = None;
                s.auth_failed = false;
            } else {
                s.error = Some(why);
                s.auth_failed = auth;
            }
        });
        if ok {
            // GitHub asks clients to poll no faster than this.
            if let Some(secs) = response.header("x-poll-interval").and_then(|v| v.trim().parse::<u32>().ok()) {
                handle.set_interval(secs.clamp(60, 600) * 1000);
            }
        } else {
            log_warn!("github: notifications status {}", response.status);
            handle.retry();
        }
        request_frame();
    }

    #[cfg(target_arch = "wasm32")]
    fn on_stats(handle: &PollHandle, response: &FetchResponse) {
        let parsed = if response.ok() { parse_stats(&response.json(), now()) } else { None };
        if parsed.is_none() {
            log_warn!("github: stats status {}", response.status);
            handle.retry();
            return;
        }
        STATE.with_borrow_mut(|s| s.stats = parsed);
        request_frame();
    }

    /// `(newest three, count)`, or `None` when the reply is not a notifications list.
    #[cfg(target_arch = "wasm32")]
    fn parse_notifs(doc: &JsonDoc) -> Option<(Vec<Notif>, usize)> {
        if doc.kind("") != Some(JsonKind::Array) {
            return None;
        }
        let n = doc.len("")?;
        let mut list = Vec::with_capacity(3);
        for i in 0..n.min(3) {
            let at = |field: &str| doc.str(&format!("/{i}/{field}"));
            let subject = at("subject/type").unwrap_or_default();
            let (label, hot) = kind(&at("reason").unwrap_or_default(), &subject);
            let repo = at("repository/name").unwrap_or_default();
            let number = matches!(subject.as_str(), "PullRequest" | "Issue")
                .then(|| at("subject/url"))
                .flatten()
                .and_then(|url| url.rsplit('/').next().filter(|n| n.bytes().all(|b| b.is_ascii_digit())).map(str::to_owned));
            list.push(Notif {
                label,
                hot,
                title: at("subject/title").unwrap_or_default(),
                place: match number {
                    Some(n) if !n.is_empty() => format!("{repo} #{n}"),
                    _ => repo,
                },
                updated_at: at("updated_at").and_then(|t| parse_datetime(&t)).unwrap_or(0),
            });
        }
        Some((list, n))
    }

    pub(crate) fn kind(reason: &str, subject: &str) -> (&'static str, bool) {
        match reason {
            "review_requested" => ("Review", false),
            "mention" | "team_mention" => ("Mention", false),
            "assign" => ("Assigned", false),
            "comment" => ("Comment", false),
            "approval_requested" => ("Approval", false),
            "invitation" => ("Invite", false),
            "ci_activity" => ("CI", true),
            "security_alert" => ("Security", true),
            _ => match subject {
                "PullRequest" => ("Pull request", false),
                "Issue" => ("Issue", false),
                "Release" => ("Release", false),
                "Discussion" => ("Discussion", false),
                "Commit" => ("Commit", false),
                "CheckSuite" => ("CI", true),
                "RepositoryVulnerabilityAlert" | "RepositoryDependabotAlertsThread" => ("Security", true),
                _ => ("Update", false),
            },
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn parse_stats(doc: &JsonDoc, now: i64) -> Option<Stats> {
        let followers = doc.i64("/data/viewer/followers/totalCount")?;
        let n = doc.len("/data/viewer/repositories/nodes")?;
        let (mut total, mut top) = (0, None);
        for i in 0..n {
            let base = format!("/data/viewer/repositories/nodes/{i}");
            let stars = doc.i64(&format!("{base}/stargazerCount")).unwrap_or(0);
            total += stars;
            // The repos come sorted by stars, so the first one is the top repo.
            if top.is_none() && stars > 0 {
                top = doc.str(&format!("{base}/name")).map(|name| (name, stars));
            }
        }
        // Days start at midnight in the Deck's time zone (UTC if it has none).
        let today = local_unix_secs_or_system(&SystemTime { unix_secs: now }, None).div_euclid(86_400);
        // Stars: against the first count of today.
        let stars_then = daily_counts("stars", total, today).last().map_or(total, |&(_, c)| c);
        // Followers: against the oldest count of the last 7 days, only from an earlier day.
        let followers_delta = daily_counts("followers", followers, today)
            .first()
            .filter(|&&(d, _)| d < today)
            .map(|&(_, c)| followers - c);
        Some(Stats {
            today: total - stars_then,
            total,
            followers,
            followers_delta,
            top,
        })
    }

    /// The daily counts of the last 7 days (today and the 6 days before), oldest first,
    /// after adding today's count if there is none yet. The counts persist on the Deck
    /// under `key` as `day:count;day:count`. The first count of each day is kept,
    /// so it is the count near midnight.
    #[cfg(target_arch = "wasm32")]
    fn daily_counts(key: &str, count: i64, today: i64) -> Vec<(i64, i64)> {
        let old = kv::get_string(key).unwrap_or_default();
        let mut days: Vec<(i64, i64)> = old
            .split(';')
            .filter_map(|p| {
                let (d, c) = p.split_once(':')?;
                Some((d.parse().ok()?, c.parse().ok()?))
            })
            .filter(|&(d, _)| d >= today - 6 && d <= today)
            .collect();
        if days.last().is_none_or(|&(d, _)| d != today) {
            days.push((today, count));
        }
        let new = days.iter().map(|(d, c)| format!("{d}:{c}")).collect::<Vec<_>>().join(";");
        if new != old {
            kv::set(key, new.as_bytes());
        }
        days
    }

    // ── Text ────────────────────────────────────────────────────────────

    /// `now`, `12m`, `3h`, `2d`, `5w`
    fn ago(secs: i64) -> String {
        let m = secs.max(0) / 60;
        match m {
            0 => "now".to_string(),
            1..60 => format!("{m}m"),
            60..1440 => format!("{}h", m / 60),
            1440..10_080 => format!("{}d", m / 1440),
            _ => format!("{}w", m / 10_080),
        }
    }

    /// `1284` → `1,284`
    fn thousands(v: i64) -> String {
        let digits = v.unsigned_abs().to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        if v < 0 { format!("-{out}") } else { out }
    }

    // ── Screen ──────────────────────────────────────────────────────────

    #[cfg(target_arch = "wasm32")]
    fn message(msg: &str) -> Node {
        col(
            props!(width: SCREEN_W, height: SCREEN_H, background: BG, justify_content: Justify::Center, cross_align: CrossAlign::Center),
            [text(msg, style!(size: 26, color: MUTE, max_width: 900, align: TextAlign::Center))],
        )
    }

    /// The bell and star from the design, in a 24-unit box, drawn with shapes:
    /// `include_svg!` puts an asset section in the WASM that the Deck refuses
    /// unless a separate tool strips it.
    fn icon(d: &mut Vec<Draw>, which: Icon, x: f32, y: f32, size: f32, color: Color) {
        let s = size / 24.0;
        let at = |px: f32, py: f32| (x + px * s, y + py * s);
        let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| {
            (0..=12).map(move |i| {
                #[expect(clippy::cast_precision_loss, reason = "i is at most 12")]
                let t = from + (to - from) * i as f32 / 12.0;
                (cx + r * t.cos(), cy + r * t.sin())
            })
        };
        match which {
            Icon::Bell => {
                let pi = std::f32::consts::PI;
                let mut body = vec![at(6.0, 16.5)];
                body.extend(arc(12.0, 11.0, 6.0, pi, 2.0 * pi).map(|(px, py)| at(px, py)));
                body.extend([at(18.0, 16.5), at(19.6, 18.3), at(4.4, 18.3)]);
                d.push(Draw::path(body, 2.4 * s, color, true, Interpolation::Linear));
                let clapper = arc(12.0, 21.0, 2.0, 0.0, pi).map(|(px, py)| at(px, py)).collect();
                d.push(Draw::path(clapper, 2.4 * s, color, false, Interpolation::Linear));
            }
            Icon::Star => {
                const STAR: [(f32, f32); 10] = [
                    (12.0, 3.2), (14.7, 8.8), (20.8, 9.6), (16.3, 13.8), (17.4, 19.9),
                    (12.0, 17.0), (6.6, 19.9), (7.7, 13.8), (3.2, 9.6), (9.3, 8.8),
                ];
                d.push(Draw::fill_path(STAR.iter().map(|&(px, py)| at(px, py)).collect(), color, false));
            }
        }
    }

    /// A box with a border and a hard shadow, as two rectangles over a third.
    #[expect(clippy::too_many_arguments, reason = "a rectangle, a shadow and a fill")]
    fn boxed(d: &mut Vec<Draw>, x: f32, y: f32, w: f32, h: f32, shadow: f32, shadow_color: Color, fill: Color) {
        d.push(Draw::rect(x + shadow, y + shadow, w, h, shadow_color));
        d.push(Draw::rect(x, y, w, h, INK));
        d.push(Draw::rect(x + BORDER, y + BORDER, w - 2.0 * BORDER, h - 2.0 * BORDER, fill));
    }

    /// The orange block: an icon at the top, a big number at the bottom.
    fn block(d: &mut Vec<Draw>, which: Icon, icon_size: f32, value: &str) {
        boxed(d, BLOCK_X, BLOCK_Y, BLOCK_W, BLOCK_H, BLOCK_SHADOW, INK, ACC);
        icon(d, which, BLOCK_X + 29.0, BLOCK_Y + 29.0, icon_size, BLK);
        let size = match value.chars().count() {
            0 | 1 => 240,
            2 => 176,
            _ => 124,
        };
        d.push(Draw::text(
            BLOCK_X + BORDER + 20.0,
            BLOCK_Y + BLOCK_H - 26.0,
            value,
            style!(size: size, weight: FontWeight::BOLD, color: BLK, valign: VerticalAlign::Baseline),
        ));
    }

    /// Top edge of each box, with the stack centred on the block.
    fn rows_y(k: usize, h: f32) -> impl Iterator<Item = f32> {
        #[expect(clippy::cast_precision_loss, reason = "k is at most 3")]
        let total = k as f32 * h + (k.saturating_sub(1)) as f32 * ROW_GAP;
        let top = ROWS_TOP + (ROWS_H - total) / 2.0;
        #[expect(clippy::cast_precision_loss, reason = "i is at most 2")]
        let y = move |i: usize| top + i as f32 * (h + ROW_GAP);
        (0..k).map(y)
    }

    fn tag(label: &str, background: Color) -> Node {
        let pad = || col(props!(width: 10.0), Vec::new());
        row(
            props!(background: background, height: 22.0, cross_align: CrossAlign::Center),
            [
                pad(),
                text(label.to_uppercase(), style!(size: 14, weight: FontWeight::BOLD, color: BLK, line_height: 1.0)),
                pad(),
            ],
        )
    }

    /// Room for the repo name: the row less the label, the age and the gaps,
    /// with the label measured generously, so the age never leaves the row.
    fn place_width(label: &str) -> u32 {
        #[expect(clippy::cast_possible_truncation, reason = "labels are short")]
        let label_w = 20 + 11 * label.len() as u32;
        TEXT_W.saturating_sub(label_w + 24 + 60)
    }

    fn notif_box(n: &Notif, y: f32, h: f32, solo: bool, now: i64) -> Node {
        let big_title = solo && n.title.chars().count() <= 70;
        let title_style = if big_title {
            style!(size: 44, weight: FontWeight::BOLD, color: INK, line_height: 1.12, max_width: TEXT_W)
        } else {
            style!(size: 30, weight: FontWeight::BOLD, color: INK, line_height: 1.2, max_width: TEXT_W, text_overflow: TextOverflow::Ellipsis)
        };
        col(
            props!(inset_left: CONTENT_X, inset_top: y + BORDER, width: CONTENT_W, height: h - 2.0 * BORDER, justify_content: Justify::Center, gap: 8.0),
            [
                row(
                    props!(width: CONTENT_W, height: 24.0, cross_align: CrossAlign::Center, gap: 12.0),
                    [
                        tag(n.label, if n.hot { ACC } else { INK }),
                        col(
                            props!(flex: 1.0),
                            [text(&n.place, style!(size: 16, weight: FontWeight::BOLD, color: MUTE, max_width: place_width(n.label), text_overflow: TextOverflow::Ellipsis))],
                        ),
                        text(ago(now - n.updated_at), style!(size: 18, weight: FontWeight::BOLD, color: INK)),
                    ],
                ),
                text(&n.title, title_style),
            ],
        )
    }

    fn stat_box(y: f32, value: &str, label: &str, right: Option<Node>) -> Node {
        let mut children = vec![
            text(value, style!(size: 50, weight: FontWeight::BOLD, color: INK, line_height: 1.0, max_width: 470, text_overflow: TextOverflow::Ellipsis)),
            tag(label, INK),
            spacer(1.0),
        ];
        children.extend(right);
        row(
            props!(inset_left: CONTENT_X, inset_top: y + BORDER, width: CONTENT_W, height: ROW_H - 2.0 * BORDER, cross_align: CrossAlign::Center, gap: 18.0),
            children,
        )
    }

    fn accent(s: String) -> Node {
        text(s, style!(size: 18, weight: FontWeight::BOLD, color: ACC))
    }

    pub(crate) fn notifications_screen(list: &[Notif], count: usize, now: i64) -> Node {
        let shown = if count >= PAGE { format!("{PAGE}+") } else { count.to_string() };
        let solo = list.len() == 1;
        let h = if solo { SOLO_H } else { ROW_H };
        let mut d = Vec::new();
        block(&mut d, Icon::Bell, 44.0, &shown);
        let ys: Vec<f32> = rows_y(list.len(), h).collect();
        for &y in &ys {
            boxed(&mut d, ROWS_X, y, ROWS_W, h, ROW_SHADOW, ACC, BOX);
        }
        let mut nodes = vec![canvas(props!(inset_left: 0.0, inset_top: 0.0, width: SCREEN_W, height: SCREEN_H), d)];
        nodes.extend(list.iter().zip(&ys).map(|(n, &y)| notif_box(n, y, h, solo, now)));
        col(props!(width: SCREEN_W, height: SCREEN_H, background: BG), nodes)
    }

    pub(crate) fn quiet_screen(stats: Option<&Stats>) -> Node {
        let mut d = Vec::new();
        let today = match stats {
            Some(s) if s.today > 0 => format!("+{}", s.today),
            Some(s) => s.today.to_string(),
            None => "-".to_string(),
        };
        block(&mut d, Icon::Star, 48.0, &today);
        let ys: Vec<f32> = rows_y(3, ROW_H).collect();
        for &y in &ys {
            boxed(&mut d, ROWS_X, y, ROWS_W, ROW_H, ROW_SHADOW, ACC, BOX);
        }
        let dash = || "-".to_string();
        let delta = stats.and_then(|s| s.followers_delta).filter(|&v| v != 0).map(|v| if v > 0 { format!("+{v}") } else { v.to_string() });
        let top = stats.and_then(|s| s.top.as_ref());
        let top_stars = top.map(|&(_, stars)| {
            row(
                props!(cross_align: CrossAlign::Center, gap: 6.0),
                [accent(thousands(stars)), canvas(props!(width: 18.0, height: 18.0), {
                    let mut star = Vec::new();
                    icon(&mut star, Icon::Star, 0.0, 0.0, 18.0, ACC);
                    star
                })],
            )
        });
        col(
            props!(width: SCREEN_W, height: SCREEN_H, background: BG),
            [
                canvas(props!(inset_left: 0.0, inset_top: 0.0, width: SCREEN_W, height: SCREEN_H), d),
                stat_box(ys[0], &stats.map_or_else(dash, |s| thousands(s.total)), "Stars", None),
                stat_box(ys[1], &stats.map_or_else(dash, |s| thousands(s.followers)), "Followers", delta.map(accent)),
                stat_box(ys[2], &top.map_or_else(dash, |(name, _)| name.clone()), "Top repo", top_stars),
            ],
        )
    }

    #[cfg(target_arch = "wasm32")]
    fn screen(s: &State, now: i64) -> Node {
        if !token_bound() {
            return message("Add a GitHub token in the widget settings.");
        }
        if s.auth_failed {
            return message(s.error.unwrap_or("GitHub refused the token."));
        }
        let Some(list) = &s.notifs else {
            return message(s.error.unwrap_or("Loading..."));
        };
        let body = if list.is_empty() { quiet_screen(s.stats.as_ref()) } else { notifications_screen(list, s.count, now) };
        if now - s.notifs_ok <= OFFLINE_AFTER_S {
            return body;
        }
        let since = format_time(SystemTime { unix_secs: s.notifs_ok }, FormatTimeOpts::default());
        col(
            props!(width: SCREEN_W, height: SCREEN_H, background: BG),
            [
                dimmed(0.3, body),
                row(
                    props!(inset_left: 0.0, inset_bottom: 22.0, width: SCREEN_W, justify_content: Justify::Center),
                    [row(
                        props!(background: PILL, border_radius: 20.0, padding: 10.0),
                        [text(format!("No new data since {since}"), style!(size: 19, color: INK))],
                    )],
                ),
            ],
        )
    }

    // ── Widget lifecycle ────────────────────────────────────────────────

    #[cfg(target_arch = "wasm32")]
    fn refresh() {
        POLLS.with_borrow(|polls| polls.iter().for_each(PollHandle::invalidate));
        request_frame();
    }

    #[cfg(target_arch = "wasm32")]
    #[unsafe(no_mangle)]
    pub extern "C" fn init() {
        let notifs = register_poll(
            notifs_request,
            on_notifs,
            PollConfig {
                interval_ms: Some(NOTIF_POLL_MS),
                retry_ms: 30_000,
                ..Default::default()
            },
        );
        let stats = register_poll(
            stats_request,
            on_stats,
            PollConfig {
                interval_ms: Some(STATS_POLL_MS),
                retry_ms: 120_000,
                ..Default::default()
            },
        );
        POLLS.set(vec![notifs, stats]);
    }

    #[cfg(target_arch = "wasm32")]
    #[unsafe(no_mangle)]
    pub extern "C" fn on_params_update() {
        refresh();
    }

    #[cfg(target_arch = "wasm32")]
    #[unsafe(no_mangle)]
    pub extern "C" fn on_credentials_update() {
        STATE.set(State::default());
        refresh();
    }

    #[cfg(target_arch = "wasm32")]
    #[unsafe(no_mangle)]
    pub extern "C" fn on_system_update() {
        request_frame();
    }

    #[cfg(target_arch = "wasm32")]
    #[unsafe(no_mangle)]
    pub extern "C" fn render(_delta_ms: u32) {
        let vp = widget_viewport();
        let root = STATE.with_borrow(|s| screen(s, now()));
        let _ = render_ui(vp.width, vp.height, root);
        // The "12m" ages change once a minute; the data comes with the polls.
        request_frame_after(30_000);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod preview;
