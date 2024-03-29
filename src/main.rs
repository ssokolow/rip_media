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
#![allow(clippy::single_call_fn)]
//
// Enforce my policy of only allowing it in my own code as a last resort
#![forbid(unsafe_code)]

// 3rd-party imports
use clap::Parser;
use log::error;

// Local imports
mod app;
mod platform;
mod subcommands;
mod validators;

/// Boilerplate to parse command-line arguments, set up logging, and handle bubbled-up `Error`s.
///
/// See `app::main` for the application-specific logic.
fn main() {
    // Parse command-line arguments (exiting on parse error, --version, or --help)
    let opts = app::CliOpts::parse();

    // Configure logging output so that -q is "decrease verbosity" rather than instant silence
    #[allow(clippy::expect_used)]
    stderrlog::new()
        .module(module_path!())
        .verbosity(opts.verbose.log_level_filter())
        .timestamp(opts.timestamp.unwrap_or(stderrlog::Timestamp::Off))
        .init()
        .expect("initialize logging output");

    if let Err(ref e) = app::main(opts) {
        // Write the top-level error message, then chained errors, then backtrace if available
        error!("error: {}", e);
        for cause in e.chain() {
            error!("caused by: {}", cause);
        }

        // Exit with a nonzero exit code
        // TODO: Decide how to allow code to set this to something other than 1
        std::process::exit(1);
    }
}

// vim: set sw=4 sts=4 expandtab :
