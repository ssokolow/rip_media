//! Abstraction around the underlying OS functionality

extern crate rustyline;

use errors::*;

use std::borrow::Cow;
use std::env;
use std::ffi::{OsString, OsStr};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::io::Result as IOResult;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use self::rustyline::Editor;

/// Default timeout duration (in seconds)
pub const DEFAULT_TIMEOUT: u64 = 10;

/// Shorthand for calling subprocesses purely for side-effects
#[macro_export]
macro_rules! subprocess_call {
    ( $cmd:expr, $( $arg:expr ), * ) => {
        Command::new($cmd)
                $(.arg($arg))*
                .status().map(|_| ())
    }
}

/// Shorthand for reading byte substrings from `Seek`-ables
macro_rules! read_exact_at {
    ( $file:expr, $bytes:expr, $offset:expr ) => {
        {
            let mut buf = [0; $bytes];
            $file.seek($offset).chain_err(|| "Failed to seek")?;
            $file.read_exact(&mut buf)
                 .chain_err(|| format!("Could not read {} bytes from {:?}", $bytes, $file))?;
            buf
        }
    }
}

/// Port of Python's naive `abspath` to be used as a prelude to Path::display()
///
/// **WARNING:** This won't handle drive-relative paths on Windows properly. (ie. c:win.com)
///
/// See this thread for context:
///   https://www.reddit.com/r/rust/comments/5tmvti/how_can_i_get_the_full_path_of_a_file_from_a/
fn abspath<P: AsRef<Path> + ?Sized>(relpath: &P) -> IOResult<PathBuf> {
    env::current_dir().map(|p| p.join(relpath.as_ref()))
}

/// Interface for manipulating media devices such as DVD drives
/// TODO: Custom error type
pub trait MediaProvider {
    /// Eject the media if the hardware supports it
    fn eject(&mut self) -> Result<()>;

    /// Load the media if the hardware supports it
    fn load(&mut self) -> Result<()>;

    /// Unmount the media if mounted
    fn unmount(&mut self) -> Result<()>;

    /// Retrieve the volume label, if one is set
    fn volume_label(&self) -> Result<String>;

    /// Wait up to `timeout` seconds for the disc to be ready
    fn wait_for_ready(&self, timeout: &Duration) -> Result<()>;
}

/// Interface for platform providers which support exposing raw device paths
pub trait RawMediaProvider {
    /// Return an `OsString` which can be used by APIs or subprocesses to
    /// reference the device
    fn device_path(&self) -> OsString;
}

/// High-level interface for notifying the user via various system APIs
/// TODO: Refactor or rename this since prompt() isn't a notification.
pub trait NotificationProvider {
    /// Play the given audio file, if supported
    fn play_sound<P: AsRef<Path> + ?Sized>(&mut self, path: &P) -> Result<()>;

    /// Prompt the user for a line of input
    fn read_line(&self, prompt: &str) -> Result<String>;
}

/// `MediaProvider` implementation which operates on (possibly GUI-less) Linux systems
pub struct LinuxPlatformProvider<'a> {
    /// Device/file to operate on
    /// TODO: Consider storing a Path internally instead.
    device: Cow<'a, OsStr>,
}

impl<'a> LinuxPlatformProvider<'a> {
    /// Create a `LinuxPlatformProvider` for a given device path
    /// TODO: Figure out how to not require the Cow to be manually supplied (eg. From)
    /// TODO: Ask whether I'm using the proper naming convention for this
    pub fn new(device: Cow<OsStr>) -> LinuxPlatformProvider {
        // TODO: Validate this path
        LinuxPlatformProvider { device: device }
    }
}

impl<'a> RawMediaProvider for LinuxPlatformProvider<'a> {
    // TODO: Actually think about this API and refactor.
    fn device_path(&self) -> OsString { self.device.clone().into_owned() }
}

impl<'a> MediaProvider for LinuxPlatformProvider<'a> {
    fn eject(&mut self) -> Result<()> {
        subprocess_call!("eject", &self.device)
            .chain_err(|| format!("Could not eject {}", &self.device.to_string_lossy()))
    }

    fn load(&mut self) -> Result<()> {
        subprocess_call!("eject", "-t", &self.device).map(|_| ())
            .chain_err(|| format!("Could not load media for {}",
                                  &self.device.to_string_lossy()))
    }

    fn unmount(&mut self) -> Result<()> {
        subprocess_call!("umount", &self.device)
            .chain_err(|| format!("Could not unmount {}", &self.device.to_string_lossy()))
    }

    fn volume_label(&self) -> Result<String> {
        // TODO: Use UDisks2 via dbus
        //
        // XXX: Could use libblkid directly:
        // https://www.kernel.org/pub/linux/utils/util-linux/v2.21/libblkid-docs/libblkid-Search-and-iterate.html#blkid-get-tag-value
        // (Use the existing Command::new("blkid") code for functional testing)

        // Allow Linux a chance to read the name (eg. for post-ISO9660 stuff)
        if let Ok(label) = Command::new("blkid")
                        .args(&["-s", "LABEL", "-o", "value"]).arg(&self.device).output()
                        .map(|o| String::from_utf8_lossy(o.stdout.as_slice()).trim().to_owned()) {
                        // XXX: Handle some types of blkid failure?
            if !label.is_empty() {
                return Ok(label)  // TODO: Is there a more idiomatic early return for Ok()?
            }
        }

        // Fall back to reading the raw ISO9660 header
        // TODO: Move this stuff into an IsoMediaProvider
        let mut dev = File::open(&self.device).chain_err(
            || format!("Could not open for reading: {}", self.device.to_string_lossy()))?;

        // Safety check for non-ISO9660 filesystems
        // http://www.cnwrecovery.co.uk/html/iso9660_disks.html
        #[cfg_attr(feature="cargo-clippy", allow(use_debug))]
        let cd_magic = read_exact_at!(dev, 2, SeekFrom::Start(32769));
        if &cd_magic != b"CD" {
            bail!("Unrecognized file format");
        }

        // http://www.commandlinefu.com/commands/view/12178
        // TODO: Find the spec to see if the split is really needed
        //       (My test discs were space-padded)
        #[cfg_attr(feature="cargo-clippy", allow(use_debug))]
        Ok(String::from_utf8_lossy(&read_exact_at!(dev, 32, SeekFrom::Start(32808)))
                  .split('\0').next().unwrap_or("").trim().to_owned())
    }

    fn wait_for_ready(&self, timeout: &Duration) -> Result<()> {
        let start_time = Instant::now();
        loop {
            // Poll for a disc and return early on success
            // (According to https://lwn.net/Articles/462178/, this is probably
            //  something we can't readily and reliably block on)
            if File::open(&self.device).is_ok() {
                return Ok(());
            }
            if start_time.elapsed() >= *timeout { break; }

            sleep(Duration::new(1, 0))
        }
        bail!("Timed out")
    }
}

impl<'a> NotificationProvider for LinuxPlatformProvider<'a> {
    fn play_sound<P: AsRef<Path> + ?Sized>(&mut self, path: &P) -> Result<()> {
        subprocess_call!("play", "-q", path.as_ref())
                .chain_err(|| format!("Could not play {}", path.as_ref().to_string_lossy()))
    }

    fn read_line(&self, prompt: &str) -> Result<String> {
        let mut rl = Editor::<()>::new();
        rl.readline(prompt).chain_err(
            || format!("Failed to request information from user with: {}", prompt))
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt; // TODO: Find a better way to produce invalid UTF-8
    use std::path::Path;
    use std::time::{Duration, Instant};
    use super::{abspath, LinuxPlatformProvider,
                MediaProvider, NotificationProvider, RawMediaProvider};

    /// TODO: Tests for macros

    /// Helper to deduplicate getting a platform provider pointed at the test fixture
    fn get_iso_provider<'a>() -> LinuxPlatformProvider<'a> {
        let path = Path::new("fixture.iso");
        assert!(path.exists(), "Test fixture not found: {}",
                abspath(path).expect("Tests should have permission to read $PWD").display());
        return LinuxPlatformProvider::new(Cow::Borrowed(path.as_os_str()));
    }

    #[test]
    fn abspath_leaves_absolute_paths_unchanged() {
        for path_str in &["/", "/etc", "/nonexistant"] {
            let path = Path::new(path_str);
            assert_eq!(abspath(path).expect("abspath must never fail with null-free input"), path)
        }
    }
    // TODO: Test abspath with relative paths

    #[test]
    fn eject_reports_failure_properly() {
        let mut p_bad = LinuxPlatformProvider::new(Cow::Borrowed(OsStr::new("/etc/shadow")));
        assert!(p_bad.eject().is_err());
    }
    // TODO: Find a good way to test the success case for `eject`

    #[test]
    fn load_reports_failure_properly() {
        let mut p_bad = LinuxPlatformProvider::new(Cow::Borrowed(OsStr::new("/etc/shadow")));
        assert!(p_bad.load().is_err());
    }
    // TODO: Find a good way to test the success case for `load`

    #[test]
    fn play_sound_reports_failure_properly() {
        let mut p_good = get_iso_provider();
        let mut p_bad = LinuxPlatformProvider::new(Cow::Borrowed(OsStr::new("/etc/shadow")));
        assert!(p_bad.play_sound("/dev/null").is_err());
        assert!(p_good.play_sound("/dev/null").is_err());
    }
    // TODO: Find a good way to test the success case for `play_sound`

    // TODO: Find a way to test `read_line`

    #[test]
    fn unmount_reports_failure_properly() {
        let mut p_bad = LinuxPlatformProvider::new(Cow::Borrowed(OsStr::new("/etc/shadow")));
        assert!(p_bad.unmount().is_err());
    }
    // TODO: Find a good way to test the success case for `unmount`

    // -- Tests for LinuxPlatformProvider.device_path()

    #[test]
    fn device_path_equals_input_path() {
        for path_str in &["/", "/etc", "/etc/passwd", "/etc/shadow", "/nonexist"] {
            device_path_equals_input_path_inner(OsStr::new(&path_str));
        }
    }

    #[test]
    fn device_path_doesnt_modify_invalid_utf8() {
        device_path_equals_input_path_inner(OsStr::from_bytes(b"/test\xff"));
    }

    fn device_path_equals_input_path_inner(path_str: &OsStr) {
            let p = LinuxPlatformProvider::new(Cow::Borrowed(path_str));
            assert_eq!(p.device_path(), path_str);
    }

    // -- Tests for LinuxPlatformProvider.volume_label()

    fn test_label_failure(path_str: &str) {
        let p_bad = LinuxPlatformProvider::new(Cow::Borrowed(OsStr::new(path_str)));
        assert!(p_bad.volume_label().is_err(), "Expected Error for {:?}", path_str);
    }

    #[test]
    fn volume_label_basic_function() {
        assert_eq!(get_iso_provider().volume_label().expect("fixture.iso has label"), "CDROM");
    }

    // TODO: Familiarize myself with error-chain enough to return ErrorKinds and test them here
    #[test]
    fn volume_label_bad_format() {
        test_label_failure("/dev/null");
        test_label_failure("/etc/passwd");  // "can't seek that far" code branch
        test_label_failure("/bin/bash");    // "bad magic number" code branch
    }
    #[test]
    fn volume_label_not_a_file() { test_label_failure("/"); }
    #[test]
    fn volume_label_permission_denied() { test_label_failure("/etc/shadow"); }
    #[test]
    fn volume_label_nonexistant() { test_label_failure("/nonexist_path"); }

    // -- Tests for LinuxPlatformProvider.wait_for_ready()

    #[test]
    /// Test that it actually calls `sleep`
    fn wait_for_ready_actually_waits() {
        let p_bad = LinuxPlatformProvider::new(Cow::Borrowed(OsStr::new("/etc/shadow")));
        let timeout = Duration::new(2, 0);  // Allow at least one sleep() call

        let start = Instant::now();
        assert!(p_bad.wait_for_ready(&timeout).is_err());
        assert!(start.elapsed() > timeout);
    }

    #[test]
    /// Guard against naively using `while start_time.elapsed() < *timeout`
    fn wait_for_ready_always_tries_at_least_once() {
        let p = get_iso_provider();
        assert!(p.wait_for_ready(&Duration::new(0, 0)).is_ok())
    }

    #[test]
    /// Test basic function
    fn wait_for_ready_returns_success_with_good_input() {
        let p = get_iso_provider();
        assert!(p.wait_for_ready(&Duration::new(2, 0)).is_ok())
    }

    // CAUTION: Missing a test to guard against some kind of stale cache causing the timeout
    //          duration to have no effect on the result.
}
// vim: set sw=4 sts=4 :
