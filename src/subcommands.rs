//! Subcommand definitions

use std::error::Error;
use std::fs::remove_file;
use std::io::ErrorKind;
use std::io::Result as IOResult;
use std::process::{Command, Stdio};
use std::result::Result;

extern crate glob;
use self::glob::{glob_with,MatchOptions};

use platform::{NotificationProvider, RawMediaProvider};

/// Sound to play on completion
/// TODO: Rearchitect once I've finished the basic port
const DONE_SOUND: &'static str = "/usr/share/sounds/KDE-Im-Nudge.ogg";

// TODO: Replace all of these format!-based filename builders with use of PathBuf

/// Dump a disc to as raw a BIN/TOC/CUE set as possible using cdrdao.
fn rip_bin<P: RawMediaProvider>(
        provider: &P,
        disc_name: &str,
        keep_tocfile: bool) -> IOResult<()> {

    let volname = disc_name.replace(" ", "_");
    let tocfile = format!("{}.toc", volname);
    let cuefile = format!("{}.cue", volname);

    // Rip it or die
    // TODO: ...or die
    subprocess_call!("cdrdao", "read-cd", "--read-raw",
                     "--driver", "generic-mmc-raw",
                     "--device", provider.device_path(),
                     "--datafile", format!("{}.bin", volname), &tocfile);

    // Generate a .CUE file
    // TODO: Find a way to detect if an ISO would be equivalent
    // TODO: Detect if there are audio tracks and, if so, byte-swap
    Command::new("toc2cue").args(&[&tocfile, &cuefile]).stdout(Stdio::null()).status()?;

    // XXX: Properly quote the cue file contents.
    // (an alernative to subbing in underscores)
    // sed -i 's@^FILE \([^"].*[^"]\) BINARY@FILE "\1" BINARY@' .cue

    if keep_tocfile != true {
        remove_file(tocfile)?;
    }

    // TODO: Better way to make audio tracks obvious
    let _ = subprocess_call!("cat", cuefile);
    Ok(())
}

/// Dump a disc to an ISO using ddrescue
fn rip_iso<P: RawMediaProvider>(provider: &P, disc_name: &str) -> Result<(), String> {
    let volname = disc_name.replace(" ", "_");  // For consistency with rip_bin
    subprocess_call!("ddrescue", "-b", "2048", provider.device_path(),
                     format!("{}.iso", volname), format!("{}.log", volname))?;
    subprocess_call!("ddrescue", "--direct", "-M", "-b", "2048", provider.device_path(),
                     format!("{}.iso", volname), format!("{}.log", volname))
    // TODO: Compare ddrescue to the reading modes of dvdiaster for recovering
    //       non-ECC-agumented discs.
}

/// Rip an audio CD using cdparanoia
pub fn rip_audio<P: RawMediaProvider>(provider: &P, _: &str) -> Result<(), String> {
    // TODO: Decide on how to specify policy for skip-control options
    // TODO: Use morituri instead, since it does everything we want already
    //       https://github.com/thomasvs/morituri
    subprocess_call!("cdparanoia", "-B", "-d", provider.device_path())?;

    let options = MatchOptions { case_sensitive: false, ..Default::default() };

    // TODO: HumanSort before operating on them
    for wav_result in glob_with("*.wav", &options).expect("Hard-coded pattern is bad") {
        match wav_result {
            Err(e) => return Err(e.description().to_owned()),  // TODO: error-chain
            Ok(path) => {
                // TODO: Tidy this up when I'm not so tired
                // TODO: The following should be async-dispatched in the background
                // TODO: Extend my subprocess_call! macro to accept a slice somehow
                // TODO: Add support for metadata retrieval and optional gain normalization
                // Encode tracks to FLAC
                Command::new("flac").arg("--best").arg(&path).status().map_err(
                    |e| e.description().to_owned())?;
                if let Err(e) = remove_file(path) {
                    if e.kind() != ErrorKind::NotFound {
                        return Err(e.description().to_owned())
                    }
                }
            }
        }

    }
    Ok(())
}

// -- interactive --

/// Robustly prompt the user for a CD key and record it in `cd_key.txt`
pub fn get_cd_key<P: RawMediaProvider>(provider: &P, disc_name: &str) -> Result<(), String> {
    unimplemented!();
}

// -- subcommands --
// TODO: Make these as asynchronous as possible

/// Subcommand to rip a CD-ROM
pub fn rip_cd<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<(), String> {
    // TODO: Make this take options so I can ask for BIN or ISO
    rip_bin(&provider, disc_name, true).map_err(|e| e.description().to_owned())?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(&provider, disc_name)
}

/// Subcommand to recover a damaged CD
pub fn rip_damaged<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<(), String> {
    // TODO: Look into integrating dvdisaster
    rip_bin(&provider, disc_name, true).map_err(|e| e.description().to_owned())?;
    rip_iso(&provider, disc_name)?;
    rip_audio(&provider, disc_name)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(&provider, disc_name)
}

/// Subcommand to rip a DVD-ROM
pub fn rip_dvd<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<(), String> {
    rip_iso(&provider, disc_name)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(&provider, disc_name)
}

/// Subcommand to rip a PlayStation (PSX/PS1) disc
pub fn rip_psx<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<(), String> {
    rip_bin(&provider, disc_name, true).map_err(|e| e.description().to_owned())
}

/// Subcommand to rip a Playstation 2 (PS2) disc
pub fn rip_ps2<P: RawMediaProvider + NotificationProvider>(provider: P, disc_name: &str)
        -> Result<(), String> {
    rip_iso(&provider, disc_name)
}
