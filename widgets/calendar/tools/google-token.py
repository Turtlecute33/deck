#!/usr/bin/env python3
"""Get a Google refresh token for the Calendar widget.

Usage: python3 tools/google-token.py ~/Downloads/client_secret_XXXX.json

The file is the OAuth client of type "Desktop app" from Google Cloud Console.
The script opens Google's sign-in page in the browser and asks for read-only
access to your calendar events. Then it puts one value in the macOS Keychain
(item deck-calendar-token): the client ID, the client secret and the refresh
token together. You paste that value into one Token account on the Deck.
It does not show the value. It connects only to Google.
"""

import base64
import hashlib
import http.server
import json
import re
import secrets
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import webbrowser
from pathlib import Path

SCOPE = "https://www.googleapis.com/auth/calendar.events.readonly"
WAIT_S = 300
KEYCHAIN = "deck-calendar-token"
AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth"
TOKEN_URL = "https://oauth2.googleapis.com/token"


def read_client(path):
    data = json.loads(Path(path).expanduser().read_text())
    client = data.get("installed") or data.get("web")
    if not client or "client_id" not in client or "client_secret" not in client:
        sys.exit("This is not an OAuth client file. Download the JSON of a Desktop app client.")
    return client["client_id"], client["client_secret"]


def wait_for_code(state):
    """Start a local server on a free port. Returns (redirect_uri, result dict, server)."""
    result = {}

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            query = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
            if query.get("state", [""])[0] != state:
                self.send_response(400)
                self.end_headers()
                return
            result["code"] = query.get("code", [None])[0]
            result["error"] = query.get("error", [None])[0]
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.end_headers()
            self.wfile.write("Done. You can close this tab and go back to the terminal.".encode())

        def log_message(self, *args):
            pass

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    return f"http://127.0.0.1:{server.server_port}/", result, server


def account_value(client_id, client_secret, refresh):
    """The Deck account value: the token request body without grant_type."""
    return urllib.parse.urlencode({"client_id": client_id, "client_secret": client_secret, "refresh_token": refresh})


def store(service, value):
    """Add or replace a Keychain item. The value goes through stdin, not the process list."""
    if not re.fullmatch(r"[A-Za-z0-9._~%=&+-]+", value):
        sys.exit(f"Unexpected characters in the value for {service}. Nothing stored.")
    command = f"add-generic-password -U -a calendar-rail -s {service} -w {value}\n"
    subprocess.run(["security", "-i"], input=command.encode(), check=True, capture_output=True)


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    client_id, client_secret = read_client(sys.argv[1])

    state = secrets.token_urlsafe(24)
    verifier = secrets.token_urlsafe(64)
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b"=").decode()
    redirect_uri, result, server = wait_for_code(state)

    url = AUTH_URL + "?" + urllib.parse.urlencode({
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "response_type": "code",
        "scope": SCOPE,
        "access_type": "offline",
        "prompt": "consent",
        "state": state,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
    })
    print("Sign in to Google in the browser. If the browser does not open, open this URL:\n")
    print(url + "\n")
    webbrowser.open(url)
    server.timeout = 5
    deadline = time.monotonic() + WAIT_S
    while "code" not in result and time.monotonic() < deadline:
        server.handle_request()
    server.server_close()

    if not result.get("code"):
        sys.exit(f"Google did not give access: {result.get('error') or 'no code'}")

    body = urllib.parse.urlencode({
        "client_id": client_id,
        "client_secret": client_secret,
        "code": result["code"],
        "code_verifier": verifier,
        "grant_type": "authorization_code",
        "redirect_uri": redirect_uri,
    }).encode()
    try:
        with urllib.request.urlopen(urllib.request.Request(TOKEN_URL, data=body), timeout=20) as reply:
            tokens = json.load(reply)
    except urllib.error.HTTPError as err:
        sys.exit(f"Google refused the code: {json.load(err).get('error', err.code)}")
    refresh = tokens.get("refresh_token")
    if not refresh:
        sys.exit("Google gave no refresh token. Remove the app access at myaccount.google.com/permissions and try again.")

    store(KEYCHAIN, account_value(client_id, client_secret, refresh))
    print(f"Stored in the Keychain as {KEYCHAIN}. To copy it for the Deck Token account:\n")
    print(f"    security find-generic-password -s {KEYCHAIN} -w | pbcopy")


if __name__ == "__main__":
    main()
