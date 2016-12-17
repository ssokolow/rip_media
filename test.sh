#!/bin/sh

cargo fmt -- --write-mode checkstyle | grep -v '<'
cargo deadlinks
rustup run nightly cargo clippy
cargo test
