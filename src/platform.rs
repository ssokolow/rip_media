//! Abstraction around the underlying OS functionality

use std::borrow::Cow;
use std::error::Error;
use std::ffi::{OsString, OsStr};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Default timeout duration (in seconds)
pub const DEFAULT_TIMEOUT: u64 = 10;

/// Default to be used with the `close_tray` argument to `MediaProvider::wait_for_ready`
pub const DEFAULT_CLOSE_TRAY: bool = true;

/// Shorthand for calling subprocesses purely for side-effects
#[macro_export]
macro_rules! subprocess_call {
    ( $cmd:expr, $( $arg:expr ), * ) => {
        Command::new($cmd)
                $(.arg($arg))*
                .status()
                .map(|_| ()).map_err(|e| e.description().to_owned())
                // TODO: Set up error-chain instead
    }
}

/// Shorthand for reading byte substrings from `Seek`-ables
macro_rules! read_exact_at {
    ( $file:expr, $bytes:expr, $offset:expr ) => {
        {
            let mut buf = [0; $bytes];
            // TODO: error-chain
            $file.seek($offset).map_err(|e| e.description().to_owned())?;
            $file.read_exact(&mut buf).map_err(|e| e.description().to_owned())?;
            buf
        }
    }
}

/// Interface for manipulating media devices such as DVD drives
/// TODO: Custom error type
pub trait MediaProvider {
    /// Eject the media if the hardware supports it
    fn eject(&mut self) -> Result<(), String>;

    /// Unmount the media if mounted
    fn unmount(&mut self) -> Result<(), String>;

    /// Retrieve the volume label, if one is set
    fn volume_label(&self) -> Result<String, String>;

    /// Wait up to `timeout` seconds for the disc to be ready
    ///
    /// If `close_tray` is `true` and the device is capable, load the media.
    fn wait_for_ready(&self, timeout: &Duration, close_tray: bool) -> Result<(), String>;
}

/// Interface for platform providers which support exposing raw device paths
pub trait RawMediaProvider {
    /// Return an `OsString` which can be used by APIs or subprocesses to
    /// reference the device
    fn device_path(&self) -> OsString;
}

/// High-level interface for notifying the user via various system APIs
pub trait NotificationProvider {
    /// Play the given audio file, if supported
    fn play_sound<P: AsRef<Path> + ?Sized>(&self, path: &P) -> Result<(), String>;
}

/// `MediaProvider` implementation which operates on (possibly GUI-less) Linux systems
pub struct LinuxPlatformProvider<'a> {
    /// Device/file to operate on
    device: Cow<'a, OsStr>,
}

impl<'a> RawMediaProvider for LinuxPlatformProvider<'a> {
    // TODO: Actually think about this API and refactor.
    fn device_path(&self) -> OsString { self.device.clone().into_owned() }
}

impl<'a> MediaProvider for LinuxPlatformProvider<'a> {
    fn eject(&mut self) -> Result<(), String> {
        subprocess_call!("eject", &self.device)
    }

    fn unmount(&mut self) -> Result<(), String> {
        subprocess_call!("umount", &self.device)
    }

    fn volume_label(&self) -> Result<String, String> {
        // Allow Linux a chance to read the name (eg. for post-ISO9660 stuff)
        if let Ok(label) = Command::new("blkid")
                        .args(&["-s", "LABEL", "-o", "value"]).arg(&self.device).output()
                        .map(|o| String::from_utf8_lossy(o.stdout.as_slice()).trim().to_owned()) {
                        // XXX: Handle some types of blkid failure?
            return Ok(label)  // TODO: Is there a more idiomatic way to do this?
        }

        // Fall back to reading the raw ISO9660 header
        let mut dev = File::open(&self.device).map_err(|e| e.description().to_owned())?;

        // Safety check for non-ISO9660 filesystems
        // http://www.cnwrecovery.co.uk/html/iso9660_disks.html
        if &read_exact_at!(dev, 2, SeekFrom::Start(32769)) != b"CD" {
            return Ok("".to_string())
        }

        // http://www.commandlinefu.com/commands/view/12178
        // TODO: Find the spec to see if the split is really needed
        //       (My test discs were space-padded)
        Ok(String::from_utf8_lossy(&read_exact_at!(dev, 32, SeekFrom::Start(32808)))
                  .split('\0').next().unwrap_or("").trim().to_owned())
    }

    fn wait_for_ready(&self, timeout: &Duration, close_tray: bool) -> Result<(), String> {
        if close_tray {
            // TODO: What *should* I do on failure here?
            subprocess_call!("eject", "-t", &self.device);
        }

        let start_time = Instant::now();
        while start_time.elapsed() < *timeout {
            // Poll for a disc and return early on success
            // (According to https://lwn.net/Articles/462178/, this is probably
            //  something we can't readily and reliably block on)
            if File::open(&self.device).is_ok() {
                return Ok(())
            }
            sleep(Duration::new(1, 0))
        }
        Err("Timed out".to_string())  // TODO: Custom error type
    }
}

impl<'a> NotificationProvider for LinuxPlatformProvider<'a> {
    fn play_sound<P: AsRef<Path> + ?Sized>(&self, path: &P) -> Result<(), String> {
        subprocess_call!("play", "-q", path.as_ref())
    }
}
