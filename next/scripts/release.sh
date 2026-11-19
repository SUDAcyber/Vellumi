#!/bin/bash
# Builds a Vellumi release (Tauri): every test gate, a Universal (arm64 +
# x86_64) .app and .dmg (hdiutil), ad-hoc signed, then the full self-test against the
# RELEASE binary as a smoke test. Artifacts land in next/dist.
#
# It never publishes: uploading, the update feed and Homebrew are manual
# steps for the owner (docs/RELEASE.md).
#
#   scripts/release.sh            full release build
#   scripts/release.sh --arm64    Apple Silicon only (faster, for checks)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT/apps/desktop/src-tauri"
DIST="$ROOT/dist"
RELEASE_CONF="$TAURI_DIR/tauri.release.conf.json"
UPDATER_CONF="$TAURI_DIR/tauri.updater.conf.json"
VERSION="$(node -p "require('$RELEASE_CONF').version")"
TARGET="universal-apple-darwin"
[[ "${1:-}" == "--arm64" ]] && TARGET="aarch64-apple-darwin"

step() { printf '\n==> %s\n' "$*"; }

if ! xcodebuild -license check >/dev/null 2>&1; then
  export DEVELOPER_DIR=/Library/Developer/CommandLineTools
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"

step "Vellumi $VERSION ($TARGET)"

step "Dependencies"
[[ -d "$ROOT/packages/editor/node_modules" ]] || npm ci --prefix "$ROOT/packages/editor"
[[ -d "$ROOT/node_modules" ]] || npm ci --prefix "$ROOT"
if [[ "$TARGET" == "universal-apple-darwin" ]] && ! rustup target list --installed | grep -q x86_64-apple-darwin; then
  rustup target add x86_64-apple-darwin
fi

step "Gates: Rust tests, clippy, JS tests, type check"
cargo test --quiet --manifest-path "$ROOT/Cargo.toml" --workspace
cargo clippy --quiet --manifest-path "$ROOT/Cargo.toml" --workspace --all-targets -- -D warnings
node --test "$ROOT"/packages/editor/test/*.test.mjs >/dev/null
(cd "$ROOT/apps/desktop" && npx svelte-check --tsconfig ./tsconfig.json --threshold error >/dev/null)

step "Gates: Markdown fidelity and UI self-test (debug build)"
"$ROOT/scripts/fidelity.sh" | tail -1
"$ROOT/scripts/selftest.sh" | tail -1

step "Build"
CONFIG_ARGS=(--config "$RELEASE_CONF")
FEATURE_ARGS=()
if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] && ! grep -q REPLACE_WITH_PUBLIC_KEY "$UPDATER_CONF"; then
  echo "update signing key present: building with the updater"
  CONFIG_ARGS+=(--config "$UPDATER_CONF")
  FEATURE_ARGS=(--features updater)
else
  echo "no update signing key: building WITHOUT the updater (docs/RELEASE.md, step 1)"
fi
npm run build -w apps/desktop --prefix "$ROOT" >/dev/null
(cd "$ROOT/apps/desktop" && npx tauri build --target "$TARGET" --bundles app "${CONFIG_ARGS[@]}" ${FEATURE_ARGS[@]+"${FEATURE_ARGS[@]}"})

BUNDLE="$ROOT/target/$TARGET/release/bundle"
APP="$BUNDLE/macos/Vellumi.app"

step "Verify the bundle"
PLIST="$APP/Contents/Info.plist"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$PLIST")" == "com.aaron.vellumi" ]] || { echo "wrong bundle id"; exit 1; }
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST")" == "$VERSION" ]] || { echo "wrong version"; exit 1; }
ARCHS="$(lipo -archs "$APP/Contents/MacOS/vellumi-desktop")"
echo "architectures: $ARCHS"
if [[ "$TARGET" == "universal-apple-darwin" ]]; then
  [[ "$ARCHS" == *x86_64* && "$ARCHS" == *arm64* ]] || { echo "not universal"; exit 1; }
fi
codesign --verify --deep --strict "$APP"
echo "signature ok (ad-hoc)"

step "Smoke test: full self-test against the release binary"
VELLUMI_APP="$APP/Contents/MacOS/vellumi-desktop" "$ROOT/scripts/selftest.sh" --no-build | tail -1

step "Artifacts"
mkdir -p "$DIST"
# DMG with hdiutil directly: Tauri's bundle_dmg.sh scripts Finder through
# AppleScript for the window layout, which needs Automation permission and
# fails non-interactively. App + /Applications link is all the layout needed.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/Vellumi.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DIST/Vellumi-$VERSION.dmg"
hdiutil create -quiet -volname "Vellumi $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO "$DIST/Vellumi-$VERSION.dmg"
hdiutil verify -quiet "$DIST/Vellumi-$VERSION.dmg"
ditto -c -k --keepParent "$APP" "$DIST/Vellumi-$VERSION.app.zip"
for file in "$DIST/Vellumi-$VERSION.dmg" "$DIST/Vellumi-$VERSION.app.zip"; do
  printf '%s  %s  %s\n' "$(shasum -a 256 "$file" | cut -d' ' -f1)" "$(du -h "$file" | cut -f1)" "$file"
done
echo
echo "Built, verified and smoke-tested. Nothing was published — see docs/RELEASE.md."
