#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=1
python3 scripts/check_text.py
test -f Cargo.lock || { echo "Cargo.lock is missing; generate, review and commit it first" >&2; exit 1; }
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked --release
./target/release/logspindle --help >/dev/null
./target/release/logspindle summary tests/fixtures/sample.jsonl --json >/dev/null
git diff --check
test -z "$(git status --porcelain)" || { echo "Working tree has uncommitted changes" >&2; exit 1; }
echo "Local verification complete"
