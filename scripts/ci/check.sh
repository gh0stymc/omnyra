#!/usr/bin/env bash
# Lint, typecheck and test everything. Used by CI and runnable locally.
#   scripts/ci/check.sh          # full check (lint + tests)
#   scripts/ci/check.sh --tests  # tests only (used on Windows)
set -euo pipefail
cd "$(dirname "$0")/../.."

tests_only=false
[[ "${1:-}" == "--tests" ]] && tests_only=true

step() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

if [[ "${CI:-}" == "true" || ! -d node_modules ]]; then
  step "npm ci"
  npm ci --no-audit --no-fund
fi

if ! $tests_only; then
  step "Prettier"
  npm run --silent format:check
  step "ESLint"
  npm run --silent lint
  step "TypeScript"
  npm run --silent typecheck
  step "rustfmt"
  cargo fmt --all -- --check
  step "Clippy"
  cargo clippy --workspace --all-targets --locked -- -D warnings
fi

step "Vitest"
npm test

# Tauri's build script needs the frontend dist dir to exist.
mkdir -p dist
step "cargo test"
cargo test --workspace --locked

if ! $tests_only; then
  # `cargo test` regenerates src/lib/bindings.ts; a diff means it was not committed.
  step "TS bindings up to date"
  if ! git diff --exit-code -- src/lib/bindings.ts; then
    echo "::error::src/lib/bindings.ts is stale. Run 'cargo test --workspace' and commit it." >&2
    exit 1
  fi
fi

step "OK"
