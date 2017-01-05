#!/bin/sh

cargo fmt -- --write-mode checkstyle | grep -v '<'
cargo outdated
cargo doc && cargo deadlinks
cargo +nightly clippy
cargo test
