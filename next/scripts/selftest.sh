#!/bin/bash
# Self-test: opens a copy of fixtures/selftest/doc.md in a HIDDEN window,
# lets the shell drive its own UI (lib/selftest.ts: panels, find/replace,
# undo, source mode, front matter, palette, settings, save, folder search)
# and checks the report plus the bytes that were saved to disk.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [[ "$(uname)" == "Darwin" ]] && ! xcodebuild -license check >/dev/null 2>&1; then
  export DEVELOPER_DIR=/Library/Developer/CommandLineTools
fi
# shellcheck disable=SC1091
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

if [[ "${1:-}" != "--no-build" ]]; then
  npm run build -w apps/desktop --prefix "$ROOT" >"$WORK/build.log" 2>&1 || { cat "$WORK/build.log"; exit 1; }
  cargo build --quiet --manifest-path "$ROOT/Cargo.toml" -p vellumi-desktop
fi

cp "$ROOT/fixtures/selftest/doc.md" "$WORK/doc.md"
VELLUMI_SELFTEST="$WORK/report.json" "${VELLUMI_APP:-$ROOT/target/debug/vellumi-desktop}" "$WORK/doc.md" 2>"$WORK/app.log" || true

if [[ ! -s "$WORK/report.json" ]]; then
  echo "selftest: no report (app log follows)"; cat "$WORK/app.log"; exit 1
fi

node --input-type=module - "$WORK/report.json" <<'JS'
import { readFileSync } from "node:fs";
const { report, fileOnDisk } = JSON.parse(readFileSync(process.argv[2], "utf8"));
let failed = 0;
for (const c of report.checks) {
  console.log(`${c.ok ? "ok  " : "FAIL"} ${c.name}${c.ok || !c.detail ? "" : `  ${c.detail}`}`);
  if (!c.ok) failed += 1;
}
const disk = [
  ["saved file has the edited front matter", fileOnDisk.startsWith("---\ntitle: 自测\ntags: [a, b]\n---\n")],
  ["saved file has the source-mode edit", fileOnDisk.includes("源码模式追加的一行")],
  ["saved file kept the undone replace undone", fileOnDisk.includes("节奏") && !fileOnDisk.includes("韵律")],
  ["saved file kept the blank line after front matter", fileOnDisk.includes("---\n\n# 慢写作笔记")],
];
for (const [name, ok] of disk) {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}`);
  if (!ok) failed += 1;
}
const total = report.checks.length + disk.length;
console.log(failed ? `selftest: ${failed} of ${total} checks FAILED` : `selftest: all ${total} checks passed`);
process.exit(failed ? 1 : 0);
JS
