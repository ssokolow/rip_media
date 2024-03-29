//! Subcommand definitions

use std::fs::remove_file;
use std::io::ErrorKind as IOErrorKind;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use glob::{glob_with, MatchOptions};

use crate::platform::{MediaProvider, NotificationProvider, RawMediaProvider, DEFAULT_TIMEOUT};

use crate::subprocess_call;

/// Sound to play on completion
/// TODO: Rearchitect once I've finished the basic port
const DONE_SOUND: &str = "/usr/share/sounds/KDE-Im-Nudge.ogg";

/// Sound to play on failure
const FAIL_SOUND: &str = "/usr/share/sounds/KDE-K3B-Finish-Error.ogg";

/// Dump a disc to as raw a BIN/TOC/CUE set as possible using cdrdao.
pub fn rip_bin<P: RawMediaProvider>(
    provider: &P,
    disc_name: &str,
    keep_tocfile: bool,
) -> Result<()> {
    // TODO: Unit-test this
    // TODO: Decide how to work in absolute paths
    // toc2cue doesn't handle spaces in filenames well, so swap in underscores
    let volbase = PathBuf::from(disc_name.replace(' ', "_"));
    let tocfile = volbase.with_extension("toc");
    let cuefile = volbase.with_extension("cue");

    // Rip it or die
    // TODO: Verify the "or die"
    subprocess_call!(
        "cdrdao",
        "read-cd",
        "--read-raw",
        "--driver",
        "generic-mmc-raw",
        "--device",
        provider.device_path(),
        "--datafile",
        volbase.with_extension("bin"),
        &tocfile
    )
    .with_context(|| "Error while dumping BIN/TOC pair")?;

    // Generate a .CUE file
    // TODO: Find a way to detect if an ISO would be equivalent
    // TODO: Detect if there are audio tracks and, if so, byte-swap
    Command::new("toc2cue")
        .args(&[&tocfile, &cuefile])
        .stdout(Stdio::null())
        .status()
        .with_context(|| {
            format!(
                "Could not generate {} file from {}",
                cuefile.to_string_lossy(),
                tocfile.to_string_lossy()
            )
        })?;

    // XXX: Properly quote the cue file contents.
    // (an alernative to subbing in underscores)
    // sed -i 's@^FILE \([^"].*[^"]\) BINARY@FILE "\1" BINARY@' .cue

    // TODO: Audit when I want to die and when I want to keep going
    if !keep_tocfile {
        remove_file(&tocfile)
            .with_context(|| format!("Could not remove {}", tocfile.to_string_lossy()))?;
    }

    // TODO: Do this without shelling out, then find a better way to make audio tracks obvious
    let _ = subprocess_call!("cat", cuefile);
    Ok(())
}

/// Dump a disc to an ISO using ddrescue
pub fn rip_iso<P: RawMediaProvider>(provider: &P, disc_name: &str) -> Result<()> {
    // TODO: Deduplicate this with rip_bin
    let volbase = PathBuf::from(disc_name.replace(' ', "_")); // For consistency with rip_bin
    let isofile = volbase.with_extension("iso");
    let logfile = volbase.with_extension("log");

    subprocess_call!("ddrescue", "-b", "2048", provider.device_path(), &isofile, &logfile)
        .with_context(|| "Initial ddrescue run reported failure")?;
    subprocess_call!(
        "ddrescue",
        "--direct",
        "-M",
        "-b",
        "2048",
        provider.device_path(),
        &isofile,
        &logfile
    )
    .with_context(|| "Second ddrescue pass reported failure")?;
    // TODO: Compare ddrescue to the reading modes of dvdiaster for recovering
    //       non-ECC-agumented discs.
    Ok(())
}

/// Rip an audio CD using cdparanoia
pub fn rip_audio<P: RawMediaProvider>(provider: &mut P, _: &str) -> Result<()> {
    // TODO: Decide on how to specify policy for skip-control options
    // TODO: Use whipper instead, since it does everything we want already
    //       https://github.com/JoeLametta/whipper
    subprocess_call!("cdparanoia", "-B", "-d", provider.device_path())
        .with_context(|| "Failed to extract CD audio properly")?;

    let options = MatchOptions { case_sensitive: false, ..Default::default() };

    // TODO: HumanSort before operating on them
    #[allow(clippy::expect_used)]
    for wav_result in glob_with("*.wav", options).expect("hard-coded pattern is valid") {
        match wav_result.with_context(|| "Could not glob path") {
            Err(e) => return Err(e),
            Ok(path) => {
                // TODO: Tidy this up when I'm not so tired
                // TODO: The following should be async-dispatched in the background
                // TODO: Extend my subprocess_call! macro to accept a slice somehow
                // TODO: Add support for metadata retrieval and optional gain normalization
                // Encode tracks to FLAC
                Command::new("flac").arg("--best").arg(&path).status().with_context(|| {
                    format!("Could not encode dumped WAV file to FLAC: {}", path.to_string_lossy())
                })?;
                remove_file(&path).or_else(|e|
                    // FIXME: What was the rationale for the following?
                    if e.kind() == IOErrorKind::NotFound { Err(e) } else { Ok(()) })
                    .with_context(|| format!("Could not remove {}", path.to_string_lossy()))?;
            },
        }
    }
    Ok(())
}

// -- interactive --

/** Ensure we have a volume name, even if it requires manual input
 *
 * Will use (in priority order):
 *
 *  * The `name` argument
 *  * `provider.volume_label()`
 *  * Prompted user input
 *
 *  TODO: Redesign this so it can defer until the end by using mkdtemp()
 *        for maximum convenience and robustness in the face of a busy user
 */
pub fn ensure_vol_label<P: MediaProvider + NotificationProvider>(
    provider: &P,
    name: Option<&str>,
) -> String {
    if let Some(x) = name {
        return x.to_owned();
    }

    // Fall back to prompting (and ensure we get a non-empty name)
    // TODO: but do it with a timeout so the user can run the script and then take
    //       their time loading the disc if they prefer that order of operations
    let mut name_str = "".to_owned();
    while name_str.trim().is_empty() {
        // Do this inside the loop so I can press Enter to retry reading
        // TODO: Think of better UX for this
        name_str = provider.volume_label().unwrap_or_default().trim().to_owned();

        if name_str.is_empty() {
            // FIXME: Do I make this fallible or swallow the error?
            // name_str = provider.read_line("Disc Name: ")?.trim().to_owned();
            unimplemented!()
        }
    }

    name_str
}

/// Robustly prompt the user for a CD key and record it in `cd_key.txt`
pub fn get_cd_key<P: NotificationProvider>(provider: &P, disc_name: &str) -> Result<()> {
    loop {
        let key = provider
            .read_line(&format!("please enter cd-key for {} (enter for none): ", disc_name))?;
        let trimmed = key.trim();

        // TODO: Have a non-rustyline one for simple y/n or Enter stuff.
        let confirm = if trimmed.is_empty() {
            provider.read_line("no cd key. is this correct? (y/n): ")?
        } else {
            provider.read_line(&format!("please confirm \"{}\" (y/n): ", trimmed))?
        };

        if confirm.to_lowercase() == "y" {
            if !trimmed.is_empty() {
                unimplemented!();
                // with open('cd_key.txt', 'w') as fobj:
                //     fobj.write('%s\n' % key)
            }
            break;
        }
    }
    Ok(())
}

// -- subcommands --
// TODO: Make these as asynchronous as possible

/// Subcommand to rip a CD-ROM
pub fn rip_cd<P: RawMediaProvider + NotificationProvider>(
    provider: &mut P,
    disc_name: &str,
) -> Result<()> {
    // TODO: Make this take options so I can ask for BIN or ISO
    rip_bin(provider, disc_name, true)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(provider, disc_name)
}

/// Subcommand to recover a damaged CD
pub fn rip_damaged<P: RawMediaProvider + NotificationProvider>(
    provider: &mut P,
    disc_name: &str,
) -> Result<()> {
    // TODO: Look into integrating dvdisaster
    rip_bin(provider, disc_name, true)?;
    rip_iso(provider, disc_name)?;
    rip_audio(provider, disc_name)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(provider, disc_name)
}

/// Subcommand to rip a DVD-ROM
pub fn rip_dvd<P: RawMediaProvider + NotificationProvider>(
    provider: &mut P,
    disc_name: &str,
) -> Result<()> {
    rip_iso(provider, disc_name)?;
    let _ = provider.play_sound(DONE_SOUND);
    get_cd_key(provider, disc_name)
}

/// Subcommand to rip a Playstation (PSX/PS1) disc
pub fn rip_psx<P: RawMediaProvider + NotificationProvider>(
    provider: &mut P,
    disc_name: &str,
) -> Result<()> {
    rip_bin(provider, disc_name, true)
}

/// Subcommand to rip a Playstation 2 (PS2) disc
pub fn rip_ps2<P: RawMediaProvider + NotificationProvider>(
    provider: &mut P,
    disc_name: &str,
) -> Result<()> {
    rip_iso(provider, disc_name)
}

/// Top-level orchestration for doing a ripping run on a single disc
/// TODO: Provide prompting via a swappable service provider similar to APT's.
pub fn rip<P, F>(plat_provider: &mut P, mode_func: F, name: Option<&str>) -> Result<()>
where
    P: MediaProvider + NotificationProvider,
    F: Fn(&mut P, &str) -> Result<()>,
{
    // TODO: Have a non-rustyline one for simple y/n or Enter stuff.
    plat_provider.read_line("Insert disc and press Enter...")?;

    // TODO: Perhaps a mode where this presses Enter for you after 30 seconds
    //       if the disc's serial number has changed?
    plat_provider.load()?;
    plat_provider.wait_for_ready(&Duration::new(DEFAULT_TIMEOUT, 0))?;
    plat_provider.unmount()?; // Ensure we can get exclusive access to the disc

    let name_str = ensure_vol_label(plat_provider, name);
    assert!(!name_str.trim().is_empty()); // Guard against empty names
                                          // with _containing_workdir(disc_name):
    mode_func(plat_provider, &name_str).map_err(|e| {
        let _ = plat_provider.play_sound(FAIL_SOUND);
        e
    })?;

    // Notify completion and eject
    // TODO: Redesign to deduplicate the audio in PC-related modes.
    let _ = plat_provider.play_sound(DONE_SOUND);
    sleep(Duration::new(2, 0)); // Give me time to reach for the door if it got closed
    let _ = plat_provider.eject(); // TODO: Notify failure here

    // TODO: Call ['par2create', '-n1', '%s.par2' % name_str, glob.glob('*')]

    // TODO: Optionally compress the image as strongly as possible
    // ['7z', 'a', '-t7z', '-m0=lzma', '-mx=9', '-mfb=64', '-md=32m', '-ms=on',
    //  '%s.7z' % name_str, name_str] && shutil.rmtree(name_str)

    Ok(())
}

// vim: set sw=4 sts=4 :
