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

    #[cfg(test)]
    mod tests {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt; // TODO: Find a better way to produce invalid UTF-8
        use super::probably_writable;

        #[test]
        fn probably_writable_basic_functionality() {
            assert!(probably_writable(OsStr::new("/tmp")));                    // OK Folder
            assert!(probably_writable(OsStr::new("/dev/null")));               // OK File
            assert!(!probably_writable(OsStr::new("/etc/shadow")));            // Denied File
            assert!(!probably_writable(OsStr::new("/etc/ssl/private")));       // Denied Folder
            assert!(!probably_writable(OsStr::new("/nonexistant_test_path"))); // Missing Path
            assert!(!probably_writable(OsStr::new("/tmp\0with\0null")));       // Bad CString
            assert!(!probably_writable(OsStr::from_bytes(b"/not\xffutf8")));   // Bad UTF-8
            assert!(!probably_writable(OsStr::new("/")));                      // Root
            // TODO: Relative path
            // TODO: Non-UTF8 path that actually does exist and is writable
        }
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
    } else if value.is_empty() {
        Err("Name is empty".into())
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
///       so my validator can be generic (A closure, maybe?)
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
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt; // TODO: Find a better way to produce invalid UTF-8
    use super::{dir_writable, filename_valid, path_readable, set_size};

    #[test]
    fn dir_writable_basic_functionality() {
        assert!(dir_writable(OsStr::new("/tmp")).is_ok());                    // OK Folder
        assert!(dir_writable(OsStr::new("/dev/null")).is_err());              // OK File
        assert!(dir_writable(OsStr::new("/etc/shadow")).is_err());            // Denied File
        assert!(dir_writable(OsStr::new("/etc/ssl/private")).is_err());       // Denied Folder
        assert!(dir_writable(OsStr::new("/nonexistant_test_path")).is_err()); // Missing Path
        assert!(dir_writable(OsStr::new("/tmp\0with\0null")).is_err());       // Invalid CString
        assert!(dir_writable(OsStr::from_bytes(b"/not\xffutf8")).is_err());   // Invalid UTF-8
        assert!(dir_writable(OsStr::new("/")).is_err());                      // Root
        // TODO: is_dir but fails to canonicalize()
        // TODO: Not-already-canonicalized paths
        // TODO: Non-UTF8 path that actually does exist and is writable
    }

    #[test]
    fn filename_valid_accepts_valid_for_posix() {
        assert!(filename_valid(OsStr::new("test")).is_ok());         // Basic, uncontroversial OK
        assert!(filename_valid(OsStr::new("te st")).is_ok());        // Filenames may have spaces
        assert!(filename_valid(OsStr::new("te\nst")).is_ok());       // Filenames may contain \n
        assert!(filename_valid(OsStr::new(".test")).is_ok());        // Filenames may start with .
        assert!(filename_valid(OsStr::new("test.")).is_ok());        // Filenames may end with .
        assert!(filename_valid(OsStr::from_bytes(b"\xff")).is_ok()); // POSIX allows invalid UTF-8
    }

    #[test]
    fn filename_valid_refuses_invalid_for_fat32() {
        assert!(filename_valid(OsStr::new("te\\st")).is_err());   // FAT32 uses \ as path separator
        assert!(filename_valid(OsStr::new("t:est")).is_err());    // FAT32 uses : for drive letters
        assert!(filename_valid(OsStr::new("\"test\"")).is_err()); // Can't escape " in COMMAND.COM
        assert!(filename_valid(OsStr::new("<test")).is_err());    // Can't escape < in COMMAND.COM
        assert!(filename_valid(OsStr::new("test>")).is_err());    // Can't escape > in COMMAND.COM
        assert!(filename_valid(OsStr::new("test|")).is_err());    // Can't escape | in COMMAND.COM
        assert!(filename_valid(OsStr::new("test*")).is_err());    // Can't escape * in COMMAND.COM
        assert!(filename_valid(OsStr::new("test?")).is_err());    // Can't escape ? in COMMAND.COM
        assert!(filename_valid(OsStr::new("?est")).is_err());     // ? at start signifies deletion
    }

    #[test]
    fn filename_valid_refuses_invalid_for_posix() {
        assert!(filename_valid(OsStr::new("")).is_err());       // Filenames may not be empty
        assert!(filename_valid(OsStr::new("te/st")).is_err());  // POSIX uses / as path separator
        assert!(filename_valid(OsStr::new("te\0st")).is_err()); // \0 is POSIX's string terminator
    }

    #[test]
    fn path_readable_basic_functionality() {
        assert!(path_readable(OsStr::new("/")).is_ok());                       // OK Folder
        assert!(path_readable(OsStr::new("/etc/passwd")).is_ok());             // OK File
        assert!(path_readable(OsStr::new("/etc/shadow")).is_err());            // Denied File
        assert!(path_readable(OsStr::new("/etc/ssl/private")).is_err());       // Denied Folder
        assert!(path_readable(OsStr::new("/nonexistant_test_path")).is_err()); // Missing Path
        assert!(path_readable(OsStr::new("/null\0containing")).is_err());      // Invalid CString
        assert!(path_readable(OsStr::from_bytes(b"/not\xffutf8")).is_err());   // Invalid UTF-8
        // TODO: Not-already-canonicalized paths
        // TODO: Non-UTF8 path that actually IS valid
        // TODO: ErrorKind::Other
    }

    #[test]
    fn set_size_requires_positive_base_10_numbers() {
        assert!(set_size("".into()).is_err());
        assert!(set_size("one".into()).is_err());
        assert!(set_size("a".into()).is_err()); // not base 11 or above
        assert!(set_size("0".into()).is_err());
        assert!(set_size("-1".into()).is_err());
    }

    #[test]
    fn set_size_requires_integers() {
        assert!(set_size("-1.5".into()).is_err());
        assert!(set_size("-0.5".into()).is_err());
        assert!(set_size("0.5".into()).is_err());
        assert!(set_size("1.5".into()).is_err());
    }

    #[test]
    fn set_size_handles_out_of_range_sanely() {
        assert!(set_size("5000000000".into()).is_err());
    }

    #[test]
    fn set_size_basic_functionality() {
        assert!(set_size("1".into()).is_ok());
        assert!(set_size("9".into()).is_ok());    // not base 9 or below
        assert!(set_size("5000".into()).is_ok()); // accept reasonably large numbers
    }
}

// vim: set sw=4 sts=4 :
