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
        .arg(Arg::with_name("inpath")
             .short("i")
             .long("inpath")
             .empty_values(false)
             .global(true)
             .value_name("PATH")
             .default_value(DEFAULT_INPATH)
             .validator(validators::path_readable)
             .help("Path to source medium (device, image file, etc.)"))
        .arg(Arg::with_name("outdir")
             .short("o")
             .long("outdir")
             .empty_values(false)
             .global(true)
             .value_name("PATH")
             .default_value(".")  // XXX: Look for an os.curdir equivalent
             // TODO: Custom validator to verify writability
             // https://github.com/kbknapp/clap-rs/blob/master/examples/15_custom_validator.rs
             .help("Path to parent directory for output file(s)"))
        .arg(Arg::with_name("name")
             .long("name")
             .empty_values(false)
             .global(true)
             .value_name("NAME")
             .validator(validators::filename_valid)
             .help("Specify the output file/folder name \
                   [default: <the volume label>]"))
             // TODO: Decide how to combine this default with --set-size
        .arg(Arg::with_name("set_size")
             .long("set-size")
             .empty_values(false)
             .global(true)
             .value_name("NUM")
             .default_value("1")
             .validator(validators::set_size)
             .help("Number of discs/cartridges/etc. to process under the same \
                    name (eg. multi-disc games/albums)"))

        // -- Subcommands --
        .subcommand(SubCommand::with_name("audio")
            .display_order(1)
            .about("Rip an audio CD"))
        .subcommand(SubCommand::with_name("cd")
            .display_order(1)
            .about("Rip a PC CD-ROM"))
        .subcommand(SubCommand::with_name("dvd")
            .display_order(1)
            .about("Rip a PC DVD-ROM"))
        .subcommand(SubCommand::with_name("psx")
            .display_order(1)
            .about("Rip a Sony PlayStation (PSX) disc into a PCSX/mednafen-\
                   compatible format"))
        .subcommand(SubCommand::with_name("ps2")
            .display_order(2)
            .about("Rip a Sony PlayStation 2 disc into a PCSX2-compatible \
                   format"))
        .subcommand(SubCommand::with_name("retrode")
            .display_order(2)
            .about("Rip a cartridge connected to the PC via a Retrode"))
        .subcommand(SubCommand::with_name("cleanrip")
            .display_order(3)
            .about("Validate and process a disc image dumped by a Wii running CleanRip")
            .arg(Arg::with_name("just_validate")
                 .long("--just-validate")
                 .help("Only run the hash-validation without processing further")))
        .subcommand(SubCommand::with_name("damaged")
            .display_order(3)
            .about("Recover a damaged CD"))
        .get_matches();
}
