//! The port against the source: for each fixture, the Open-Meteo replies
//! the widget reads, and what `engine.js` computed from the same replies
//! (`tools/parity.mjs`). The index must match exactly, and every
//! intermediate value to within [`TOLERANCE`].

use super::{WindowKind, evaluate};
use crate::model::{Flow, Forecast, Sea};
use crate::spot::resolve;

use bmc_wasm_sdk::*;

/// Floating-point results may differ in the last bits between the two
/// math libraries; nothing the faces show moves at this size.
const TOLERANCE: f64 = 1e-9;

struct Case {
    name: &'static str,
    forecast: &'static [u8],
    flow: Option<&'static [u8]>,
    sea: Option<&'static [u8]>,
    expected: &'static [u8],
}

macro_rules! case {
    ($name:literal) => {
        case!($name, None, None)
    };
    ($name:literal, flow) => {
        case!(
            $name,
            Some(include_bytes!(concat!("../../fixtures/", $name, ".flow.json"))),
            None
        )
    };
    ($name:literal, sea) => {
        case!(
            $name,
            None,
            Some(include_bytes!(concat!("../../fixtures/", $name, ".sea.json")))
        )
    };
    ($name:literal, $flow:expr, $sea:expr) => {
        Case {
            name: $name,
            forecast: include_bytes!(concat!("../../fixtures/", $name, ".forecast.json")),
            flow: $flow,
            sea: $sea,
            expected: include_bytes!(concat!("../../fixtures/", $name, ".expected.json")),
        }
    };
}

const CASES: &[Case] = &[
    case!("mo-panaro-vignola", flow),
    case!("mo-panaro-vignola+138", flow),
    case!("mo-panaro-vignola+210", flow),
    case!("ra-senio-casola", flow),
    case!("pc-nure-bettola", flow),
    case!("pc-nure-bettola+138", flow),
    case!("pc-nure-bettola+320", flow),
    case!("mo-casse-secchia"),
    case!("mo-casse-secchia+138"),
    case!("mo-casse-secchia+210"),
    case!("re-cavo-fiuma"),
    case!("rn-laghi-santarcangelo"),
    case!("rn-portocanale-rimini", sea),
    case!("rn-portocanale-rimini+138", sea),
    case!("rn-portocanale-rimini+210", sea),
    case!("pr-taro-san-secondo", flow),
];

fn close(name: &str, what: &str, got: Option<f64>, want: Option<f64>) {
    match (got, want) {
        (Some(g), Some(w)) => assert!(
            (g - w).abs() <= TOLERANCE,
            "{name} {what}: got {g}, the site has {w}"
        ),
        (None, None) => {}
        _ => panic!("{name} {what}: got {got:?}, the site has {want:?}"),
    }
}

fn window_kind(kind: WindowKind) -> &'static str {
    match kind {
        WindowKind::Dawn => "Dawn",
        WindowKind::Dusk => "Dusk",
        WindowKind::Night => "Night",
        WindowKind::AllDay => "AllDay",
    }
}

fn check(case: &Case) -> usize {
    let want = JsonDoc::parse(case.expected);
    let spot_id = want.str("/spot").expect("BUG: the fixture names its spot");
    let spot = resolve(&spot_id).expect("BUG: the fixture spot is in the catalogue");

    let mut forecast =
        Forecast::try_from(&JsonDoc::parse(case.forecast)).expect("BUG: a valid forecast");
    if let Some(sea) = case.sea {
        forecast.merge_sea(&Sea::try_from(&JsonDoc::parse(sea)).expect("BUG: a valid sea reply"));
    }
    let flow = case
        .flow
        .map(|f| Flow::try_from(&JsonDoc::parse(f)).expect("BUG: a valid flow reply"));

    let days = want.len("/days").unwrap_or(0);
    assert!(days > 0, "{}: no expected days", case.name);
    for k in 0..days {
        let at = |field: &str| fmt!("/days/{}/{}", k, field);
        let date = want.str(&at("date")).unwrap_or_default();
        let name = fmt!("{} {}", case.name, date.as_str());
        let index = forecast
            .days
            .iter()
            .position(|d| d.date == date)
            .expect("BUG: the expected day is in the forecast");
        let got = evaluate(spot, &forecast, flow.as_ref(), index).expect("BUG: the day scores");

        assert_eq!(
            Some(i64::from(got.score)),
            want.i64(&at("score")),
            "{name}: index"
        );
        close(&name, "water", got.water_temp, want.f64(&at("water")));
        assert_eq!(Some(got.water_measured), want.bool(&at("measured")), "{name}");
        close(&name, "wave", got.wave, want.f64(&at("wave")));
        close(&name, "turbidity", Some(got.turbidity), want.f64(&at("turbidity")));
        close(&name, "flow", got.flow, want.f64(&at("flow")));
        close(&name, "flow ratio", got.flow_ratio, want.f64(&at("ratio")));
        close(&name, "rain 72 h", Some(got.rain_72h), want.f64(&at("rain72")));
        close(&name, "pressure", got.pressure_delta, want.f64(&at("pressureDelta")));
        close(&name, "moon", Some(got.moon.illumination), want.f64(&at("moon")));
        assert_eq!(Some(got.moon.name.to_owned()), want.str(&at("moonName")), "{name}");
        assert_eq!(Some(got.season.label().to_owned()), want.str(&at("season")), "{name}");

        let species = want.len(&at("species")).unwrap_or(0);
        assert_eq!(got.species.len(), species, "{name}: species count");
        for (n, s) in got.species.iter().enumerate() {
            let item = |i: usize| fmt!("/days/{}/species/{}/{}", k, n, i);
            assert_eq!(
                Some(s.entry.info().key.to_owned()),
                want.str(&item(0)),
                "{name}: species {n}"
            );
            close(&name, s.entry.info().key, Some(s.score), want.f64(&item(1)));
            assert_eq!(Some(s.release_only), want.bool(&item(2)), "{name}: {}", s.entry.info().key);
        }

        let mods: Vec<String> = (0..want.len(&at("mods")).unwrap_or(0))
            .filter_map(|n| want.str(&fmt!("/days/{}/mods/{}", k, n)))
            .collect();
        let got_mods: Vec<String> = got.modifiers.iter().map(super::Modifier::key).collect();
        assert_eq!(got_mods, mods, "{name}: modifiers");

        let windows: Vec<(String, String)> = (0..want.len(&at("windows")).unwrap_or(0))
            .map(|n| {
                let part = |i: usize| {
                    want.str(&fmt!("/days/{}/windows/{}/{}", k, n, i))
                        .unwrap_or_default()
                };
                (part(0), part(1))
            })
            .collect();
        let got_windows: Vec<(String, String)> = got
            .windows
            .iter()
            .map(|w| (window_kind(w.kind).to_owned(), w.span.clone()))
            .collect();
        assert_eq!(got_windows, windows, "{name}: windows");
    }
    days
}

#[test]
fn every_fixture_day_scores_what_the_site_engine_scores() {
    let days: usize = CASES.iter().map(check).sum();
    // 16 fixtures of 21 days each.
    assert_eq!(days, 16 * 21);
}

#[test]
fn the_fixtures_cover_every_kind_of_water() {
    use crate::spot::WaterKind;
    let kinds: Vec<WaterKind> = CASES
        .iter()
        .filter_map(|c| JsonDoc::parse(c.expected).str("/spot"))
        .filter_map(|id| resolve(&id).map(|s| s.kind))
        .collect();
    for kind in [
        WaterKind::River,
        WaterKind::Torrent,
        WaterKind::Lake,
        WaterKind::Canal,
        WaterKind::Quarry,
        WaterKind::Sea,
    ] {
        assert!(kinds.contains(&kind), "no fixture for {kind:?}");
    }
}
