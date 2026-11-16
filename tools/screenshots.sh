#!/bin/zsh
# Renders the README screenshots into docs/screenshots/<crate>-<shot>.png.
#
# 1. Each widget's `preview::write_shots` test writes its sample screens, made
#    from sample data only, as the encoded trees the Deck would receive.
# 2. tools/screenshots draws them with the Deck's own renderer (bmc-render).
#
# macOS only (OpenGL through CGL). Needs rustup, git and curl.
set -euo pipefail

root=${0:A:h:h}
rev=$(sed -n 's/.*BraiinsForge\/bmc-main", rev = "\([0-9a-f]*\)".*/\1/p' "$root/Cargo.toml" | head -n 1)
[[ -n $rev ]] || { print -u2 "No bmc-main rev in Cargo.toml"; exit 1; }

# bmc-main at that rev, with the real files for the Git LFS pointers the
# renderer compiles in. git-lfs is not needed: GitHub serves each file.
src=$root/tools/screenshots/.bmc-main
if [[ $(git -C "$src" rev-parse HEAD 2>/dev/null) != "$rev" ]]; then
  rm -rf "$src"
  git init -q "$src"
  git -C "$src" fetch -q --depth 1 https://github.com/BraiinsForge/bmc-main.git "$rev"
  git -C "$src" -c advice.detachedHead=false checkout -q FETCH_HEAD
fi
for file in $(cd "$src" && grep -rl --include='*' '^version https://git-lfs' assets/fonts assets/intl); do
  print "Fetching $file"
  curl -fsSL -o "$src/$file.part" "https://media.githubusercontent.com/media/BraiinsForge/bmc-main/$rev/$file"
  mv "$src/$file.part" "$src/$file"
done

cd "$root"
host=$(rustc -vV | sed -n 's/^host: //p')
trees=$(mktemp -d)
trap 'rm -rf "$trees"' EXIT

DECK_SHOTS_DIR=$trees cargo test --quiet --workspace --target "$host" write_shots
cargo run --quiet --release \
  --manifest-path tools/screenshots/Cargo.toml --target "$host" -- "$trees" "$root/docs/screenshots"
