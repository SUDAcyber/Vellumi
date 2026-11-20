#!/bin/bash
# Fidelity check: runs every fixture through the REAL editor (hidden Tauri
# windows) and compares what a save would write against the reviewed
# expectations, then feeds the output through again to prove saving is
# idempotent (open → save → open → save changes nothing the second time).
#
#   scripts/fidelity.sh            check
#   scripts/fidelity.sh --accept   overwrite expected/ with the current output
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIX="$ROOT/fixtures/fidelity"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Builds with the Command Line Tools when the Xcode license is not accepted.
if [[ "$(uname)" == "Darwin" ]] && ! xcodebuild -license check >/dev/null 2>&1; then
  export DEVELOPER_DIR=/Library/Developer/CommandLineTools
fi
# shellcheck disable=SC1091
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

node --test "$ROOT"/packages/editor/test/*.test.mjs >/dev/null
npm run build -w apps/desktop --prefix "$ROOT" >"$WORK/build.log" 2>&1 || { cat "$WORK/build.log"; exit 1; }
cargo build --quiet --manifest-path "$ROOT/Cargo.toml" -p vellumi-desktop
APP="${VELLUMI_APP:-$ROOT/target/debug/vellumi-desktop}"

roundtrip() { # <out dir> <files...>
  local out="$1"; shift
  mkdir -p "$out"
  VELLUMI_ROUNDTRIP_OUT="$out" "$APP" "$@" 2>"$out.log" || { cat "$out.log"; exit 1; }
}

roundtrip "$WORK/pass1" "$FIX"/input/*.md
roundtrip "$WORK/pass2" "$WORK"/pass1/*.md

if [[ "${1:-}" == "--accept" ]]; then
  rm -rf "$FIX/expected" && cp -R "$WORK/pass1" "$FIX/expected"
  echo "expected/ updated — review the diff before committing."
  exit 0
fi

status=0
for input in "$FIX"/input/*.md; do
  name="$(basename "$input")"
  if [[ -f "$WORK/pass1/$name.error" ]]; then
    echo "FAIL $name: editor error: $(cat "$WORK/pass1/$name.error")"; status=1; continue
  fi
  if ! diff -u "$FIX/expected/$name" "$WORK/pass1/$name"; then
    echo "FAIL $name: differs from expected"; status=1
  fi
  if ! cmp -s "$WORK/pass1/$name" "$WORK/pass2/$name"; then
    echo "FAIL $name: second save changed the file again"
    diff -u "$WORK/pass1/$name" "$WORK/pass2/$name" || true
    status=1
  fi
done
[[ $status -eq 0 ]] && echo "fidelity: all $(ls "$FIX"/input/*.md | wc -l | tr -d ' ') fixtures OK"
exit $status
