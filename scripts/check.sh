#!/bin/sh
# Runs every gate used for a release checkpoint.
set -eu
cd "$(dirname "$0")/.."

./scripts/rust.sh fmt --all --check
./scripts/rust.sh clippy --workspace --all-targets -- -D warnings
./scripts/rust.sh test --workspace
RUSTDOCFLAGS="-D warnings" ./scripts/rust.sh doc --workspace --no-deps
echo "all gates passed"
