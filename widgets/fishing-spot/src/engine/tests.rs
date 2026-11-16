use super::*;
use crate::model::{Day, Flow, Forecast};
use crate::spot::{Category, Notice, Spot, WaterKind, WinterClosure, resolve};

fn days(y: i64, m: u32, d: u32) -> i64 {
    days_from_civil(y, m, d).unwrap_or(0)
}

fn day(date: &str) -> Day {
    Day {
        date: date.to_owned(),
        weather_code: Some(2),
        t_max: Some(24.0),
        t_min: Some(14.0),
        t_mean: Some(19.0),
        precipitation: Some(0.0),
        wind: Some(10.0),
        pressure: Some(1015.0),
        cloud_cover: Some(30.0),
        sunrise: "06:55".to_owned(),
        sunset: "19:21".to_owned(),
        ..Day::default()
    }
}

fn forecast(from: &str, count: i64, today: usize) -> Forecast {
    let start = parse_date(from)
        .and_then(|(y, m, d)| days_from_civil(y, m, d))
        .unwrap_or(0);
    Forecast {
        elevation: 50.0,
        has_mean: true,
        days: (0..count).map(|n| day(&iso_date(start + n))).collect(),
        today_index: today,
    }
}

fn spot(address: &str) -> &'static Spot {
    resolve(address).expect("BUG: the catalogue has this spot")
}

#[test]
fn the_bell_peaks_at_the_centre_and_dips_at_the_optimum_edges() {
    assert!((bell(Some(20.0), (15.0, 25.0), (5.0, 32.0)) - 1.0).abs() < 1e-12);
    assert!((bell(Some(15.0), (15.0, 25.0), (5.0, 32.0)) - 0.88).abs() < 1e-12);
    assert!((bell(Some(5.0), (15.0, 25.0), (5.0, 32.0)) - 0.03).abs() < 1e-12);
    assert!((bell(Some(33.0), (15.0, 25.0), (5.0, 32.0)) - 0.03).abs() < 1e-12);
    assert!((bell(None, (15.0, 25.0), (5.0, 32.0)) - 0.6).abs() < 1e-12);
}

#[test]
fn running_water_follows_the_air_on_the_plain_and_stays_cold_up_high() {
    let air = [Some(25.0); 7];
    let plain = water_temperature(WaterKind::River, &air, 50.0).unwrap_or(0.0);
    let high = water_temperature(WaterKind::River, &air, 1000.0).unwrap_or(0.0);
    assert!(plain > high + 5.0, "{plain} vs {high}");
    assert!(water_temperature(WaterKind::Lake, &[None, None], 0.0).is_none());
}

#[test]
fn no_water_passes_its_ceiling() {
    let air = [Some(45.0); 14];
    for kind in [WaterKind::Canal, WaterKind::Lake, WaterKind::Torrent, WaterKind::Sea] {
        let t = water_temperature(kind, &air, 0.0).unwrap_or(0.0);
        assert!(t <= kind.inertia().ceiling, "{kind:?}: {t}");
    }
}

#[test]
fn the_trout_season_closes_after_the_first_october_sunday_and_opens_in_march() {
    // 2026: last Sunday of March is the 29th, first Sunday of October the 4th.
    assert_eq!(trout_season_reopens(days(2026, 3, 28), 2026, false), Some(days(2026, 3, 29)));
    assert_eq!(trout_season_reopens(days(2026, 3, 29), 2026, false), None);
    assert_eq!(trout_season_reopens(days(2026, 10, 4), 2026, false), None);
    assert_eq!(trout_season_reopens(days(2026, 10, 5), 2026, false), Some(days(2027, 3, 28)));
    // Some mountain lakes open on the first Sunday of May.
    assert_eq!(trout_season_reopens(days(2027, 4, 30), 2027, true), Some(days(2027, 5, 2)));
}

#[test]
fn closed_windows_include_their_ends_and_can_wrap_the_year() {
    assert!(in_window(5, 15, [5, 15, 6, 30]));
    assert!(in_window(6, 30, [5, 15, 6, 30]));
    assert!(!in_window(7, 1, [5, 15, 6, 30]));
    assert!(in_window(1, 10, [10, 5, 3, 25]));
    assert!(!in_window(4, 1, [10, 5, 3, 25]));
}

#[test]
fn the_seasonal_value_fades_between_months_but_a_zero_stays_a_step() {
    let months = [0.0, 0.2, 0.4, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.0];
    assert!(seasonal(&months, 2026, 2, 1) < 0.4);
    assert!((seasonal(&months, 2026, 2, 16) - 0.4).abs() < 0.02);
    assert!(seasonal(&months, 2026, 2, 31) > 0.4);
    // January is zero: closed or absent, with no fade from February.
    assert!(seasonal(&months, 2026, 0, 31).abs() < f64::EPSILON);
    // February next to a zero January keeps its own value.
    assert!((seasonal(&months, 2026, 1, 1) - 0.2).abs() < f64::EPSILON);
}

#[test]
fn labels_and_bands_meet_at_the_source_thresholds() {
    assert_eq!(label(78), "Excellent");
    assert_eq!(label(77), "Very good");
    assert_eq!(label(62), "Very good");
    assert_eq!(label(46), "Good");
    assert_eq!(label(30), "Fair");
    assert_eq!(label(16), "Poor");
    assert_eq!(label(15), "Avoid");
    assert_eq!(band(62), Band::Good);
    assert_eq!(band(61), Band::Fair);
    assert_eq!(band(29), Band::Poor);
}

#[test]
fn the_moon_cycles_and_names_its_phases() {
    // Full moon on 2026-08-28, new moon on 2026-09-11.
    let full = moon_phase(days(2026, 8, 28), 2026);
    assert!(full.illumination > 0.97, "{full:?}");
    assert_eq!(full.name, "Full moon");
    let new = moon_phase(days(2026, 9, 11), 2026);
    assert!(new.illumination < 0.03, "{new:?}");
    assert_eq!(new.name, "New moon");
}

#[test]
fn a_notice_closes_the_spot_only_inside_its_dates() {
    static NOTICE: [Notice; 1] = [Notice {
        from: Some("2026-10-20"),
        to: Some("2026-10-31"),
    }];
    let base = spot("panaro-a-vignola");
    let closed = Spot {
        notices: &NOTICE,
        ..*base
    };
    let d = |date: &str| {
        let (y, m, dd) = parse_date(date).unwrap_or((0, 1, 1));
        closure(&closed, date, days(y, m, dd), y)
    };
    assert_eq!(d("2026-10-19"), None);
    assert_eq!(d("2026-10-20"), Some(Closure::Notice));
    assert_eq!(d("2026-10-31"), Some(Closure::Notice));
    assert_eq!(d("2026-11-01"), None);
}

#[test]
fn a_closed_spot_scores_zero_and_has_no_windows() {
    let trout = trout_spot();
    let f = forecast("2026-11-01", 21, 14);
    let e = evaluate(trout, &f, None, 14).expect("BUG: the day scores");
    assert_eq!(e.score, 0);
    assert_eq!(e.label(), "Closed");
    assert!(e.windows.is_empty());
    assert!(matches!(
        e.modifiers.first(),
        Some(Modifier::Closed(Closure::Season { .. }))
    ));
}

fn trout_spot() -> &'static Spot {
    crate::spot::SPOTS
        .iter()
        .find(|s| s.category == Category::D && s.winter_closure == WinterClosure::March)
        .expect("BUG: the catalogue has trout waters")
}

#[test]
fn a_thunderstorm_and_heavy_rain_cost_score() {
    let s = spot("panaro-a-vignola");
    let f = forecast("2026-09-01", 21, 14);
    let clear = evaluate(s, &f, None, 14).expect("BUG: the day scores").score;
    let mut stormy = f.clone();
    stormy.days[14].weather_code = Some(95);
    stormy.days[14].precipitation = Some(30.0);
    let e = evaluate(s, &stormy, None, 14).expect("BUG: the day scores");
    assert!(e.score < clear, "{} vs {clear}", e.score);
    assert!(e.modifiers.contains(&Modifier::Thunderstorms));
    assert!(e.modifiers.contains(&Modifier::HeavyRain));
}

#[test]
fn a_flood_shows_in_the_ratio_and_the_modifiers() {
    let s = spot("panaro-a-vignola");
    let f = forecast("2026-09-01", 21, 14);
    let mut flow = Flow {
        days: f.days.iter().map(|d| (d.date.clone(), Some(5.0))).collect(),
    };
    flow.days[14].1 = Some(40.0);
    let e = evaluate(s, &f, Some(&flow), 14).expect("BUG: the day scores");
    assert!(e.flow_ratio.is_some_and(|r| r > 2.6), "{:?}", e.flow_ratio);
    assert!(e.modifiers.contains(&Modifier::Flood));

    // Less than 0.02 m³/s is noise, not a reading.
    flow.days[14].1 = Some(0.01);
    let e = evaluate(s, &f, Some(&flow), 14).expect("BUG: the day scores");
    assert_eq!(e.flow, None);
    assert_eq!(e.flow_ratio, None);
}

#[test]
fn a_day_without_readings_still_scores() {
    let s = spot("panaro-a-vignola");
    let f = Forecast {
        elevation: 0.0,
        has_mean: true,
        days: vec![Day {
            date: "2026-09-17".to_owned(),
            ..Day::default()
        }],
        today_index: 0,
    };
    let e = evaluate(s, &f, None, 0).expect("BUG: a dated day always scores");
    assert!(e.score <= 97);
    assert!(e.water_temp.is_none());
}

#[test]
fn evaluate_declines_an_out_of_range_or_undated_day() {
    let s = spot("panaro-a-vignola");
    let mut f = forecast("2026-09-01", 3, 1);
    assert!(evaluate(s, &f, None, 3).is_none());
    f.days[1].date = "rubbish".to_owned();
    assert!(evaluate(s, &f, None, 1).is_none());
}

#[test]
fn species_come_back_ranked_best_first() {
    let s = spot("panaro-a-vignola");
    let f = forecast("2026-09-01", 21, 14);
    let e = evaluate(s, &f, None, 14).expect("BUG: the day scores");
    assert_eq!(e.species.len(), s.species.len());
    assert!(e.species.windows(2).all(|w| w[0].score >= w[1].score));
}

#[test]
fn dates_move_across_months_and_years() {
    assert_eq!(date_after("2026-12-31", 1).as_deref(), Some("2027-01-01"));
    assert_eq!(date_after("2026-03-01", -1).as_deref(), Some("2026-02-28"));
}
