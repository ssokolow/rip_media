#!/bin/bash
# Source: https://users.rust-lang.org/t/tutorial-how-to-collect-test-coverages-for-rust-project/650
# TODO: https://gist.github.com/colin-kiegel/e3a1fea04cd3ad8ed06d
# TODO: https://github.com/rust-lang/cargo/issues/1924
PKGID="$(cargo pkgid)"
[ -z "$PKGID" ] && exit 1
ORIGIN="${PKGID%#*}"
ORIGIN="${ORIGIN:7}"
PKGNAMEVER="${PKGID#*#}"
PKGNAME="${PKGNAMEVER%:*}"

# XXX: Why do some `cargo pkgid` runs not contain the `name:` part?
if [ "$PKGNAME" = "$PKGNAMEVER" ]; then
    PKGNAME="${ORIGIN##*/}"
fi


shift
cargo test --no-run || exit $?
EXE=($ORIGIN/target/debug/$PKGNAME-*)
if [ ${#EXE[@]} -ne 1 ]; then
    echo 'Non-unique test file, retrying...' >2
    rm -f "${EXE[@]}"
    cargo test --no-run || exit $?
fi
rm -rf "$ORIGIN/target/cov"
kcov "$ORIGIN/target/cov" "$ORIGIN/target/debug/$PKGNAME-"* "$@"
