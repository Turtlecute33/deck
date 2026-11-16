//! Claude quota widget for the Braiins Deck.
//!
//! Reads the LAN quota server every 60 s. Left: the 5-hour and weekly use with
//! the time to reset. Right: the weekly use since the last weekly reset.

#[cfg(target_arch = "wasm32")]
mod widget {
    use std::cell::RefCell;
    use std::time::Duration;

    use bmc_wasm_sdk::format::{FormatTimeOpts, format_time, local_unix_secs_or_system, strftime};
    use bmc_wasm_sdk::*;

    const BG: Color = Color::from_hex(0x14_14_13);
    const INK: Color = Color::from_hex(0xF4_F3_EE);
    const MUTE: Color = Color::from_hex(0x8E_8B_83);
    const DIM: Color = Color::from_hex(0x5B_59_53);
    const TRACK: Color = Color::from_hex(0x26_26_24);
    const ACC: Color = Color::from_hex(0xD9_77_57);
    const GRID: Color = Color::from_hex(0x1F_1F_1D);
    const RULE: Color = Color::from_hex(0x2C_2B_28);
    const PILL: Color = Color::from_hex(0x0A_0A_0A);
    const PILL_TEXT: Color = Color::from_hex(0xD6_D3_CB);

    /// The host puts the bound key here as the request leaves; the widget never sees it.
    const KEY: &str = "{{ credential.server.token }}";
    const POLL_MS: u32 = 60_000;
    /// No good reply for this long: dim the screen and say how old the data is.
    const OFFLINE_AFTER_S: i64 = 180;
    const WEEK_S: i64 = 7 * 86_400;

    // Screen layout (the manifest allows only the 1280 × 480 full screen).
    const SCREEN_W: f32 = 1280.0;
    const SCREEN_H: f32 = 480.0;
    const METER_W: f32 = 384.0;
    const CHART_X: f32 = 520.0;
    const CHART_W: f32 = 696.0;
    const CHART_TOP: f32 = 130.0;
    const CHART_BOT: f32 = 350.0;

    #[derive(Clone, Copy)]
    struct Window {
        pct: f64,
        resets_at: i64,
    }

    struct Quota {
        updated_at: i64,
        server_stale: bool,
        five: Option<Window>,
        weekly: Option<Window>,
        week_start: i64,
        /// `(seconds since week_start, pct)`
        points: Vec<(i64, f64)>,
    }

    #[derive(Default)]
    struct State {
        quota: Option<Quota>,
        /// Device time of the last good reply.
        last_ok: i64,
        /// The last reply failed.
        failed: bool,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
        static POLL: RefCell<Option<PollHandle>> = const { RefCell::new(None) };
    }

    fn now() -> i64 {
        SystemTime::now().unix_secs
    }

    fn key_bound() -> bool {
        credentials::current().is_bound("server")
    }

    /// The quota server address from the settings. There is no default.
    fn server_url() -> Option<String> {
        let p = params::current();
        p.get_str("server_url")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('/').to_string())
    }

    // ── Data ────────────────────────────────────────────────────────────

    fn build_request(_handle: &PollHandle) -> Option<FetchSpec> {
        if !key_bound() {
            return None;
        }
        let server = server_url()?;
        Some(
            FetchSpec::get(format!("{server}/v1/quota"))
                .headers(format!("Authorization: Bearer {KEY}"))
                .timeout(Duration::from_secs(5)),
        )
    }

    fn on_reply(handle: &PollHandle, response: &FetchResponse) {
        let parsed = if response.ok() { parse(&response.json()) } else { None };
        let ok = parsed.is_some();
        STATE.with_borrow_mut(|s| {
            if let Some(q) = parsed {
                s.quota = Some(q);
                s.last_ok = now();
                s.failed = false;
            } else {
                s.failed = true;
            }
        });
        if !ok {
            log_warn!("claude-quota: no usable reply (status {})", response.status);
            handle.retry();
        }
        request_frame();
    }

    fn num(doc: &JsonDoc, path: &str) -> Option<f64> {
        #[expect(clippy::cast_precision_loss, reason = "percentages are small")]
        doc.f64(path).or_else(|| doc.i64(path).map(|v| v as f64))
    }

    fn window(doc: &JsonDoc, key: &str) -> Option<Window> {
        Some(Window {
            pct: num(doc, &format!("/{key}/pct"))?,
            resets_at: doc.i64(&format!("/{key}/resets_at"))?,
        })
    }

    fn parse(doc: &JsonDoc) -> Option<Quota> {
        let updated_at = doc.i64("/updated_at")?;
        let n = doc.len("/week/points").unwrap_or(0).min(400);
        let mut points = Vec::with_capacity(n);
        for i in 0..n {
            let t = doc.i64(&format!("/week/points/{i}/0"));
            let p = num(doc, &format!("/week/points/{i}/1"));
            if let (Some(t), Some(p)) = (t, p) {
                points.push((t, p));
            }
        }
        Some(Quota {
            updated_at,
            server_stale: doc.bool("/stale").unwrap_or(false),
            five: window(doc, "five_hour"),
            weekly: window(doc, "weekly"),
            week_start: doc.i64("/week/start").unwrap_or(0),
            points,
        })
    }

    // ── Time text ───────────────────────────────────────────────────────

    fn local(ts: i64) -> i64 {
        local_unix_secs_or_system(&SystemTime { unix_secs: ts }, None)
    }

    fn clock(ts: i64) -> String {
        format_time(SystemTime { unix_secs: ts }, FormatTimeOpts::default())
    }

    fn weekday(ts: i64) -> String {
        strftime(local(ts), "%a")
    }

    /// `2h 13m`, `3d 11h`, `42m`, rounded up to the minute.
    fn time_left(secs: i64) -> String {
        let m = (secs + 59) / 60;
        let (d, h, mm) = (m / 1440, m % 1440 / 60, m % 60);
        if d > 0 {
            format!("{d}d {h}h")
        } else if h > 0 {
            format!("{h}h {mm}m")
        } else {
            format!("{mm}m")
        }
    }

    // ── Screen ──────────────────────────────────────────────────────────

    fn message(msg: &str) -> Node {
        col(
            props!(width: SCREEN_W, height: SCREEN_H, background: BG, justify_content: Justify::Center, cross_align: CrossAlign::Center),
            [text(msg, style!(size: 26, color: MUTE))],
        )
    }

    fn bar(pct: f64) -> Node {
        #[expect(clippy::cast_possible_truncation, reason = "a pixel width")]
        let fill = (f64::from(METER_W) * pct.clamp(0.0, 100.0) / 100.0) as f32;
        let mut draws = vec![
            Draw::rect(3.0, 0.0, METER_W - 6.0, 6.0, TRACK),
            Draw::circle(3.0, 3.0, 3.0, TRACK),
            Draw::circle(METER_W - 3.0, 3.0, 3.0, TRACK),
        ];
        if fill >= 1.0 {
            let fill = fill.max(6.0);
            draws.push(Draw::circle(3.0, 3.0, 3.0, ACC));
            draws.push(Draw::rect(3.0, 0.0, fill - 6.0, 6.0, ACC));
            draws.push(Draw::circle(fill - 3.0, 3.0, 3.0, ACC));
        }
        canvas(props!(width: METER_W, height: 6.0), draws)
    }

    fn meter(label: &str, w: Option<Window>, now: i64) -> Node {
        let pct = w.map_or(0.0, |w| w.pct.max(0.0));
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "0..=100-ish")]
        let shown = pct.round() as u32;
        let reset = match w {
            Some(w) if w.resets_at > now => format!("resets in {}", time_left(w.resets_at - now)),
            Some(_) => "resets now".to_string(),
            None => "not started".to_string(),
        };
        let number_color = if shown >= 80 { ACC } else { INK };
        col(
            props!(width: METER_W, gap: 12.0),
            [
                row(
                    props!(width: METER_W, cross_align: CrossAlign::End),
                    [
                        text(label, style!(size: 22, weight: FontWeight::SEMIBOLD, color: INK)),
                        spacer(1.0),
                        text(reset, style!(size: 18, color: MUTE)),
                    ],
                ),
                paragraph(
                    style!(size: 76, weight: FontWeight::BOLD, color: number_color, line_height: 1.1),
                    [
                        span(shown.to_string(), ()),
                        span("%", style!(size: 30, weight: FontWeight::SEMIBOLD, color: MUTE)),
                    ],
                ),
                bar(pct),
            ],
        )
    }

    fn chart(q: &Quota, now: i64) -> Vec<Draw> {
        let label = |size: u32, color: Color| style!(size: size, color: color, valign: VerticalAlign::Baseline);
        let mut d = vec![Draw::text(
            0.0,
            110.0,
            "Weekly use this week",
            style!(size: 20, weight: FontWeight::SEMIBOLD, color: MUTE, valign: VerticalAlign::Baseline),
        )];

        let start = q.week_start;
        if q.weekly.is_none() || start <= 0 {
            d.push(Draw::text(0.0, 240.0, "No weekly data yet", label(20, DIM)));
            return d;
        }
        let end = start + WEEK_S;
        d.push(Draw::text(
            CHART_W,
            110.0,
            format!("since {} {}", weekday(start), clock(start)),
            style!(size: 17, color: DIM, align: TextAlign::Right, valign: VerticalAlign::Baseline),
        ));

        #[expect(clippy::cast_precision_loss, reason = "pixel math")]
        let x = |t: i64| ((t - start) as f32 / WEEK_S as f32 * CHART_W).clamp(0.0, CHART_W);
        #[expect(clippy::cast_possible_truncation, reason = "pixel math")]
        let y = |p: f64| CHART_BOT - (p.clamp(0.0, 100.0) / 100.0) as f32 * (CHART_BOT - CHART_TOP);

        // Local midnights inside the window, then one label per day.
        let mut bounds = vec![start];
        let mut b = start + (86_400 - local(start).rem_euclid(86_400));
        while b < end {
            bounds.push(b);
            d.push(Draw::path(vec![(x(b), CHART_TOP), (x(b), CHART_BOT)], 2.0, GRID, false, Interpolation::Linear));
            b += 86_400;
        }
        bounds.push(end);
        for pair in bounds.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if x(b) - x(a) < 40.0 {
                continue;
            }
            let today = (a..b).contains(&now);
            d.push(Draw::text(
                f32::midpoint(x(a), x(b)),
                CHART_BOT + 12.0,
                weekday(a),
                if today {
                    style!(size: 17, weight: FontWeight::SEMIBOLD, color: INK, align: TextAlign::Center)
                } else {
                    style!(size: 17, color: DIM, align: TextAlign::Center)
                },
            ));
        }

        // The limit (100 %) and the base line.
        d.push(Draw::dashed_path(vec![(0.0, CHART_TOP), (CHART_W, CHART_TOP)], 2.0, RULE, 6.0, 8.0));
        d.push(Draw::text(
            CHART_W,
            CHART_TOP + 22.0,
            "100%",
            style!(size: 15, color: DIM, align: TextAlign::Right, valign: VerticalAlign::Baseline),
        ));
        d.push(Draw::path(vec![(0.0, CHART_BOT), (CHART_W, CHART_BOT)], 2.0, RULE, false, Interpolation::Linear));

        // The readings, ending at the current value.
        let mut pts: Vec<(f32, f32)> = q.points.iter().map(|&(t, p)| (x(start + t), y(p))).collect();
        if let Some(w) = q.weekly
            && q.points.last().is_none_or(|&(t, _)| start + t < q.updated_at)
        {
            pts.push((x(q.updated_at), y(w.pct)));
        }
        if let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
            if pts.len() > 1 {
                let mut area = pts.clone();
                area.push((last.0, CHART_BOT));
                area.push((first.0, CHART_BOT));
                d.push(Draw::fill_path(area, Fill::linear(0.0, ACC.with_alpha(0.35), ACC.with_alpha(0.0)), false));
                d.push(Draw::path(pts, 3.0, ACC, false, Interpolation::Linear));
            }
            d.push(Draw::circle(last.0, last.1, 10.0, BG));
            d.push(Draw::circle(last.0, last.1, 7.0, ACC));
        }
        d
    }

    fn screen(s: &State, now: i64) -> Node {
        if !key_bound() {
            return message("Add the quota server key in the widget settings.");
        }
        if server_url().is_none() {
            return message("Add the quota server address in the widget settings.");
        }
        let Some(q) = &s.quota else {
            return message(if s.failed { "Cannot reach the quota server." } else { "Loading…" });
        };

        let body = col(
            props!(width: SCREEN_W, height: SCREEN_H, background: BG),
            [
                col(
                    props!(inset_left: 64.0, inset_top: 0.0, width: METER_W, height: SCREEN_H, justify_content: Justify::Center, gap: 40.0),
                    [meter("5-hour", q.five, now), meter("Weekly", q.weekly, now)],
                ),
                canvas(props!(inset_left: CHART_X, inset_top: 0.0, width: CHART_W, height: SCREEN_H), chart(q, now)),
            ],
        );

        let offline = q.server_stale || now - s.last_ok > OFFLINE_AFTER_S;
        if !offline {
            return body;
        }
        let since = if local(now).div_euclid(86_400) == local(q.updated_at).div_euclid(86_400) {
            clock(q.updated_at)
        } else {
            format!("{} {}", weekday(q.updated_at), clock(q.updated_at))
        };
        col(
            props!(width: SCREEN_W, height: SCREEN_H, background: BG),
            [
                dimmed(0.3, body),
                row(
                    props!(inset_left: 0.0, inset_bottom: 22.0, width: SCREEN_W, justify_content: Justify::Center),
                    [row(
                        props!(background: PILL, border_radius: 20.0, padding: 10.0),
                        [text(format!("No new data since {since}"), style!(size: 19, color: PILL_TEXT))],
                    )],
                ),
            ],
        )
    }

    // ── Widget lifecycle ────────────────────────────────────────────────

    #[unsafe(no_mangle)]
    pub extern "C" fn init() {
        let handle = register_poll(
            build_request,
            on_reply,
            PollConfig {
                interval_ms: Some(POLL_MS),
                retry_ms: 30_000,
                ..Default::default()
            },
        );
        POLL.set(Some(handle));
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn on_params_update() {
        POLL.with_borrow(|h| {
            if let Some(h) = h {
                h.invalidate();
            }
        });
        request_frame();
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn on_system_update() {
        request_frame();
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn render(_delta_ms: u32) {
        let vp = widget_viewport();
        let root = STATE.with_borrow(|s| screen(s, now()));
        let _ = render_ui(vp.width, vp.height, root);
        // The countdowns change once a minute; the data comes with the poll.
        request_frame_after(30_000);
    }
}
