#!/bin/sh
CHANNEL=stable
TARGET=i686-unknown-linux-musl
TARGET_PATH="target/$TARGET/release/rip_media"
STRIP_FLAGS="--strip-unneeded"
FEATURES=""
export UPX="--ultra-brute"

# If --nightly, use opt-level=z and alloc_system to cut 115K from the output
if [ "$1" = "--nightly" ]; then
    CHANNEL=nightly
    FEATURES="$FEATURES --features=nightly"

    # TODO: Find a less hacky way to do this
    cleanup() {
        sed -i 's/opt-level = "z"/opt-level = 3/' Cargo.toml
    }
    trap cleanup EXIT
    sed -i 's/opt-level = 3/opt-level = "z"/' Cargo.toml
fi

# Always delete and rebuild (since stripping a UPXd executable is fatal)
rm "$TARGET_PATH"
# shellcheck disable=SC2086
rustup run "$CHANNEL" cargo build --release --target="$TARGET" $FEATURES

# Crunch down the resulting output using strip, sstrip, and upx
strip $STRIP_FLAGS "$TARGET_PATH"
sstrip "$TARGET_PATH"  # from ELFkickers
upx "$TARGET_PATH"

# Display the result
ls -lh "$TARGET_PATH"
