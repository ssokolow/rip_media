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
    /// TODO: Make this wrapper portable
    ///       https://doc.rust-lang.org/book/conditional-compilation.html
    /// TODO: Consider making `wrapped_access` typesafe using the `bitflags` crate `clap` pulled in
    extern crate libc;
    use self::libc::{access, c_char, c_int, W_OK};

    // TODO: Figure out how to make this accept &str/Path/PathBuf to reduce conversion
    // (Perhaps AsRef<OsStr> as in Path::new, with http://stackoverflow.com/q/38948669/435253)
    /// Lower-level safety wrapper shared by all probably_* functions I define
    #[cfg_attr(feature="cargo-clippy", allow(needless_return))]
    fn wrapped_access(abs_path: &str, mode: c_int) -> bool {
        // Debug-time check that we're using the API properly
        // (Debug-only because relying on it in a release build grants a false sense
        // of security and, besides, access() is only really safe to use as a way to
        // abort early for convenience on errors that would still be safe anyway.)
        assert!(::std::path::Path::new(&abs_path).is_absolute());


        let ptr = abs_path.as_ptr() as *const c_char;

        // I'm only willing to trust my ability to use unsafe correctly
        // because access() shouldn't mutate anything
        unsafe { access(ptr, mode) == 0 }
    }

    /// API suitable for a lightweight "fail early" check for whether a target
    /// directory is writable without worry that a fancy filesystem may be configured
    /// to allow write but deny deletion for the resulting test file.
    /// (It's been seen in the wild)
    ///
    /// Uses a name which helps to drive home the security hazard in access() abuse
    /// and hide the mode flag behind an abstraction so the user can't mess up unsafe{}
    /// (eg. On my system, "/" erroneously returns success)
    pub fn probably_writable(path: &str) -> bool { wrapped_access(path, W_OK) }
}

// TODO: Can (and should) I rewrite these to take &str instead of String?

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
        // TODO: Return a custom error type so we're not risking stringly-typed error matching
        //       https://brson.github.io/2016/11/30/starting-with-error-chain
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
}
