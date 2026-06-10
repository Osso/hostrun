#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

cargo test
cargo build --release --bin hostrun-mcp --bin codex-hostrun-mcp
cargo install --path . --bins --force
