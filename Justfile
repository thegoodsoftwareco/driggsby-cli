required-check:
    #!/usr/bin/env bash
    set -euo pipefail
    npm ci
    npm run check
    npm run build
    cargo fmt --all -- --check
    cargo clippy --workspace --lib --bins --examples --all-features -- \
      -D warnings -D clippy::unwrap_used -D clippy::expect_used \
      -D clippy::panic -D clippy::todo -D clippy::unimplemented \
      -D clippy::undocumented_unsafe_blocks
    cargo clippy --workspace --tests --all-features -- \
      -D warnings -D clippy::unwrap_used -D clippy::expect_used \
      -D clippy::todo -D clippy::unimplemented \
      -D clippy::undocumented_unsafe_blocks

verify: required-check
    cargo test --all-features --locked
    cargo build --locked
