# Calendar (Rail) · Braiins Deck widget

Shows today's events from the Google calendars that you choose. The current or next event is large at the top. The full day is one line below it, with an orange line at the current time.

- A call has a Meet, Zoom or other video link. A meeting has other guests. All other items are events.
- An outline bar is an event that you did not accept yet.
- "No break": the next event starts when the current one ends.
- Tap a bar to see the details of that event. Tap again, or wait 20 s, to go back.
- Sizes: 1280 × 480, 638 × 480, 638 × 238 and 317 × 238.

## Data

- Read-only scope `calendar.events.readonly`. The widget cannot change your calendar.
- It reads only the calendars in the widget settings, from today to 7 days ahead, every 5 min.
- It reads only the title, times, place, video link and the guest answers. It does not read descriptions or guest names.
- It connects only to `oauth2.googleapis.com` (to get a 1-hour access token) and `www.googleapis.com` (the events).
- The client secret and the refresh token stay in the Deck credential account. The widget never sees them.
- With no data for 15 min, the top right shows the time of the last data.

## Setup

### 1. Google (one time)

1. Open console.cloud.google.com. Make a project.
2. APIs & Services > Library > Google Calendar API > Enable.
3. OAuth consent screen:
   - A Google Workspace account: user type Internal.
   - A personal Gmail account: user type External. Add your address as a test user, then set the publishing status to "In production". If the status stays "Testing", the refresh token stops after 7 days. Google shows an "unverified app" warning: select Advanced > Go to the app.
4. Credentials > Create credentials > OAuth client ID > Desktop app. Download the JSON.
5. Run:

   ```
   python3 widgets/calendar/tools/google-token.py ~/Downloads/client_secret_XXXX.json
   ```

   Sign in and allow access. The script puts one value in the macOS Keychain, item `deck-calendar-token`: the client ID, the client secret and the refresh token together. It does not show the value. To copy it for the Deck web UI:

   ```
   security find-generic-password -s deck-calendar-token -w | pbcopy
   ```

The token reads the calendars of one Google account. To show a calendar of a different account, share that calendar with this account first.

### 2. Deck

1. Deck web UI > Display Widgets > Install Widget. Select `dist/calendar-rail-<version>.nar`.
2. Add the widget to a scene.
3. Widget settings > Google Calendar > make a new account:
   - Kind: Token. Name: Google Calendar.
   - Token: paste the value from the Keychain.
   - Allowed destinations: `oauth2.googleapis.com`.
4. Widget settings > Calendars > add each calendar: a name, the calendar ID and a colour. Nothing shows until you add a calendar.
   - Calendar ID: Google Calendar > Settings > the calendar > Integrate calendar.
   - Your main calendar: `primary`.

Other settings: all-day events (on), declined events (off), first and last hour of the timeline (8 and 20).

## Build

From the repo root:

```
cargo test -p calendar-rail --target aarch64-apple-darwin   # the date and screen logic
cargo build --release -p calendar-rail
python3 tools/pack.py calendar
```

Output: `dist/calendar-rail-<version>.nar`. The SDK pin is in the root `Cargo.toml`.

To update: increase `version` in `manifest.json`, build, and install the new `.nar`.
