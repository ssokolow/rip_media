#!/bin/sh

cargo fmt -- --write-mode checkstyle | grep -v '<'
cargo doc && cargo deadlinks
rustup run nightly cargo clippy
cargo test
