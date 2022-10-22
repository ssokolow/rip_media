/*! [Eventually a] simple, robust script for making backups of various types of media

This file provided by [rust-cli-boilerplate](https://github.com/ssokolow/rust-cli-boilerplate)
*/
// Copyright 2017-2019, Stephan Sokolow

// `error_chain` recursion adjustment
#![recursion_limit = "1024"]
//
// Make rustc's built-in lints more strict and set clippy into a whitelist-based configuration so
// we see new lints as they get written (We'll opt back out selectively)
#![warn(clippy::all, clippy::complexity, clippy::correctness, clippy::pedantic, clippy::perf)]
#![warn(clippy::style, clippy::restriction)]
//
// Opt out of the lints I've seen and don't want
#![allow(clippy::blanket_clippy_restriction_lints, clippy::pattern_type_mismatch)]
#![allow(clippy::float_arithmetic, clippy::implicit_return, clippy::std_instead_of_core)]
#![allow(clippy::std_instead_of_alloc, clippy::unseparated_literal_suffix)]
#![allow(clippy::decimal_literal_representation, clippy::default_numeric_fallback)]
//
// Enforce my policy of only allowing it in my own code as a last resort
#![forbid(unsafe_code)]

// stdlib imports
use std::io;

// 3rd-party imports
mod errors;
use log::error;
use structopt::StructOpt;

// Local imports
mod app;
mod platform;
mod subcommands;
mod validators;

/// Boilerplate to parse command-line arguments, set up logging, and handle bubbled-up `Error`s.
///
/// Based on the `StructOpt` example from stderrlog and the suggested error-chain harness from
/// [quickstart.rs](https://github.com/brson/error-chain/blob/master/examples/quickstart.rs).
///
/// See `app::main` for the application-specific logic.
///
/// **TODO:** Consider switching to Failure and look into `impl Termination` as a way to avoid
///           having to put the error message pretty-printing inside main()
fn main() {
    // Parse command-line arguments (exiting on parse error, --version, or --help)
    let opts = app::CliOpts::from_args();

    // Configure logging output so that -q is "decrease verbosity" rather than instant silence
    let verbosity = opts
        .boilerplate
        .verbose
        .saturating_add(app::DEFAULT_VERBOSITY)
        .saturating_sub(opts.boilerplate.quiet);
    #[allow(clippy::expect_used)]
    stderrlog::new()
        .module(module_path!())
        .quiet(verbosity == 0)
        .verbosity(verbosity.saturating_sub(1))
        .timestamp(opts.boilerplate.timestamp.unwrap_or(stderrlog::Timestamp::Off))
        .init()
        .expect("initialize logging output");

    // If requested, generate shell completions and then exit with status of "success"
    if let Some(shell) = opts.boilerplate.dump_completions {
        app::CliOpts::clap().gen_completions_to(
            app::CliOpts::clap().get_bin_name().unwrap_or(env!("CARGO_PKG_NAME")),
            shell,
            &mut io::stdout(),
        );
        std::process::exit(0);
    };

    if let Err(ref e) = app::main(opts) {
        // Write the top-level error message, then chained errors, then backtrace if available
        error!("error: {}", e);
        for err in e.iter().skip(1) {
            error!("caused by: {}", err);
        }
        if let Some(backtrace) = e.backtrace() {
            error!("backtrace: {:?}", backtrace);
        }

        // Exit with a nonzero exit code
        // TODO: Decide how to allow code to set this to something other than 1
        std::process::exit(1);
    }
}

// vim: set sw=4 sts=4 expandtab :
