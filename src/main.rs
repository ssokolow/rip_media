//! [Eventually a] simple, robust script for dumping backups of various types of media
// Make clippy very strict. I'll opt out of the false positives as I hit them
#![cfg_attr(feature="cargo-clippy", warn(clippy_pedantic))]
#![cfg_attr(feature="cargo-clippy", warn(indexing_slicing))]
#![cfg_attr(feature="cargo-clippy", warn(integer_arithmetic))]

// Use musl's malloc when building on nightly for maximum size reduction
#![cfg_attr(feature="nightly", feature(alloc_system))]
#[cfg(feature="nightly")]
extern crate alloc_system;

/// libstd imports
use std::borrow::Cow;

/// clap-rs imports
#[macro_use]
extern crate clap;
use clap::{App,AppSettings,Arg,SubCommand};

/// Custom clap-rs input validators
mod validators;

// TODO: Allow overriding in a config file
/// Default path to read from if none is specified
const DEFAULT_INPATH: &'static str = "/dev/sr0";
/// Allow different defaults to be passed to unit tests
struct AppConfig<'a> {
    /// Device/file to dump from
    inpath: Cow<'a, str>,
}

impl<'a> Default for AppConfig<'a> {
    fn default() -> AppConfig<'a> {
        AppConfig { inpath: Cow::Borrowed(DEFAULT_INPATH) }
    }
}

/// # Development Policy
/// Clap validators for references like filesystem paths (as opposed to self-contained
/// data like set sizes) are to be used only to improving the user experience by
/// maximizing the chance that bad data will be caught early.
///
/// To avoid vulnerabilities based on race conditions or shortcomings in functions like
/// access() (which will falsely claim "/" is writable), all "reference data" must be
/// validated (and failures handled) on **every** use.
///
/// See Also:
///  http://blog.ssokolow.com/archives/2016/10/17/a-more-formal-way-to-think-about-validity-of-input-data/

/// Wrapper for `Arg::from_usage` to deduplicate setting a few things all args have
/// TODO: Does Clap provide a more proper way to set defaults than this?
fn arg_from_usage(usage: &str) -> Arg {
    Arg::from_usage(usage)
        .global(true)           // FIXME: clap-rs issue #694
        .empty_values(false)
}

/// Initialize the clap parser to be used by main() or unit tests
fn make_clap_parser<'a, 'b>(defaults: &'a AppConfig<'b>) -> App<'a, 'a> where 'a: 'b {
    App::new("rip_media")
        .about("Simple frontend for backing up physical media")
        .version(crate_version!())
        .settings(&[
            AppSettings::GlobalVersion,
            AppSettings::SubcommandRequiredElseHelp])

        // -- Common Arguments --
        .args(&[
              arg_from_usage("[inpath] -i --inpath=<PATH>")
                .default_value(&defaults.inpath)
                .validator(validators::path_readable)
                .help("Path to source medium (device, image file, etc.)"),
            arg_from_usage("[outdir] -o --outdir=<PATH>")
                .default_value(".")  // XXX: Look for an os.curdir equivalent
                .validator(validators::dir_writable)
                .help("Path to parent directory for output file(s)"),
            arg_from_usage("[name] --name=[NAME]")
                .validator(validators::filename_valid)
                .help("Specify the output file/folder name \
                       [default: <the volume label>]"),
                // TODO: Decide how to combine this default with --set-size
            arg_from_usage("[set_size] --set-size=<NUM>")
                .default_value("1")
                .validator(validators::set_size)
                .help("Number of discs/cartridges/etc. to process under the same \
                       name (eg. multi-disc games/albums)")])

        // -- Subcommands --
        // TODO: Ordering with a ton of explicit .display_order() calls
        .subcommands(vec![
            SubCommand::with_name("audio")
                .display_order(1)
                .about("Rip an audio CD"),
            SubCommand::with_name("cd")
                .display_order(1)
                .about("Rip a PC CD-ROM"),
            SubCommand::with_name("dvd")
                .display_order(1)
                .about("Rip a PC DVD-ROM"),
            SubCommand::with_name("psx")
                .display_order(1)
                .about("Rip a Sony PlayStation (PSX) disc into a PCSX/mednafen-\
                       compatible format"),
            SubCommand::with_name("ps2")
                .display_order(2)
                .about("Rip a Sony PlayStation 2 disc into a PCSX2-compatible \
                        format"),
            SubCommand::with_name("retrode")
                .display_order(2)
                .about("Rip a cartridge connected to the PC via a Retrode"),
            SubCommand::with_name("cleanrip")
                .display_order(3)
                .about("Validate and process a disc image dumped by a Wii running \
                        CleanRip")
                .arg(Arg::with_name("just_validate")
                     .long("--just-validate")
                     .help("Only run the hash-validation without processing further")),
                // TODO: Can I make this an --in-place option shared among subcommands?
            SubCommand::with_name("damaged")
                .display_order(3)
                .about("Recover a damaged CD")])
}

/// Program entry point
fn main() {
    // TODO: Move all of this parsing into its own function so I can unit test it
    let defaults = AppConfig::default();
    let matches = make_clap_parser(&defaults).get_matches();

    match matches.subcommand_name() {
        Some(e) => println!("TODO: Implement subcommand: {}", e),
        None => unreachable!()
    }

    // TODO:
    // 1. Do common setup for disc set
    // 2. For each disc...
    //      ...call the ripping command appropriate to the subcommand
}

// Tests go below the code where they'll be out of the way when not the target of attention
#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use super::{AppConfig, make_clap_parser};

    #[test]
    /// Can override DEFAULT_INPATH when specifying -i before the subcommand
    fn test_can_override_inpath_before() {
        let defaults = AppConfig::default();
        let matches = make_clap_parser(&defaults).get_matches_from(&["rip_media", "-i/", "cd"]);
        let inpath = matches.value_of("inpath").unwrap();
        assert!(inpath == "/", "\"-i/ cd\" should have produced \"/\" \
                                       but actually produced \"{}\"", inpath)
    }

    #[test]
    /// Can override DEFAULT_INPATH when specifying -i after the subcommand
    fn test_can_override_inpath_after() {
        let defaults = AppConfig::default();
        let matches = make_clap_parser(&defaults).get_matches_from(&["rip_media", "cd", "-i/"]);
        let inpath = matches.value_of("inpath").unwrap();
        assert!(inpath == "/", "\"cd -i/\" should have produced \"/\" \
                                       but actually produced \"{}\"", inpath)
    }

    #[test]
    /// Validator doesn't get run on the default inpath if -i was specified
    fn test_only_validates_inpath_to_be_used_before() {
        let defaults = AppConfig { inpath: Cow::Borrowed("/etc/shadow") };
        let matches = make_clap_parser(&defaults).get_matches_from(&["rip_media", "-i/", "cd"]);
        let inpath = matches.value_of("inpath").unwrap();
        assert!(inpath == "/", "\"cd -i/\" should have produced \"/\" \
                                       but actually produced \"{}\"", inpath)
    }

    #[test]
    /// Validator doesn't get run on the default inpath if -i was specified
    fn test_only_validates_inpath_to_be_used_after() {
        let defaults = AppConfig { inpath: Cow::Borrowed("/etc/shadow") };
        let matches = make_clap_parser(&defaults).get_matches_from(&["rip_media", "cd", "-i/"]);
        let inpath = matches.value_of("inpath").unwrap();
        assert!(inpath == "/", "\"cd -i/\" should have produced \"/\" \
                                       but actually produced \"{}\"", inpath)
    }
}
// vim: set sw=4 sts=4 :
