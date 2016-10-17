extern crate libc;

use std::error::Error;
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

use self::libc::{access,W_OK};

// TODO: Make this different if built on Windows
const INVALID_FILENAME_CHARS: &'static str = "/\0";

/// Return true if the given character is in `INVALID_FILENAME_CHARS`
fn is_bad_for_fname(c: &char) -> bool {
    INVALID_FILENAME_CHARS.chars().any(|x| x == *c)
}

/// Test that the given string doesn't contain any `INVALID_FILENAME_CHARS`
/// Adapted from http://stackoverflow.com/a/30791678
///
/// TODO: Rethink this to properly handle non-POSIX target filesystems under POSIX OSes
pub fn filename_valid(value: String) -> Result<(), String> {
    if value.chars().all(|c| !is_bad_for_fname(&c)) {
        return Ok(());
    }
    Err(format!("Name contains invalid characters: {}", value))
}

/// Test that the given path **should** be writable
/// TODO: Unit test **HEAVILY** (Has unsafe block. Here be dragons!)
pub fn dir_writable(value: String) -> Result<(), String> {
    // Test that the path is a directory
    if !Path::new(&value).is_dir() {
        return Err(format!("Not a directory: {}", value));
    }

    // I'm willing to risk this because access() shouldn't mutate anything
    let ptr = value.as_ptr() as *const i8;
    unsafe {
        // Test if we **should** be able to write to the given path
        if access(ptr, W_OK) == 0 { return Ok(()); }
    }

    Err(format!("Would be unable to write to destination directory: {}", value))
}

/// Test that the given path can be opened for reading and adjust failure messages
pub fn path_readable(value: String) -> Result<(), String> {
    File::open(&value).map(|_| ()).map_err(|e|
        format!("{}: {}", &value, match e.kind() {
            ErrorKind::NotFound => "path does not exist",
            _ => e.description()
        })
    )
}

/// TODO: Find a way to make *clap* mention which argument failed
///       validation so my validator can be generic
pub fn set_size(value: String) -> Result<(), String> {
    // I can't imagine needing more than u8, but I see no harm in being flexible
    if let Ok(num) = value.parse::<u32>() {
        if num >= 1_u32 {
            return Ok(());
        } else {
            return Err(format!("Set size must be 1 or greater (not \"{}\")", value));
        }
    }
    Err(format!("Set size must be an integer (whole number), not \"{}\"", value))
}
