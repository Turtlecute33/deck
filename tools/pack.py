#!/usr/bin/env python3
"""Make dist/<name>-<version>.nar for each Braiins Deck widget.

Usage: python3 tools/pack.py [widget ...]
A widget is a folder name in widgets/. With no widget, packs all of them.
Run after `cargo build --release`.

Install the .nar on the Deck: Display Widgets > Install Widget.
Writes the NAR (Nix archive) format directly, so Nix is not needed.
"""

import json
import struct
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WIDGETS = ROOT / "widgets"
MAINTAINER = "turtlecute"
PROTOCOL_MAJOR = "0"  # firmware 26.10.x runs bmc-wasm-thin-v0

# Must match bmc-upgrade/assets/wasm-widget-wrapper.sh byte for byte.
LAUNCHER = (
    "#!/bin/sh\n"
    "exec /run/current-profile/bin/bmc-wasm-thin-v@protocolMajor@ \\\n"
    '    --wasm "${0%/*}/../lib/wasm/@name@.wasm" \\\n'
    '    --asset-root "${0%/*}/../lib/assets" \\\n'
    '    "$@"\n'
)


def nar_str(value):
    data = value if isinstance(value, bytes) else value.encode()
    return struct.pack("<Q", len(data)) + data + b"\0" * (-len(data) % 8)


def nar_node(node):
    """A dict is a directory; a (bytes, executable) tuple is a regular file."""
    out = nar_str("(") + nar_str("type")
    if isinstance(node, dict):
        out += nar_str("directory")
        for name in sorted(node, key=str.encode):
            out += nar_str("entry") + nar_str("(") + nar_str("name") + nar_str(name)
            out += nar_str("node") + nar_node(node[name]) + nar_str(")")
    else:
        contents, executable = node
        out += nar_str("regular")
        if executable:
            out += nar_str("executable") + nar_str("")
        out += nar_str("contents") + nar_str(contents)
    return out + nar_str(")")


def custom_sections(wasm):
    """Names of the custom sections in a WASM binary."""
    names, i = [], 8
    def leb(i):
        value = shift = 0
        while True:
            byte = wasm[i]
            i += 1
            value |= (byte & 0x7F) << shift
            shift += 7
            if byte < 0x80:
                return value, i
    while i < len(wasm):
        section_id = wasm[i]
        size, i = leb(i + 1)
        if section_id == 0:
            name_len, j = leb(i)
            names.append(wasm[j:j + name_len].decode())
        i += size
    return names


def pack(widget_dir):
    crate = tomllib.loads((widget_dir / "Cargo.toml").read_text())["package"]["name"]
    wasm_path = ROOT / "target/wasm32-unknown-unknown/release" / f"{crate.replace('-', '_')}.wasm"
    if not wasm_path.is_file():
        sys.exit(f"No WASM build for {crate}. Run: cargo build --release")
    wasm = wasm_path.read_bytes()
    # The Deck refuses a module that still holds package assets (include_svg! and the like).
    if "bmc_assets_v1" in custom_sections(wasm):
        sys.exit(f"{crate}: the WASM has a bmc_assets_v1 section; the Deck will refuse it. Draw icons with shapes instead.")
    manifest = (widget_dir / "manifest.json").read_bytes()
    meta = json.loads(manifest)
    name = meta["binary"].removeprefix("bin/")
    if meta["binary"] != f"bin/{name}" or "/" in name:
        sys.exit(f"{widget_dir.name}: manifest binary must be bin/<name>")

    launcher = LAUNCHER.replace("@protocolMajor@", PROTOCOL_MAJOR).replace("@name@", name)
    widget = {
        "manifest.json": (manifest, False),
        "bin": {name: (launcher.encode(), True)},
        "lib": {"wasm": {f"{name}.wasm": (wasm, False)}},
        "assets": {"icon.svg": ((widget_dir / "assets/icon.svg").read_bytes(), False)},
    }
    tree = {"lib": {"bmc-widgets": {MAINTAINER: {name: widget}}}}

    out = ROOT / "dist" / f"{name}-{meta['version']}.nar"
    out.parent.mkdir(exist_ok=True)
    out.write_bytes(nar_str("nix-archive-1") + nar_node(tree))
    print(f"{out.relative_to(ROOT)} ({out.stat().st_size} bytes)")


def main():
    names = sys.argv[1:] or sorted(p.name for p in WIDGETS.iterdir() if (p / "manifest.json").is_file())
    for name in names:
        widget_dir = WIDGETS / name
        if not (widget_dir / "manifest.json").is_file():
            sys.exit(f"No widget {name!r} in widgets/")
        pack(widget_dir)


if __name__ == "__main__":
    main()
