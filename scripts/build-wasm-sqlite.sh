#!/usr/bin/env bash
# Builds the wasm32 static SQLite library (libwsqlite3.a) for the WME extension.
#
# sqlite-wasm-rs (the C FFI behind rusqlite on wasm32) normally compiles SQLite
# from C at build time, which requires a C compiler targeting wasm32. Building
# SQLite with the default rustc/cc wrapper does not work, so we prebuild the
# archive with Homebrew LLVM and commit it to vendor/wasm32-unknown-unknown.
#
# Rebuild it when the sqlite-wasm-rs version in Cargo.lock changes.
#
# Requires: brew install llvm, rustup target add wasm32-unknown-unknown

set -euo pipefail

VERSION="${1:-0.5.5}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CC=/opt/homebrew/opt/llvm/bin/clang
AR=/opt/homebrew/opt/llvm/bin/llvm-ar
WORK="$(mktemp -d)"

if [ ! -x "$CC" ] || [ ! -x "$AR" ]; then
    echo "error: llvm not found. Install it with: brew install llvm" >&2
    exit 1
fi

trap 'rm -rf "$WORK"' EXIT

git clone --depth 1 --branch "$VERSION" https://github.com/Spxg/sqlite-wasm-rs "$WORK/sqlite-wasm-rs"
cd "$WORK/sqlite-wasm-rs"
CC="$CC" AR="$AR" cargo build --release --target wasm32-unknown-unknown

A=$(find target/wasm32-unknown-unknown/release/build -name 'libwsqlite3.a' | head -1)
[ -n "$A" ] || { echo "error: libwsqlite3.a not found" >&2; exit 1; }

mkdir -p "$ROOT/vendor/wasm32-unknown-unknown"
cp "$A" "$ROOT/vendor/wasm32-unknown-unknown/libwsqlite3.a"
echo "Wrote $ROOT/vendor/wasm32-unknown-unknown/libwsqlite3.a"
