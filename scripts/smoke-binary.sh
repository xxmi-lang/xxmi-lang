#!/usr/bin/env bash
# Smoke-tests dist/bin/xxmi-<target>: CLI, then the language server under both of its entry points.
# Usage: scripts/smoke-binary.sh <target>   (e.g. linux-x64; release.yml)
set -euo pipefail

target="$1"
exe=""
if [ "${RUNNER_OS:-}" = Windows ]; then exe=".exe"; fi

bin="dist/bin/xxmi-$target$exe"
"$bin" --version
"$bin" lint fixtures/rules/XM201/good.ini

# The language server is chosen by the program's file name, so test it under that name too.
mkdir -p smoke
cp "$bin" "smoke/xxmi-lsp$exe"
node rust/bench/lsp-smoke.mjs "smoke/xxmi-lsp$exe"
node rust/bench/lsp-smoke.mjs "$bin" lsp
