//! Sample screens for the README, rendered off-device by `tools/screenshots`.
//!
//! Host only: none of this reaches the binary the Deck runs. The data is
//! public: the Open-Meteo replies in `fixtures/` for two catalogue spots.

use bmc_wasm_sdk::{JsonDoc, Node, SizeVariant, WidgetSize};

use crate::model::{Flow, Forecast, Sea};
use crate::render;
use crate::spot::resolve;
use crate::view::View;

/// One screen at one size.
pub struct Shot {
    pub name: &'static str,
    pub caption: &'static str,
    pub width: u32,
    pub height: u32,
    pub node: Node,
}

const RIVER_FORECAST: &[u8] = include_bytes!("../fixtures/mo-panaro-vignola.forecast.json");
const RIVER_FLOW: &[u8] = include_bytes!("../fixtures/mo-panaro-vignola.flow.json");
const SEA_FORECAST: &[u8] = include_bytes!("../fixtures/rn-portocanale-rimini.forecast.json");
const SEA_REPLY: &[u8] = include_bytes!("../fixtures/rn-portocanale-rimini.sea.json");

fn shot(name: &'static str, caption: &'static str, variant: SizeVariant, view: &View) -> Shot {
    let size = WidgetSize {
        variant,
        width: variant.width(),
        height: variant.height(),
    };
    Shot {
        name,
        caption,
        width: size.width,
        height: size.height,
        node: render::view(view, size),
    }
}

/// The sample screens: a river spot at every size, and a sea spot full screen.
///
/// # Panics
///
/// When a fixture no longer parses or a spot left the catalogue; the
/// screenshot run should stop then rather than draw an empty face.
#[must_use]
pub fn shots() -> Vec<Shot> {
    let river = resolve("panaro-a-vignola").expect("BUG: the catalogue has this spot");
    let forecast = Forecast::try_from(&JsonDoc::parse(RIVER_FORECAST)).expect("BUG: a valid forecast");
    let flow = Flow::try_from(&JsonDoc::parse(RIVER_FLOW)).expect("BUG: a valid flow reply");
    let river = View::build(river, &forecast, Some(&flow), 0).expect("BUG: today is in the fixture");

    let sea = resolve("portocanale-di-rimini").expect("BUG: the catalogue has this spot");
    let mut sea_forecast = Forecast::try_from(&JsonDoc::parse(SEA_FORECAST)).expect("BUG: a valid forecast");
    sea_forecast.merge_sea(&Sea::try_from(&JsonDoc::parse(SEA_REPLY)).expect("BUG: a valid sea reply"));
    let sea = View::build(sea, &sea_forecast, None, 0).expect("BUG: today is in the fixture");

    vec![
        shot(
            "fullscreen",
            "A river spot full screen: the index, the best window, eight readings and the best species.",
            SizeVariant::Full,
            &river,
        ),
        shot("large", "The same spot at 638 × 480.", SizeVariant::Large, &river),
        shot("medium", "The same spot at 638 × 238.", SizeVariant::Medium, &river),
        shot("small", "The same spot at 317 × 238.", SizeVariant::Small, &river),
        shot(
            "sea",
            "A sea spot: the measured sea temperature and the waves replace the river flow.",
            SizeVariant::Full,
            &sea,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmc_wasm_sdk::encode;

    #[test]
    fn every_shot_builds_and_encodes() {
        let shots = shots();
        assert_eq!(shots.len(), 5);
        for s in &shots {
            assert!(!encode(&s.node).is_empty(), "{} encoded to nothing", s.name);
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
