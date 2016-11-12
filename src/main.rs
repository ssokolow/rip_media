// Use musl's malloc when building on nightly for maximum size reduction
#![cfg_attr(feature="nightly", feature(alloc_system))]
#[cfg(feature="nightly")]
extern crate alloc_system;

#[macro_use]
extern crate clap;
use clap::{App,AppSettings,Arg,SubCommand};

mod validators;

// TODO: Allow overriding in a config file
const DEFAULT_INPATH: &'static str = "/dev/sr0";

/// # Development Policy
/// Clap validators for references like filesystem paths (as opposed to self-contained
/// data like set sizes) are to be used only to improving the user experience by
/// maximizing the chance that bad data will be caught early.
///
/// To avoid vulnerabilities based on race conditions or shortcomings in functions like
/// access() (which will falsely claim "/" is writable), all "reference data" must be
/// validated (and failures handled) on **every** use.

/// Wrapper for Arg::from_usage to deduplicate setting a few things all args have
/// TODO: Does Clap provide a more proper way to set defaults than this?
fn arg_from_usage(usage: &str) -> Arg {
    Arg::from_usage(usage)
        .global(true)
        .empty_values(false)
}

fn main() {
    App::new("rip_media")
        .about("Simple frontend for backing up physical media")
        .version(crate_version!())
        .settings(&[
            AppSettings::GlobalVersion,
            AppSettings::SubcommandRequiredElseHelp])

        // -- Common Arguments --
        .args(&[
              arg_from_usage("[inpath] -i --inpath=<PATH>")
                .default_value(DEFAULT_INPATH)
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
        .get_matches();
}
