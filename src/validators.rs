use std::error::Error;
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

/// Characters invalid under NTFS in the Win32 namespace
const INVALID_FILENAME_CHARS: &'static str = "/\\:*?\"<>|\0";

/// The effects of unsafety cannot be isolated with more granularity than a module scope
/// because of how public/private access control works, so isolate the access() libc
/// call in its own module to make the intent more clear when refactoring this file.
mod access {
    extern crate libc;
    use self::libc::{access, c_int, W_OK};

    fn wrapped_access(abs_path: &str, mode: c_int) -> bool {
        // Debug-time check that we're using the API properly
        assert!(::std::path::Path::new(&abs_path).is_absolute());


        let ptr = abs_path.as_ptr() as *const i8;

        // I'm willing to risk using unsafe because access() shouldn't mutate anything
        unsafe {
            // Test if we **should** be able to write to the given path
            if access(ptr, mode) == 0 { return true; }
        }
        return false;
    }

    /// Use a name which helps to drive home the security hazard in access() abuse
    /// and hide the mode flag behind an abstraction so the user can't mess up unsafe{}
    /// (eg. On my system, "/" erroneously returns success)
    pub fn probably_writable(path: &str) -> bool { wrapped_access(path, W_OK) }
}

/// Test that the given path **should** be writable
/// TODO: Unit test **HEAVILY** (Has unsafe block. Here be dragons!)
pub fn dir_writable(value: String) -> Result<(), String> {
    let path = Path::new(&value);

    // Test that the path is a directory
    // (Check this before, not after, as an extra safety guard on the unsafe block)
    if !path.is_dir() {
        return Err(format!("Not a directory: {}", value));
    }

    // TODO: Think about how to code this more elegantly (try! perhaps?)
    if let Ok(abs_pathbuf) = path.canonicalize() {
        if let Some(abs_path) = abs_pathbuf.to_str() {
            if self::access::probably_writable(abs_path) { return Ok(()); }
        }
    }

    Err(format!("Would be unable to write to destination directory: {}", value))
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

/// Return true if the given character is in `INVALID_FILENAME_CHARS`
fn is_bad_for_fname(c: &char) -> bool {
    INVALID_FILENAME_CHARS.chars().any(|x| x == *c)
}

/// Test that the given path can be opened for reading and adjust failure messages
pub fn path_readable(value: String) -> Result<(), String> {
    File::open(&value).map(|_| ()).map_err(|e|
        format!("{}: {}", &value, match e.kind() {
            ErrorKind::NotFound => "path does not exist",
            // TODO: Return Ok(()) for ErrorKind::Other (we can wait/retry later)
            ErrorKind::Other => "unknown OS error (medium not ready?)",
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
