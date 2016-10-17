#[macro_use]
extern crate clap;
use clap::{App,Arg,SubCommand};

mod validators;

// TODO: Allow overriding in a config file
const DEFAULT_INPATH: &'static str = "/dev/sr0";

fn main() {
    App::new("rip_media")
        .about("Simple frontend for backing up physical media")
        .version(crate_version!())

        // -- Common Arguments --
        .args(&[
              Arg::from_usage("[inpath] -i --inpath=<PATH>")
                .global(true)
                .empty_values(false)
                .default_value(DEFAULT_INPATH)
                .validator(validators::path_readable)
                .help("Path to source medium (device, image file, etc.)"),
            Arg::from_usage("[outdir] -o --outdir=<PATH>")
                .global(true)
                .empty_values(false)
                .default_value(".")  // XXX: Look for an os.curdir equivalent
                .validator(validators::dir_writable)
                .help("Path to parent directory for output file(s)"),
            Arg::from_usage("[name] --name=[NAME]")
                .global(true)
                .empty_values(false)
                .validator(validators::filename_valid)
                .help("Specify the output file/folder name \
                       [default: <the volume label>]"),
                // TODO: Decide how to combine this default with --set-size
            Arg::from_usage("[set_size] --set-size=<NUM>")
                .global(true)
                .empty_values(false)
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
