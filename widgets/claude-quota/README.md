# Claude Quota · Braiins Deck widget

Shows your Claude plan use: the 5-hour and the weekly use with the time to reset, and a chart of the weekly use since the last weekly reset.

- The data comes from your quota server (`GET /v1/quota`), not from Claude. The quota server is a separate program. It is not in this repo.
- Size: 1280 × 480 (full screen) only.

## Data

- It connects only to the quota server address in the widget settings. There is no default address.
- One request every 60 s. After an error, it tries again after 30 s. Each request stops after 5 s.
- The key stays in the Deck credential account. The host adds it to the request as `Authorization: Bearer`. The widget never sees it.
- With no good reply for 3 min, the screen dims and shows the time of the last data.

## Setup

1. Deck web UI > Display Widgets > Install Widget. Select `dist/claude-quota-<version>.nar`.
2. Add the widget to a scene as a full-screen widget.
3. Widget settings > Quota server key: add a generic token account with the `READ_KEY` of the quota server. Allow only the host of the quota server.
4. Widget settings > Quota server: the address of the quota server, for example `http://quota-server.local:8787`. Nothing shows until you set it.

## Build

From the repo root:

```
cargo build --release -p claude-quota
python3 tools/pack.py claude-quota
```

Output: `dist/claude-quota-<version>.nar`. To update: increase `version` in `manifest.json`, build, and install the new `.nar`.
