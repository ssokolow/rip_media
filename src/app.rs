//! [Eventually a] simple, robust script for making backups of various types of media
// Copyright 2016-2019, Stephan Sokolow

// Standard library imports
use std::borrow::Cow;
use std::path::{Component::CurDir, PathBuf};

// 3rd-party crate imports
use anyhow::Result;
use clap::Parser;

// Local Imports
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
#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct BoilerplateOpts {
    // -- Arguments used by main.rs --
    // TODO: Move these into a struct of their own in something like helpers.rs
    /// Decrease verbosity (-q, -qq, -qqq, etc.)
    #[clap(short, long, parse(from_occurrences))]
    pub quiet: usize,

    /// Increase verbosity (-v, -vv, -vvv, etc.)
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: usize,

    /// Display timestamps on log messages (sec, ms, ns, none)
    #[clap(short, long, value_name = "resolution")]
    pub timestamp: Option<stderrlog::Timestamp>,
}

/// Command-line argument schema
// NOTE: long_about must begin with '\n' for compatibility with help2man
// FIXME: clap-rs issue #694
#[derive(Parser, Debug)]
#[clap(
    version,
    template = HELP_TEMPLATE,
    rename_all = "kebab-case",
    about = "\nSimple frontend for backing up physical media",
    setting = clap::AppSettings::GlobalVersion,
    setting = clap::AppSettings::SubcommandRequiredElseHelp
)]
pub struct CliOpts {
    #[allow(clippy::missing_docs_in_private_items)] // clap won't let us document this
    #[clap(flatten)]
    pub boilerplate: BoilerplateOpts,

    // -- Common Arguments --
    // TODO: Test (using something like `assert_cmd`) that inpath is required
    /// Path to source medium (device, image file, etc.)
    #[clap(
        short,
        long,
        global = true,
        empty_values = false,
        value_name = "PATH",
        required = false,
        value_parser,
        default_value = DEFAULT_INPATH
    )]
    inpath: PathBuf,

    /// Path to parent directory for output file(s)
    #[clap(
        short,
        long,
        parse(from_os_str),
        global = true,
        empty_values = false,
        value_name = "PATH",
        required = false,
        default_value_os = CurDir.as_os_str()
    )]
    outdir: PathBuf,

    /// Specify the output file/folder name [default: <the volume label>]
    #[clap(long, global = true, empty_values = false, value_name = "NAME")]
    name: Option<String>, // TODO: Decide how to combine this default with --set-size

    /// Number of discs/cartridges/etc. to process under the same name
    /// (eg. multi-disc games/albums)
    #[clap(long, global = true, empty_values = false, value_name = "NUM", default_value = "1",
        value_parser = clap::value_parser!(u16).range(1..))]
    set_size: u16,

    /// Which subcommand to invoke
    #[clap(subcommand)]
    cmd: Command,
}

/// Valid subcommands
#[allow(clippy::upper_case_acronyms)]
#[derive(Parser, Debug)]
#[clap(
    rename_all = "kebab-case",
    about = "\nSimple frontend for backing up physical media",
    template = HELP_TEMPLATE)]
pub enum Command {
    /// Rip an audio CD
    #[clap(display_order = 1, template = HELP_TEMPLATE)]
    Audio,

    /// Rip a PC CD-ROM
    #[clap(display_order = 1, template = HELP_TEMPLATE)]
    CD,

    /// Rip a PC DVD-ROM
    #[clap(display_order = 1, template = HELP_TEMPLATE)]
    DVD,

    /// Rip a Sony PlayStation (PSX) disc in a PCSX/mednafen-compatible format
    #[clap(display_order = 1, template = HELP_TEMPLATE)]
    PSX,

    /// Rip a Sony PlayStation 2 disc into a PCSX2-compatible format
    #[clap(display_order = 1, template = HELP_TEMPLATE)]
    PS2,

    /// Rip a cartridge connected to the PC via a Retrode
    #[clap(display_order = 2, template = HELP_TEMPLATE)]
    Retrode,

    /// Rip a UMD via a USB-connected PSP running custom firmware
    #[clap(display_order = 2, template = HELP_TEMPLATE)]
    UMD,

    /// Validate and process a disc image dumped by a Wii running CleanRip
    #[clap(display_order = 2, template = HELP_TEMPLATE)]
    Cleanrip {
        // TODO: Can I make this an --in-place option shared among subcommands?
        /// Only run the hash-validation without processing further
        #[clap(long)]
        just_validate: bool,
    },

    /// Recover a damaged CD
    #[clap(display_order = 1, template = HELP_TEMPLATE)]
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
}

// vim: set sw=4 sts=4 :
