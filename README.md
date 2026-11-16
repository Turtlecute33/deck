# Deck

Widgets for the Braiins Deck. Each widget is a Rust crate that compiles to WASM.

| Widget | Folder | Shows |
| --- | --- | --- |
| Calendar (Rail) | [`widgets/calendar`](widgets/calendar) | Today's events from the Google calendars you choose |
| Claude Quota | [`widgets/claude-quota`](widgets/claude-quota) | Claude plan use: 5-hour and weekly limits, from the quota server on the LAN |
| GitHub | [`widgets/github`](widgets/github) | Unread notifications. With none: stars today, followers and top repo |
| Fishing Spot | [`widgets/fishing-spot`](widgets/fishing-spot) | Daily fishing index for a dovepescare.com spot that you choose |

No widget has personal data built in. Tokens go in Deck credential accounts, and addresses, calendars and spots go in the widget settings. The setup for each widget is in its README.

## Build

Requires rustup. `rust-toolchain.toml` installs Rust 1.99.1 with the `wasm32-unknown-unknown` target.

```
cargo build --release                         # all widgets
python3 tools/pack.py                         # all widgets: dist/<name>-<version>.nar
python3 tools/pack.py github                  # one widget (a folder name in widgets/)
cargo test --target aarch64-apple-darwin      # the tests run on the host
```

The SDK (`bmc-wasm-sdk`) is pinned in the root `Cargo.toml` to the 26.10.1 firmware release (WASM protocol 0.9). All widgets use the same pin. The release profile and the shared lints (from `bmc-main/widgets-wasm`) are also in the root `Cargo.toml`. All widgets must pass `cargo clippy --workspace -- -D warnings`.

## Install

1. Deck web UI > Display Widgets > Install Widget. Select the `.nar` from `dist/`.
2. Add the widget to a scene.

To update a widget: increase `version` in its `manifest.json`, build, pack, and install the new `.nar`.

## Add a widget

1. Make `widgets/<folder>/` with:
   - `Cargo.toml`: `crate-type = ["cdylib"]`, `bmc-wasm-sdk.workspace = true` and `[lints] workspace = true`.
   - `manifest.json`: a new `uid`, and `"binary": "bin/<name>"`. The `.nar` gets this name.
   - `assets/icon.svg` and `src/lib.rs`.
   - `README.md` with these sections: an intro, `## Data` (hosts, intervals, what stays on the Deck), `## Setup`, `## Build`, and `## Notes` if needed.
   - No personal values as defaults: no addresses, accounts or locations.
2. Build and pack. The workspace and `tools/pack.py` find the new folder.

## Licence

GPL-3.0-or-later, as set in each widget's `Cargo.toml`.
