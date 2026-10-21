#!/usr/bin/env bash
# Runs the official toml-test suite (TOML v1.0.0) against kosmetic.
#
# The suite is a git submodule pinned to a release tag; its runner is built
# from that same checkout, so tests and runner always match. Needs Go and Cargo.
# Extra arguments go to the runner, e.g. `-run 'valid/string/*'` or `-v`.
set -euo pipefail

cd "$(dirname "$0")/.."
git submodule update --init toml-test

cargo build --quiet -p kosmetic-toml-test
(cd toml-test && go build -o ../target/toml-test ./cmd/toml-test)

target/toml-test test -toml=1.0 -decoder="$PWD/target/debug/kosmetic-toml-test" "$@"
