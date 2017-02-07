//! Subcommand definitions

use std::fs::remove_file;
use std::io::ErrorKind as IOErrorKind;
use std::path::PathBuf;
use std::process::{Command, Stdio};

extern crate glob;
use self::glob::{glob_with,MatchOptions};

use errors::*;
use platform::{NotificationProvider, RawMediaProvider};

/// Sound to play on completion
/// TODO: Rearchitect once I've finished the basic port
const DONE_SOUND: &'static str = "/usr/share/sounds/KDE-Im-Nudge.ogg";

// TODO: Replace all of these format!-based filename builders with use of PathBuf

/// Dump a disc to as raw a BIN/TOC/CUE set as possible using cdrdao.
fn rip_bin<P: RawMediaProvider>(
        provider: &P,
        disc_name: &str,
        keep_tocfile: bool) -> Result<()> {

    // TODO: Unit-test this
    // TODO: Decide how to work in absolute paths
    // toc2cue doesn't handle spaces in filenames well, so swap in underscores
    let volbase = PathBuf::from(disc_name.replace(" ", "_"));
    let tocfile = volbase.with_extension("toc");
    let cuefile = volbase.with_extension("cue");

    // Rip it or die
    // TODO: Verify the "or die"
    subprocess_call!("cdrdao", "read-cd", "--read-raw",
                     "--driver", "generic-mmc-raw",
                     "--device", provider.device_path(),
                     "--datafile", volbase.with_extension("bin"), &tocfile)
        .chain_err(|| format!("Error while dumping BIN/TOC pair"))?;

    // Generate a .CUE file
    // TODO: Find a way to detect if an ISO would be equivalent
    // TODO: Detect if there are audio tracks and, if so, byte-swap
    Command::new("toc2cue").args(&[&tocfile, &cuefile]).stdout(Stdio::null()).status()
        .chain_err(|| format!("Could not generate {} file from {}",
            cuefile.to_string_lossy(), tocfile.to_string_lossy()))?;

    // XXX: Properly quote the cue file contents.
    // (an alernative to subbing in underscores)
    // sed -i 's@^FILE \([^"].*[^"]\) BINARY@FILE "\1" BINARY@' .cue

    // TODO: Audit when I want to die and when I want to keep going
    if !keep_tocfile {
        remove_file(&tocfile).chain_err(|| format!("Could not remove {}",
                                                   tocfile.to_string_lossy()))?;
    }

    // TODO: Better way to make audio tracks obvious
    let _ = subprocess_call!("cat", cuefile);
    Ok(())
}

/// Dump a disc to an ISO using ddrescue
fn rip_iso<P: RawMediaProvider>(provider: &P, disc_name: &str) -> Result<()> {
    // TODO: Deduplicate this with rip_bin
    let volbase = PathBuf::from(disc_name.replace(" ", "_"));  // For consistency with rip_bin
    let isofile = volbase.with_extension("iso");
    let logfile = volbase.with_extension("log");

    subprocess_call!("ddrescue", "-b", "2048", provider.device_path(), &isofile, &logfile)
        .chain_err(|| "Initial ddrescue run reported failure")?;
    subprocess_call!("ddrescue", "--direct", "-M", "-b", "2048", provider.device_path(),
                     &isofile, &logfile)
        .chain_err(|| "Second ddrescue pass reported failure")?;
    // TODO: Compare ddrescue to the reading modes of dvdiaster for recovering
    //       non-ECC-agumented discs.
    Ok(())
}

/// Rip an audio CD using cdparanoia
pub fn rip_audio<P: RawMediaProvider>(provider: &P, _: &str) -> Result<()> {
    // TODO: Decide on how to specify policy for skip-control options
    // TODO: Use morituri instead, since it does everything we want already
    //       https://github.com/thomasvs/morituri
    subprocess_call!("cdparanoia", "-B", "-d", provider.device_path())
        .chain_err(|| "Failed to extract CD audio properly")?;

    let options = MatchOptions { case_sensitive: false, ..Default::default() };

    // TODO: HumanSort before operating on them
    for wav_result in glob_with("*.wav", &options).expect("Hard-coded pattern is bad") {
        match wav_result.chain_err(|| format!("Could not glob path")) {
            Err(e) => return Err(e),
            Ok(path) => {
                // TODO: Tidy this up when I'm not so tired
                // TODO: The following should be async-dispatched in the background
                // TODO: Extend my subprocess_call! macro to accept a slice somehow
                // TODO: Add support for metadata retrieval and optional gain normalization
                // Encode tracks to FLAC
                Command::new("flac").arg("--best").arg(&path).status()
                    .chain_err(|| format!("Could not encode dumped WAV file to FLAC: {}",
                                          path.to_string_lossy()))?;
                remove_file(&path).or_else(|e|
                    if e.kind() != IOErrorKind::NotFound { Ok(()) } else { Err(e) }
                ).chain_err(|| format!("Could not remove {}", path.to_string_lossy()))?
            }
        }

    }
    Ok(())
}

// -- interactive --

/// Robustly prompt the user for a CD key and record it in `cd_key.txt`
pub fn get_cd_key<P: RawMediaProvider>(provider: &P, disc_name: &str) -> Result<()> {
    unimplemented!();
}

// -- subcommands --
// TODO: Make these as asynchronous as possible

/// Subcommand to rip a CD-ROM
pub fn rip_cd<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<()> {
    // TODO: Make this take options so I can ask for BIN or ISO
    rip_bin(&provider, disc_name, true)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(&provider, disc_name)
}

/// Subcommand to recover a damaged CD
pub fn rip_damaged<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<()> {
    // TODO: Look into integrating dvdisaster
    rip_bin(&provider, disc_name, true)?;
    rip_iso(&provider, disc_name)?;
    rip_audio(&provider, disc_name)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(&provider, disc_name)
}

/// Subcommand to rip a DVD-ROM
pub fn rip_dvd<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<()> {
    rip_iso(&provider, disc_name)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(&provider, disc_name)
}

/// Subcommand to rip a PlayStation (PSX/PS1) disc
pub fn rip_psx<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<()> {
    rip_bin(&provider, disc_name, true)
}

/// Subcommand to rip a Playstation 2 (PS2) disc
pub fn rip_ps2<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<()> {
    rip_iso(&provider, disc_name)
}
