use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;

/// Characters invalid under NTFS in the Win32 namespace
/// TODO: Probably best to impose FAT32 limits instead in case of flash drives
const INVALID_FILENAME_CHARS: &'static str = "/\\:*?\"<>|\0";

/// The effects of unsafety cannot be isolated with more granularity than a
/// module scope because of how public/private access control works, so isolate
/// the access() libc call in its own module to make the intent more clear when
/// refactoring this file.
mod access {
    /// TODO: Make this wrapper portable
    ///       https://doc.rust-lang.org/book/conditional-compilation.html
    /// TODO: Consider making `wrapped_access` typesafe using the `bitflags`
    ///       crate `clap` pulled in
    extern crate libc;
    use self::libc::{access, c_int, W_OK};
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    /// Lower-level safety wrapper shared by all probably_* functions I define
    /// TODO: Unit test **HEAVILY** (Has unsafe block. Here be dragons!)
    fn wrapped_access(abs_path: &Path, mode: c_int) -> bool {
        // Debug-time check that we're using the API properly
        // (Debug-only because relying on it in a release build grants a false
        // sense of security and, besides, access() is only really safe to use
        // as a way to abort early for convenience on errors that would still
        // be safe anyway.)
        assert!(abs_path.is_absolute());

        // Make a null-terminated copy of the path for libc
        match CString::new(abs_path.as_os_str().as_bytes()) {
            // If we succeed, call access(2), convert the result into bool, and return it
            Ok(cstr) => unsafe { access(cstr.as_ptr(), mode) == 0 },
            // If we fail, return false because it can't be an access()ible path
            Err(_) => false,
        }
    }

    /// API suitable for a lightweight "fail early" check for whether a target
    /// directory is writable without worry that a fancy filesystem may be
    /// configured to allow write but deny deletion for the resulting test file.
    /// (It's been seen in the wild)
    ///
    /// Uses a name which helps to drive home the security hazard in access()
    /// abuse and hide the mode flag behind an abstraction so the user can't
    /// mess up unsafe{} (eg. On my system, "/" erroneously returns success)
    pub fn probably_writable<P: AsRef<Path> + ?Sized>(path: &P) -> bool {
        wrapped_access(path.as_ref(), W_OK)
    }
}

/// Test that the given path **should** be writable
pub fn dir_writable(value: &OsStr) -> Result<(), OsString> {
    let path = Path::new(&value);

    // Test that the path is a directory
    // (Check before, not after, as an extra safety guard on the unsafe block)
    if !path.is_dir() {
        #[allow(use_debug)]
        return Err(format!("Not a directory: {:?}", value).into());
    }

    // TODO: Think about how to code this more elegantly (try! perhaps?)
    if let Ok(abs_pathbuf) = path.canonicalize() {
        if let Some(abs_path) = abs_pathbuf.to_str() {
            if self::access::probably_writable(abs_path) { return Ok(()); }
        }
    }

    #[allow(use_debug)]
    Err(format!("Would be unable to write to destination directory: {:?}", value).into())
}

/// Test that the given string doesn't contain any `INVALID_FILENAME_CHARS`
/// Adapted from http://stackoverflow.com/a/30791678
///
/// TODO: Is there a way to ask the filesystem itself whether a name is OK?
pub fn filename_valid(value: &OsStr) -> Result<(), OsString> {
    // TODO: Switch to using to_bytes() once it's stabilized
    if value.to_string_lossy().chars().any(|c| is_bad_for_fname(&c)) {
        #[allow(use_debug)]
        Err(format!("Name contains invalid characters: {:?}", value).into())
    } else {
        Ok(())
    }
}

/// Return true if the given character is in `INVALID_FILENAME_CHARS`
fn is_bad_for_fname(c: &char) -> bool {
    INVALID_FILENAME_CHARS.chars().any(|x| x == *c)
}

/// Test that the given path can be opened for reading and adjust failure messages
pub fn path_readable(value: &OsStr) -> Result<(), OsString> {
    File::open(&value).map(|_| ()).map_err(|e|
        // TODO: Custom error type to avoid risking stringly-typed matching
        //       https://brson.github.io/2016/11/30/starting-with-error-chain
        format!("{:?}: {}", value, match e.kind() {
            ErrorKind::NotFound => "path does not exist",
            // TODO: Return Ok(()) for ErrorKind::Other (we can wait/retry later)
            ErrorKind::Other => "unknown OS error (medium not ready?)",
            _ => e.description()
        }).into()
    )
}

/// TODO: Find a way to make *clap* mention which argument failed validation
///       so my validator can be generic
pub fn set_size(value: String) -> Result<(), String> {
    // I can't imagine needing more than u8... no harm in being flexible here
    if let Ok(num) = value.parse::<u32>() {
        if num >= 1_u32 {
            return Ok(());
        } else {
            return Err(format!("Set size must be 1 or greater (not \"{}\")", value));
        }
    }
    Err(format!("Set size must be an integer (whole number), not \"{}\"", value))
}

#[cfg(test)]
mod tests {
    use super::path_readable;

    #[test]
    /// Can override DEFAULT_INPATH when specifying -i before the subcommand
    fn path_readable_basic_functionality() {
        assert!(path_readable("/".to_string()).is_ok());
        assert!(path_readable("/etc/passwd".to_string()).is_ok());
        assert!(path_readable("/etc/shadow".to_string()).is_err());
        assert!(path_readable("/nonexistant_test_path".to_string()).is_err());
    }

    // TODO: More unit tests
}

// vim: set sw=4 sts=4 :
