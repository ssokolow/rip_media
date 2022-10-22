//! [Eventually a] simple, robust script for making backups of various types of media
// Copyright 2016-2019, Stephan Sokolow

// Standard library imports
use std::borrow::Cow;
use std::path::{Component::CurDir, PathBuf};

// 3rd-party crate imports
use structopt::{clap, StructOpt};

#[allow(unused_imports)] // TEMPLATE:REMOVE
use log::{debug, error, info, trace, warn};

// Local Imports
use crate::errors::Result;
use crate::validators::{dir_writable, filename_valid_portable, path_readable};
use crate::{platform, subcommands};

/// The verbosity level when no `-q` or `-v` arguments are given, with `0` being `-q`
pub const DEFAULT_VERBOSITY: usize = 1;

// TODO: The retrode path should incorporate the current username
// TODO: Allow overriding in a config file (Perhaps via .env with
//       https://siciarz.net/24-days-rust-environment-variables)
/// Default path to read from if none is specified
const DEFAULT_INPATH: &str = "/dev/sr0";
// const RETRODE_INPATH: &str = "/media/ssokolow/RETRODE";
// TODO: Use libblkid to look up RETRODE at runtime:
// https://www.kernel.org/pub/linux/utils/util-linux/v2.21/libblkid-docs/libblkid-Tags-and-Spec-evaluation.html
//
// const VOLUME_SIZE: u64 = 4480 * 1024 * 1024;  // DVD+R, given ISO+UDF overhead

// TODO: Audit all of my explicit lifetimes and give them descriptive names
// https://scribbles.pascalhertleif.de/elegant-apis-in-rust.html

/// Modified version of Clap's default template for proper help2man compatibility
const HELP_TEMPLATE: &str = "{bin} {version}

{about}

USAGE:
    {usage}

{all-args}
";

/// Options used by boilerplate code
// TODO: Move these into a struct of their own in something like helpers.rs
#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct BoilerplateOpts {
    // -- Arguments used by main.rs --
    // TODO: Move these into a struct of their own in something like helpers.rs
    /// Decrease verbosity (-q, -qq, -qqq, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    pub quiet: usize,

    /// Increase verbosity (-v, -vv, -vvv, etc.)
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Display timestamps on log messages (sec, ms, ns, none)
    #[structopt(short, long, value_name = "resolution")]
    pub timestamp: Option<stderrlog::Timestamp>,

    /// Write a completion definition for the specified shell to stdout (bash, zsh, etc.)
    #[structopt(long, value_name = "shell")]
    pub dump_completions: Option<clap::Shell>,
}

/// Command-line argument schema
// NOTE: long_about must begin with '\n' for compatibility with help2man
// FIXME: clap-rs issue #694
#[derive(StructOpt, Debug)]
#[structopt(
    template = HELP_TEMPLATE,
    rename_all = "kebab-case",
    about = "\nSimple frontend for backing up physical media",
    global_setting = structopt::clap::AppSettings::ColoredHelp,
    setting = structopt::clap::AppSettings::GlobalVersion,
    setting = structopt::clap::AppSettings::SubcommandRequiredElseHelp
)]
pub struct CliOpts {
    #[allow(clippy::missing_docs_in_private_items)] // StructOpt won't let us document this
    #[structopt(flatten)]
    pub boilerplate: BoilerplateOpts,

    // -- Common Arguments --
    // TODO: Test (using something like `assert_cmd`) that inpath is required
    /// Path to source medium (device, image file, etc.)
    #[structopt(
        short,
        long,
        parse(from_os_str),
        global = true,
        empty_values = false,
        value_name = "PATH",
        required = false,
        validator_os = path_readable,
        default_value = DEFAULT_INPATH
    )]
    inpath: PathBuf,

    /// Path to parent directory for output file(s)
    #[structopt(
        short,
        long,
        parse(from_os_str),
        global = true,
        empty_values = false,
        value_name = "PATH",
        required = false,
        validator_os = dir_writable,
        default_value_os = CurDir.as_os_str()
    )]
    outdir: PathBuf,

    /// Specify the output file/folder name [default: <the volume label>]
    #[structopt(
        long,
        global = true,
        empty_values = false,
        value_name = "NAME",
        validator_os = filename_valid_portable
    )]
    name: Option<String>, // TODO: Decide how to combine this default with --set-size

    /// Number of discs/cartridges/etc. to process under the same name
    /// (eg. multi-disc games/albums)
    #[structopt(
        long,
        global = true,
        empty_values = false,
        value_name = "NUM",
        validator = valid_set_size,
        default_value = "1"
    )]
    set_size: usize,

    /// Which subcommand to invoke
    #[structopt(subcommand)]
    cmd: Command,
}

/// Valid subcommands
#[allow(clippy::upper_case_acronyms)]
#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case", template = HELP_TEMPLATE)]
pub enum Command {
    /// Rip an audio CD
    #[structopt(display_order = 1, template = HELP_TEMPLATE)]
    Audio,

    /// Rip a PC CD-ROM
    #[structopt(display_order = 1, template = HELP_TEMPLATE)]
    CD,

    /// Rip a PC DVD-ROM
    #[structopt(display_order = 1, template = HELP_TEMPLATE)]
    DVD,

    /// Rip a Sony PlayStation (PSX) disc in a PCSX/mednafen-compatible format
    #[structopt(display_order = 1, template = HELP_TEMPLATE)]
    PSX,

    /// Rip a Sony PlayStation 2 disc into a PCSX2-compatible format
    #[structopt(display_order = 1, template = HELP_TEMPLATE)]
    PS2,

    /// Rip a cartridge connected to the PC via a Retrode
    #[structopt(display_order = 2, template = HELP_TEMPLATE)]
    Retrode,

    /// Rip a UMD via a USB-connected PSP running custom firmware
    #[structopt(display_order = 2, template = HELP_TEMPLATE)]
    UMD,

    /// Validate and process a disc image dumped by a Wii running CleanRip
    #[structopt(display_order = 2, template = HELP_TEMPLATE)]
    Cleanrip {
        // TODO: Can I make this an --in-place option shared among subcommands?
        /// Only run the hash-validation without processing further
        #[structopt(long)]
        just_validate: bool,
    },

    /// Recover a damaged CD
    #[structopt(display_order = 1, template = HELP_TEMPLATE)]
    Damaged,
}

/// Program entry point
pub fn main(opts: CliOpts) -> Result<()> {
    let subcommand_func = match opts.cmd {
        Command::Audio => subcommands::rip_audio,
        Command::CD => subcommands::rip_cd,
        Command::DVD => subcommands::rip_dvd,
        Command::PSX => subcommands::rip_psx,
        Command::PS2 => subcommands::rip_ps2,
        Command::Damaged => subcommands::rip_damaged,
        e => panic!("TODO: Implement subcommand: {:?}", e),
    };

    // IDEA: Could I adapt the "parameterized impl for verified state machine"
    //       trick to compile-time verify that code which may be called in
    //       non-interactive-mode doesn't depend on interactive calls like
    //       prompt()?

    // TODO:
    // 1. Do common setup for disc set
    // 2. For each disc...
    //      ...call the ripping command appropriate to the subcommand

    //with _containing_workdir(args.outdir or os.getcwdu()):
    //    for _ in range(0, args.set_size):
    // TODO: Actually put set_size things in the same folder
    // TODO: Unify error-handling and replace expect() with ok_or() and ?
    let mut provider = platform::LinuxPlatformProvider::new(Cow::Borrowed(opts.inpath.as_os_str()));
    subcommands::rip(&mut provider, subcommand_func, opts.name.as_ref().map(String::as_ref))?;

    Ok(()) // TODO
}

/// TODO: Find a way to make *clap* mention which argument failed validation
///       so my validator can be generic (A closure, maybe?)
#[allow(clippy::needless_pass_by_value)]
pub fn valid_set_size(value: String) -> std::result::Result<(), String> {
    // I can't imagine needing more than u8... no harm in being flexible here
    if let Ok(num) = value.parse::<u32>() {
        if num >= 1u32 {
            return Ok(());
        }
        return Err(format!("Set size must be 1 or greater (not \"{}\")", value));
    }
    Err(format!("Set size must be an integer (whole number), not \"{}\"", value))
}

// Tests go below the code where they'll be out of the way when not the target of attention
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// TODO: Use a macro to generate the positionality/default-validation tests and also apply
    /// them to outdir
    ///
    #[test]
    fn inpath_has_expected_default_if_not_given() {
        let opts = CliOpts::from_iter(&["rip_media", "cd"]);
        assert!(
            opts.inpath == Path::new(DEFAULT_INPATH),
            "Expected default inpath to be {:?} but got {:?}",
            DEFAULT_INPATH,
            opts.inpath
        )
    }

    #[test]
    fn outdir_has_expected_default_if_not_given() {
        let opts = CliOpts::from_iter(&["rip_media", "cd"]);
        assert!(
            opts.outdir == Path::new(CurDir.as_os_str()),
            "Expected default outdir to be {:?} but got {:?}",
            CurDir.as_os_str(),
            opts.outdir
        )
    }

    #[test]
    /// Can override `DEFAULT_INPATH` when specifying -i before the subcommand
    fn test_can_override_inpath_before() {
        let opts = CliOpts::from_iter(&["rip_media", "-i/", "cd"]);
        assert!(
            opts.inpath == Path::new("/"),
            "\"-i/ cd\" should have produced \"/\" but actually produced \"{:?}\"",
            opts.inpath
        )
    }

    #[test]
    /// Can override `DEFAULT_INPATH` when specifying -i after the subcommand
    fn test_can_override_inpath_after() {
        let opts = CliOpts::from_iter(&["rip_media", "cd", "-i/"]);
        assert!(
            opts.inpath == Path::new("/"),
            "\"cd -i/\" should have produced \"/\" but actually produced \"{:?}\"",
            opts.inpath
        )
    }

    //#[test]
    ///// Validator doesn't get run on the default inpath if -i was specified
    //fn test_only_validates_inpath_to_be_used_before() {
    //    let defaults = AppConfig { inpath: Cow::Borrowed("/etc/shadow") };
    //    let matches = make_clap_parser(&defaults)
    //        .get_matches_from_safe(&["rip_media", "-i/", "cd"])
    //        .unwrap_or_else(|e| { panic!("Undesired failure on input: {}", e) });
    //    let inpath = matches.value_of("inpath").unwrap();
    //    assert!(inpath == "/",
    //            "\"cd -i/\" should have produced \"/\" but actually produced \"{}\"", inpath)
    //}

    //#[test]
    ///// Validator doesn't get run on the default inpath if -i was specified
    //fn test_only_validates_inpath_to_be_used_after() {
    //    let defaults = AppConfig { inpath: Cow::Borrowed("/etc/shadow") };
    //    let matches = make_clap_parser(&defaults)
    //        .get_matches_from_safe(&["rip_media", "cd", "-i/"])
    //        .unwrap_or_else(|e| { panic!("Undesired failure on input: {}", e) });
    //    let inpath = matches.value_of("inpath").unwrap();
    //    assert!(inpath == "/",
    //            "\"cd -i/\" should have produced \"/\" but actually produced \"{}\"", inpath)
    //}

    // TODO: More unit tests

    #[test]
    fn valid_set_size_requires_positive_base_10_numbers() {
        assert!(valid_set_size("".into()).is_err());
        assert!(valid_set_size("one".into()).is_err());
        assert!(valid_set_size("a".into()).is_err()); // not base 11 or above
        assert!(valid_set_size("0".into()).is_err());
        assert!(valid_set_size("-1".into()).is_err());
    }

    #[test]
    fn valid_set_size_requires_integers() {
        assert!(valid_set_size("-1.5".into()).is_err());
        assert!(valid_set_size("-0.5".into()).is_err());
        assert!(valid_set_size("0.5".into()).is_err());
        assert!(valid_set_size("1.5".into()).is_err());
    }

    #[test]
    fn valid_set_size_handles_out_of_range_sanely() {
        assert!(valid_set_size("5000000000".into()).is_err());
    }

    #[test]
    fn valid_set_size_basic_functionality() {
        assert!(valid_set_size("1".into()).is_ok());
        assert!(valid_set_size("9".into()).is_ok()); // not base 9 or below
        assert!(valid_set_size("5000".into()).is_ok()); // accept reasonably large numbers
    }
}

// vim: set sw=4 sts=4 :
