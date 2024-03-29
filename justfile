# Version 0.3-nocomp
# Copyright 2017-2020, Stephan Sokolow

# --== Variables to be customized/overridden by the user ==--

# The target for `cargo` commands to use and `install-rustup-deps` to install
export CARGO_BUILD_TARGET := "x86_64-unknown-linux-musl"

# An easy way to override the `cargo` channel for just this project
channel := "stable"

# Extra cargo features to enable
features := ""

# An easy place to modify the build flags used
build_flags := ""

# Example for OpenPandora cross-compilation
# export CARGO_BUILD_TARGET := "arm-unknown-linux-gnueabi"

# -- `build-dist` --

# Flags passed to [UPX](https://upx.github.io/)
upx_flags := "--ultra-brute"

# -- `kcachegrind` --

# Extra arguments to pass to [callgrind](http://valgrind.org/docs/manual/cl-manual.html).
callgrind_args := ""

# Temporary file used by `just kcachegrind`
callgrind_out_file := "callgrind.out.justfile"

# Set this to override how `kcachegrind` is called
kcachegrind := "kcachegrind"

# -- `install` and `uninstall` --

# Where to `install` manpages. As long as `~/.cargo/bin` is in your `PATH`, `man` should
# automatically pick up this location.
manpage_dir := "~/.cargo/share/man/man1"

# --== Code Begins ==--

# Internal variables
# TODO: Look up that GitHub issues post on whitespace handling
_cargo_cmd := "cargo"  # Used for --dry-run simulation
_cargo := _cargo_cmd + " \"+" + channel + "\""
_build_flags := "--features=\"" + features + "\" " + build_flags
_doc_flags := "--document-private-items --features=\"" + features + "\""

# Parse the value of the "name" key in the [package] section of Cargo.toml
# using only the commands any POSIX-compliant platform should have
# Source: http://stackoverflow.com/a/40778047/435253
export _pkgname := `sed -nr "/^\[package\]/ { :l /^name[ ]*=/ { s/.*=[ ]*//; p; q;}; n; b l;}" Cargo.toml | sed 's@^"\(.*\)"$@\1@'`
export _pkgname_snake := `sed -nr "/^\[package\]/ { :l /^name[ ]*=/ { s/.*=[ ]*//; p; q;}; n; b l;}" Cargo.toml | sed 's@^"\(.*\)"$@\1@' | tr - _`
export _rls_bin_path := "target/" + CARGO_BUILD_TARGET  + "/release/" + _pkgname
export _dbg_bin_path := "target/" + CARGO_BUILD_TARGET  + "/debug/" + _pkgname

# Shorthand for `just fulltest`
DEFAULT: fulltest

# -- Development --

# Alias for cargo-edit's `cargo add` which regenerates local API docs afterwards
add +args="":
	{{_cargo}} add {{args}}
	just doc

# Alias for `cargo bloat`
bloat +args="":
	{{_cargo}} bloat {{_build_flags}} {{args}}

# Alias for `cargo check`
check +args="":
	{{_cargo}} check {{_build_flags}} {{args}}

# Superset of `cargo clean -v` which deletes other stuff this justfile builds
clean +args="":
	{{_cargo}} clean -v {{args}}
	export CARGO_TARGET_DIR="target/kcov" && {{_cargo}} clean -v
	rm -rf dist

# Alias for `cargo clippy` which touches `src/*` to work around clippy bug
clippy +args="":
	touch src/*
	{{_cargo}} clippy {{args}}

# Run rustdoc with `--document-private-items` and then run cargo-deadlinks
doc +args="":
	{{_cargo}} doc {{_doc_flags}} {{args}} && \
	{{_cargo}} deadlinks --dir target/{{CARGO_BUILD_TARGET}}/doc/{{_pkgname_snake}}

# Alias for `cargo +nightly fmt -- {{args}}`
fmt +args="":
	{{_cargo_cmd}} +nightly fmt -- {{args}}

# Alias for `cargo +nightly fmt -- --check {{args}}` which un-bloats TODO/FIXME warnings
fmt-check +args="":
	cargo +nightly fmt -- --check --color always {{args}} 2>&1 | egrep -v '[0-9]*[ ]*\|'

# Run all installed static analysis, plus `cargo test`
fulltest:
	@echo "============================= Outdated Packages ============================="
	{{_cargo}} outdated
	@echo "\n============================= Insecure Packages ============================="
	{{_cargo}} audit -q
	@echo "\n=============================== Clippy Lints ================================"
	{{_cargo}} clippy -q {{_build_flags}}
	@echo "\n===================== Dead Internal Documentation Links ====================="
	{{_cargo}} doc -q --document-private-items {{_build_flags}} && \
	{{_cargo}} deadlinks --dir target/$CARGO_BUILD_TARGET/doc/{{_pkgname}}
	@echo "\n============================== Automated Tests =============================="
	{{_cargo}} test -q {{_build_flags}}
	@echo "==============================================================================="

# Alias for `cargo geiger`
geiger +args="":
	{{_cargo}} geiger {{args}}

# Run a debug build under [callgrind](http://valgrind.org/docs/manual/cl-manual.html), then open the
# profile in [KCachegrind](https://kcachegrind.github.io/)
kcachegrind +args="":
	{{_cargo}} build
	rm -rf '{{ callgrind_out_file }}'
	valgrind --tool=callgrind --callgrind-out-file='{{ callgrind_out_file }}' \
		{{ callgrind_args }} 'target/{{ CARGO_BUILD_TARGET }}/debug/{{ _pkgname }}' \
		'{{ args }}' || true
	test -e '{{ callgrind_out_file }}'
	{{kcachegrind}} '{{ callgrind_out_file }}'

# Generate a statement coverage report in `target/cov/`
kcov:
	#!/bin/sh
	# Adapted from:
	# - http://sunjay.ca/2016/07/25/rust-code-coverage
	# - https://users.rust-lang.org/t/tutorial-how-to-collect-test-coverages-for-rust-project/650
	#
	# As of July 2, 2016, there is no option to make rustdoc generate a runnable
	# test executable. That means that documentation tests will not show in your
	# coverage data. If you discover a way to run the doctest executable with kcov,
	# please open an Issue and we will add that to these instructions.
	# -- https://github.com/codecov/example-rust

	# Ensure that kcov can see totally unused functions without clobbering regular builds
	# Adapted from:
	# - http://stackoverflow.com/a/38371687/435253
	# - https://gist.github.com/dikaiosune/07177baf5cea76c27783efa55e99da89
	export CARGO_TARGET_DIR="target/kcov"
	export RUSTFLAGS='-C link-dead-code'
	kcov_path="$CARGO_TARGET_DIR/html"

	if [ "$#" -gt 0 ]; then shift; fi # workaround for "can't shift that many" being fatal in dash
	cargo test --no-run || exit $?
	rm -rf "$kcov_path"

	for file in "$CARGO_TARGET_DIR"/"$CARGO_BUILD_TARGET"/debug/$_pkgname-*; do
		if [ -x "$file" ]; then
			outpath="$kcov_path/$(basename "$file")"
			mkdir -p "$outpath"
			rustig -f --binary "$file" || true
			kcov --exclude-pattern=/.cargo,/usr/lib --verify "$outpath" "$file" "$@"
		elif echo "$file" | grep -F -e '-*'; then
			echo "No build files found for coverage!"
			exit 1
		fi
	done

# Alias for cargo-edit's `cargo rm` which regenerates local API docs afterwards
rm +args="":
	{{_cargo}} rm {{args}}
	just doc

# Convenience alias for opening a crate search on lib.rs in the browser
search +args="":
	xdg-open "https://lib.rs/search?q={{args}}"

# Alias for `cargo test`
test +args="":
	{{_cargo}} test -q {{_build_flags}} {{args}}

# Alias for cargo-edit's `cargo update` which regenerates local API docs afterwards
update +args="":
	{{_cargo}} update {{args}}
	just doc

	# TODO: https://users.rust-lang.org/t/howto-sanitize-your-rust-code/9378

# -- Local Builds --

# Alias for `cargo build`
build:
	@echo "\n--== Building with {{channel}} for {{CARGO_BUILD_TARGET}} (features: {{features}}) ==--\n"
	{{_cargo}} build {{_build_flags}}

# Install the un-packed binary and a manpage
install: dist-supplemental
	@# Install the manpage
	cp dist/{{ _pkgname }}.1.gz {{ manpage_dir }}/{{ _pkgname }}.1.gz || true
	@# Install the command to ~/.cargo/bin
	{{_cargo}} install --path . --force --features="{{features}}"

# Alias for `cargo run -- {{args}}`
run +args="":
	{{_cargo}} run {{_build_flags}} -- {{args}}

# Remove any files installed by the `install` task (but leave any parent directories created)
uninstall:
	@# TODO: Implement the proper fallback chain from `cargo install`
	rm ~/.cargo/bin/{{ _pkgname }} || true
	rm {{ manpage_dir }}/{{ _pkgname }}.1.gz || true

# -- Release Builds --

# Make a release build and compress the resulting binary
build-dist:
	@echo "\n--== Building with {{channel}} for {{CARGO_BUILD_TARGET}} (features: {{features}}) ==--\n"
	{{_cargo}} build --release {{_build_flags}}

	@# Don't modify the original "cargo" output. That confuses cargo somehow.
	@printf "\n--== Compressing With UPX ==--\n"
	@# Allow upx to fail in case the user wants to force no UPXing by leaving it uninstalled
	cp "{{_rls_bin_path}}" "{{_rls_bin_path}}.packed"
	upx {{upx_flags}} "{{_rls_bin_path}}.packed" || true
	@# Display the resulting file sizes so we can keep an eye on them
	@# (Separate `ls` invocations are used to force the display ordering)
	@printf "\n--== Final Result ==--\n"
	@ls -1sh "{{_rls_bin_path}}"
	@ls -1sh "{{_rls_bin_path}}.packed"
	@printf "\n"


# Build the a manpage, and put it in `dist/`
dist-supplemental:
	mkdir -p dist
	@# Generate manpage and store it gzipped in dist/
	@# (This comes last so the earlier calls to `cargo run` will get the compiler warnings out)
	help2man -N '{{_cargo}} run {{_build_flags}} --' \
		| gzip -9 > dist/{{ _pkgname }}.1.gz || true

# Call `dist-supplemental` and `build-dist` and copy the packed binary to `dist/`
dist: build-dist dist-supplemental
	@# Copy the packed command to dist/
	cp  "{{ _rls_bin_path }}.packed" dist/{{ _pkgname }} || \
	cp  "{{ _rls_bin_path }}" dist/{{ _pkgname }}

# -- Dependencies --

# Use `apt-get` to install dependencies `cargo` can't (except `kcov`)
install-apt-deps:
	sudo apt-get install binutils help2man kcachegrind upx valgrind

# `install-rustup-deps` and then `cargo install` tools
install-cargo-deps: install-rustup-deps
	@# Prevent "already installed" from causing a failure
	{{_cargo}} install cargo-audit || true
	{{_cargo}} install cargo-bloat || true
	{{_cargo}} install cargo-deadlinks || true
	{{_cargo}} install cargo-edit || true
	{{_cargo}} install cargo-geiger || true
	{{_cargo}} install cargo-outdated || true
	{{_cargo}} install --git https://github.com/Technolution/rustig rustig
	cargo +nightly install cargo-cov || true

# Install (don't update) nightly and `channel` toolchains, plus `CARGO_BUILD_TARGET`, clippy, and rustfmt
install-rustup-deps:
	@# Prevent this from gleefully doing an unwanted "rustup update"
	rustup toolchain list | grep -q '{{channel}}' || rustup toolchain install '{{channel}}'
	rustup toolchain list | grep -q nightly || rustup toolchain install nightly
	rustup target list | grep -q '{{CARGO_BUILD_TARGET}} (' || rustup target add '{{CARGO_BUILD_TARGET}}'
	rustup component list | grep -q 'clippy-\S* (' || rustup component add clippy
	rustup component list --toolchain nightly | grep 'rustfmt-\S* (' || rustup component add rustfmt --toolchain nightly

# Run `install-apt-deps` and `install-cargo-deps`. List what remains.
@install-deps: install-apt-deps install-cargo-deps
	echo
	echo "-----------------------------------------------------------"
	echo "IMPORTANT: You will need to install the following manually:"
	echo "-----------------------------------------------------------"
	echo " * Rust-compatible kcov (http://sunjay.ca/2016/07/25/rust-code-coverage)"

# Local Variables:
# mode: makefile
# End:

# vim: set ft=make textwidth=100 colorcolumn=101 noexpandtab sw=8 sts=8 ts=8 :
