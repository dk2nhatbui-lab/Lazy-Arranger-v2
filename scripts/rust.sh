#!/usr/bin/env bash
# Runs cargo with a workspace-local CARGO_HOME so the build never needs to
# write outside the repository (the agent sandbox allows only workspace writes).
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo-home"
exec cargo "$@"
