/*! Validator functions suitable for use with `Clap` and `StructOpt` */
// Copyright 2017-2019, Stephan Sokolow

use std::ffi::OsString;
use std::fs::File;
use std::path::{Component, Path};

/// Special filenames which cannot be used for real files under Win32
///
/// (Unless your app uses the `\\?\` path prefix to bypass legacy Win32 API compatibility
/// limitations)
///
/// Source: [Boost Path Name Portability Guide
/// ](https://www.boost.org/doc/libs/1_36_0/libs/filesystem/doc/portability_guide.htm)
#[allow(dead_code)] // TEMPLATE:REMOVE
const RESERVED_DOS_FILENAMES: &[&str] = &[
    "AUX", "CON", "NUL", "PRN", // Comments for rustfmt
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", // Serial Ports
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", // Parallel Ports
    "CLOCK$",
]; // https://www.boost.org/doc/libs/1_36_0/libs/filesystem/doc/portability_guide.htm
   // TODO: Add the rest of the disallowed names from
   // https://en.wikipedia.org/wiki/Filename#Comparison_of_filename_limitations

/// Module to contain the unsafety of an `unsafe` call to `access()`
mod access {
    /// TODO: Make this wrapper portable
    ///       <https://doc.rust-lang.org/book/conditional-compilation.html>
    /// TODO: Consider making `wrapped_access` typesafe using the `bitflags`
    ///       crate `clap` pulled in
    use libc::{access, c_int, W_OK};
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
        use super::probably_writable;
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt; // TODO: Find a better way to produce invalid UTF-8

        #[test]
        fn probably_writable_basic_functionality() {
            assert!(probably_writable(OsStr::new("/tmp"))); // OK Folder
            assert!(probably_writable(OsStr::new("/dev/null"))); // OK File
            assert!(!probably_writable(OsStr::new("/etc/shadow"))); // Denied File
            assert!(!probably_writable(OsStr::new("/etc/ssl/private"))); // Denied Folder
            assert!(!probably_writable(OsStr::new("/nonexistant_test_path"))); // Missing Path
            assert!(!probably_writable(OsStr::new("/tmp\0with\0null"))); // Bad CString
            assert!(!probably_writable(OsStr::from_bytes(b"/not\xffutf8"))); // Bad UTF-8
            assert!(!probably_writable(OsStr::new("/"))); // Root
                                                          // TODO: Relative path
                                                          // TODO: Non-UTF8 path that actually does exist and is writable
        }
    }
}

/// Test that the given path **should** be writable
#[allow(dead_code)] // TEMPLATE:REMOVE
pub fn path_output_dir<P: AsRef<Path> + ?Sized>(value: &P) -> Result<(), OsString> {
    let path = Path::new(value.as_ref());

    // Test that the path is a directory
    // (Check before, not after, as an extra safety guard on the unsafe block)
    if !path.is_dir() {
        return Err(format!("Not a directory: {}", path.display()).into());
    }

    // TODO: Think about how to code this more elegantly (try! perhaps?)
    if let Ok(abs_pathbuf) = path.canonicalize() {
        if let Some(abs_path) = abs_pathbuf.to_str() {
            if self::access::probably_writable(abs_path) {
                return Ok(());
            }
        }
    }

    Err(format!("Would be unable to write to destination directory: {}", path.display()).into())
}

/// The given path can be opened for reading
///
/// ## Use For:
///  * Input file paths
///
/// ## Relevant Conventions:
///  * Commands which read from `stdin` by default should use `-f` to specify the input path.
///    [\[1\]](http://www.catb.org/esr/writings/taoup/html/ch10s05.html)
///  * Commands which read from files by default should use positional arguments to specify input
///    paths.
///  * Allow an arbitrary number of input paths if feasible.
///  * Interpret a value of `-` to mean "read from `stdin`" if feasible.
///    [\[2\]](http://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap12.html)
///
/// **Note:** The following command-lines, which interleave files and `stdin`, are a good test of
/// how the above conventions should interact:
///
///     data_source | my_utility_a header.dat - footer.dat > output.dat
///     data_source | my_utility_b -f header.dat -f - -f footer.dat > output.dat
///
/// ## Cautions:
///  * This will momentarily open the given path for reading to verify that it is readable.
///    However, relying on this to remain true will introduce a race condition. This validator is
///    intended only to allow your program to exit as quickly as possible in the case of obviously
///    bad input.
///  * As a more reliable validity check, you are advised to open a handle to the file in question
///    as early in your program's operation as possible, use it for all your interactions with the
///    file, and keep it open until you are finished. This will both verify its validity and
///    minimize the window in which another process could render the path invalid.
///
/// **TODO:** Determine why `File::open` has no problem opening directory paths and decide how to
/// adjust this.
#[allow(dead_code)] // TEMPLATE:REMOVE
pub fn path_readable<P: AsRef<Path> + ?Sized>(value: &P) -> std::result::Result<(), OsString> {
    let path = value.as_ref();
    File::open(path).map(|_| ()).map_err(|e| format!("{}: {}", path.display(), e).into())
}

/// Test that the given path **should** be writable
pub fn dir_writable<P: AsRef<Path> + ?Sized>(value: &P) -> std::result::Result<(), OsString> {
    let path = value.as_ref();

    // Test that the path is a directory
    // (Check before, not after, as an extra safety guard on the unsafe block)
    if !path.is_dir() {
        return Err(format!("Not a directory: {}", path.display()).into());
    }

    // TODO: Think about how to code this more elegantly (try! perhaps?)
    if let Ok(abs_pathbuf) = path.canonicalize() {
        if let Some(abs_path) = abs_pathbuf.to_str() {
            if self::access::probably_writable(abs_path) {
                return Ok(());
            }
        }
    }

    Err(format!("Would be unable to write to destination directory: {}", path.display()).into())
}

/// The given path is valid on all major filesystems and OSes
///
/// ## Use For:
///  * Output file or directory paths
///
/// ## Relevant Conventions:
///  * Use `-o` to specify the output path.
///    [\[1\]](http://www.catb.org/esr/writings/taoup/html/ch10s05.html)
///    [\[2\]](http://tldp.org/LDP/abs/html/standard-options.html)
///  * Interpret a value of `-` to mean "Write output to stdout".
///    [\[3\]](http://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap12.html)
///  * Because `-o` does not inherently indicate whether it expects a file or a directory, consider
///    also providing a GNU-style long version with a name like `--outfile` to allow scripts which
///    depend on your tool to be more self-documenting.
///
/// ## Cautions:
///  * To ensure files can be copied/moved without issue, this validator may impose stricter
///    restrictions on filenames than your filesystem. Do *not* use it for input paths.
///  * Other considerations, such as paths containing symbolic links with longer target names, may
///    still cause your system to reject paths which pass this check.
///  * As a more reliable validity check, you are advised to open a handle to the file in question
///    as early in your program's operation as possible and keep it open until you are finished.
///    This will both verify its validity and minimize the window in which another process could
///    render the path invalid.
///
/// ## Design Considerations: [\[4\]](https://en.wikipedia.org/wiki/Comparison_of_file_systems#Limits)
///  * Many popular Linux filesystems impose no total length limit.
///  * This function imposes a 32,760-character limit for compatibility with flash drives formatted
///    FAT32 or exFAT.
///  * Some POSIX API functions, such as `getcwd()` and `realpath()` rely on the `PATH_MAX`
///    constant, which typically specifies a length of 4096 bytes including terminal `NUL`, but
///    this is not enforced by the filesystem itself.
///    [\[4\]](https://insanecoding.blogspot.com/2007/11/pathmax-simply-isnt.html)
///
///    Programs which rely on libc for this functionality but do not attempt to canonicalize paths
///    will usually work if you change the working directory and use relative paths.
///  * The following lengths were considered too limiting to be enforced by this function:
///    * The UDF filesystem used on DVDs imposes a 1023-byte length limit on paths.
///    * When not using the `\\?\` prefix to disable legacy compatibility, Windows paths  are
///      limited to 260 characters, which was arrived at as `A:\MAX_FILENAME_LENGTH<NULL>`.
///      [\[5\]](https://stackoverflow.com/a/1880453/435253)
///    * ISO 9660 without Joliet or Rock Ridge extensions does not permit periods in directory
///      names, directory trees more than 8 levels deep, or filenames longer than 32 characters.
///      [\[6\]](https://www.boost.org/doc/libs/1_36_0/libs/filesystem/doc/portability_guide.htm)
///
///  **TODO:**
///   * Write another function for enforcing the limits imposed by targeting optical media.
#[allow(dead_code)] // TEMPLATE:REMOVE
pub fn path_valid_portable<P: AsRef<Path> + ?Sized>(value: &P) -> Result<(), OsString> {
    #![allow(clippy::match_same_arms, clippy::decimal_literal_representation)]
    let path = value.as_ref();

    if path.as_os_str().is_empty() {
        Err("Path is empty".into())
    } else if path.as_os_str().len() > 32760 {
        // Limit length to fit on VFAT/exFAT when using the `\\?\` prefix to disable legacy limits
        // Source: https://en.wikipedia.org/wiki/Comparison_of_file_systems
        Err(format!("Path is too long ({} chars): {:?}", path.as_os_str().len(), path).into())
    } else {
        for component in path.components() {
            if let Component::Normal(string) = component {
                filename_valid_portable(string)?;
            }
        }
        Ok(())
    }
}

/// The string is a valid file/folder name on all major filesystems and OSes
///
/// ## Use For:
///  * Output file or directory names within a parent directory specified through other means.
///
/// ## Relevant Conventions:
///  * Most of the time, you want to let users specify a full path via [`path_valid_portable`
///    ](fn.path_valid_portable.html)instead.
///
/// ## Cautions:
///  * To ensure files can be copied/moved without issue, this validator may impose stricter
///    restrictions on filenames than your filesystem. Do *not* use it for input filenames.
///  * This validator cannot guarantee that a given filename will be valid once other
///    considerations such as overall path length limits are taken into account.
///  * As a more reliable validity check, you are advised to open a handle to the file in question
///    as early in your program's operation as possible, use it for all your interactions with the
///    file, and keep it open until you are finished. This will both verify its validity and
///    minimize the window in which another process could render the path invalid.
///
/// ## Design Considerations: [\[3\]](https://en.wikipedia.org/wiki/Comparison_of_file_systems#Limits)
///  * In the interest of not inconveniencing users in the most common case, this validator imposes
///    a 255-character length limit.
///  * The eCryptFS home directory encryption offered by Ubuntu Linux imposes a 143-character
///    length limit when filename encryption is enabled.
///    [\[4\]](https://bugs.launchpad.net/ecryptfs/+bug/344878)
///  * the Joliet extensions for ISO 9660 are specified to support only 64-character filenames and
///    tested to support either 103 or 110 characters depending whether you ask the mkisofs
///    developers or Microsoft. [\[5\]](https://en.wikipedia.org/wiki/Joliet_(file_system))
///  * The [POSIX Portable Filename Character Set
///    ](http://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap03.html#tag_03_282)
///    is too restrictive to be baked into a general-purpose validator.
///
/// **TODO:** Consider converting this to a private function that just exists as a helper for the
/// path validator in favour of more specialized validators for filename patterns, prefixes, and/or
/// suffixes, to properly account for how "you can specify a name bu not a path" generally
/// comes about.
#[allow(dead_code)] // TEMPLATE:REMOVE
pub fn filename_valid_portable<P: AsRef<Path> + ?Sized>(value: &P) -> Result<(), OsString> {
    #![allow(clippy::match_same_arms, clippy::else_if_without_else)]
    let path = value.as_ref();

    // TODO: Should I refuse incorrect Unicode normalization since Finder doesn't like it or just
    //       advise users to run a normalization pass?
    // Source: https://news.ycombinator.com/item?id=16993687

    // Check that the length is within range
    let os_str = path.as_os_str();
    if os_str.len() > 255 {
        return Err(format!(
            "File/folder name is too long ({} chars): {:?}",
            path.as_os_str().len(),
            path
        )
        .into());
    } else if os_str.is_empty() {
        return Err("Path component is empty".into());
    }

    // Check for invalid characters
    let lossy_str = os_str.to_string_lossy();
    let last_char = lossy_str.chars().last().expect("getting last character");
    if [' ', '.'].iter().any(|&x| x == last_char) {
        // The Windows shell and UI don't support component names ending in periods or spaces
        // Source: https://docs.microsoft.com/en-us/windows/desktop/FileIO/naming-a-file
        return Err("Windows forbids path components ending with spaces/periods".into());
    } else if lossy_str.as_bytes().iter().any(|c| {
        matches!(
            c,
            // invalid on all APIs which don't use counted strings like inside the NT kernel
            b'\0' |
        // invalid under FAT*, VFAT, exFAT, and NTFS
        0x0
                ..=0x1f | 0x7f | b'"' | b'*' | b'<' | b'>' | b'?' | b'|' |
        // POSIX path separator (invalid on Unixy platforms like Linux and BSD)
        b'/' |
        // HFS/Carbon path separator (invalid in filenames on MacOS and Mac filesystems)
        // DOS/Win32 drive separator (invalid in filenames on Windows and Windows filesystems)
        b':' |
        // DOS/Windows path separator (invalid in filenames on Windows and Windows filesystems)
        b'\\' // let everything else through
        )
    }) {
        #[allow(clippy::use_debug)]
        return Err(format!("Path component contains invalid characters: {:?}", path).into());
    }

    // Reserved DOS filenames that still can't be used on modern Windows for compatibility
    if let Some(file_stem) = path.file_stem() {
        let stem = file_stem.to_string_lossy().to_uppercase();
        if RESERVED_DOS_FILENAMES.iter().any(|&x| x == stem) {
            return Err(format!("Filename is reserved on Windows: {:?}", file_stem).into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[cfg(not(windows))]
    use std::os::unix::ffi::OsStrExt;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;

    #[test]
    fn path_output_dir_basic_functionality() {
        assert!(path_output_dir(OsStr::new("/")).is_err()); // Root
        assert!(path_output_dir(OsStr::new("/tmp")).is_ok()); // OK Folder
        assert!(path_output_dir(OsStr::new("/dev/null")).is_err()); // OK File
        assert!(path_output_dir(OsStr::new("/etc/shadow")).is_err()); // Denied File
        assert!(path_output_dir(OsStr::new("/etc/ssl/private")).is_err()); // Denied Folder
        assert!(path_output_dir(OsStr::new("/nonexistant_test_path")).is_err()); // Missing Path
        assert!(path_output_dir(OsStr::new("/tmp\0with\0null")).is_err()); // Invalid CString
                                                                           // TODO: is_dir but fails to canonicalize()
                                                                           // TODO: Not-already-canonicalized paths

        assert!(path_output_dir(OsStr::from_bytes(b"/not\xffutf8")).is_err()); // Invalid UTF-8
                                                                               // TODO: Non-UTF8 path that actually does exist and is writable
    }

    // ---- path_readable ----

    // TODO: Use a `cfg` to pick some appropriate alternative paths for Windows
    #[test]
    fn path_readable_basic_functionality() {
        // Existing paths
        assert!(path_readable(OsStr::new("/")).is_ok()); // OK Folder
        assert!(path_readable(OsStr::new("/bin/sh")).is_ok()); // OK File
        assert!(path_readable(OsStr::new("/bin/../../etc/.././.")).is_ok()); // Not canonicalized
        assert!(path_readable(OsStr::new("/../../../..")).is_ok()); // Above root

        // Inaccessible, nonexistent, or invalid paths
        assert!(path_readable(OsStr::new("")).is_err()); // Empty String
        assert!(path_readable(OsStr::new("/etc/shadow")).is_err()); // Denied File
        assert!(path_readable(OsStr::new("/etc/ssl/private")).is_err()); // Denied Folder
        assert!(path_readable(OsStr::new("/nonexistant_test_path")).is_err()); // Missing Path
        assert!(path_readable(OsStr::new("/null\0containing")).is_err()); // Invalid CString
    }

    #[cfg(not(windows))]
    #[test]
    fn path_readable_invalid_utf8() {
        assert!(path_readable(OsStr::from_bytes(b"/not\xffutf8")).is_err()); // Invalid UTF-8
                                                                             // TODO: Non-UTF8 path that actually IS valid
    }
    #[cfg(windows)]
    #[test]
    fn path_readable_unpaired_surrogates() {
        unimplemented!()
        // TODO: #[cfg(windows) test with un-paired UTF-16 surrogates
    }

    // ---- dir_writable ----

    #[test]
    fn dir_writable_basic_functionality() {
        assert!(dir_writable(OsStr::new("/tmp")).is_ok()); // OK Folder
        assert!(dir_writable(OsStr::new("/dev/null")).is_err()); // OK File
        assert!(dir_writable(OsStr::new("/etc/shadow")).is_err()); // Denied File
        assert!(dir_writable(OsStr::new("/etc/ssl/private")).is_err()); // Denied Folder
        assert!(dir_writable(OsStr::new("/nonexistant_test_path")).is_err()); // Missing Path
        assert!(dir_writable(OsStr::new("/tmp\0with\0null")).is_err()); // Invalid CString
        assert!(dir_writable(OsStr::from_bytes(b"/not\xffutf8")).is_err()); // Invalid UTF-8
        assert!(dir_writable(OsStr::new("/")).is_err()); // Root
                                                         // TODO: is_dir but fails to canonicalize()
                                                         // TODO: Not-already-canonicalized paths
                                                         // TODO: Non-UTF8 path that actually does exist and is writable
    }

    // ---- filename_valid_portable ----

    const VALID_FILENAMES: &[&str] = &[
        // regular, space, and leading period
        "test1", "te st", ".test",
        // Stuff which would break if the DOS reserved names check is doing dumb pattern matching
        "lpt", "lpt0", "lpt10",
    ];

    // Paths which should pass because std::path::Path will recognize the separators
    // TODO: Actually run the tests on Windows to make sure they work
    #[cfg(windows)]
    const PATHS_WITH_NATIVE_SEPARATORS: &[&str] =
        &["re/lative", "/ab/solute", "re\\lative", "\\ab\\solute"];
    #[cfg(not(windows))]
    const PATHS_WITH_NATIVE_SEPARATORS: &[&str] = &["re/lative", "/ab/solute"];

    // Paths which should fail because std::path::Path won't recognize the separators and we don't
    // want them showing up in the components.
    #[cfg(windows)]
    const PATHS_WITH_FOREIGN_SEPARATORS: &[&str] = &["Classic Mac HD:Folder Name:File"];
    #[cfg(not(windows))]
    const PATHS_WITH_FOREIGN_SEPARATORS: &[&str] = &[
        "relative\\win32",
        "C:\\absolute\\win32",
        "\\drive\\relative\\win32",
        "\\\\unc\\path\\for\\win32",
        "Classic Mac HD:Folder Name:File",
    ];

    // Source: https://docs.microsoft.com/en-us/windows/desktop/FileIO/naming-a-file
    const INVALID_PORTABLE_FILENAMES: &[&str] = &[
        "test\x03",
        "test\x07",
        "test\x08",
        "test\x0B",
        "test\x7f", // Control characters (VFAT)
        "\"test\"",
        "<testsss",
        "testsss>",
        "testsss|",
        "testsss*",
        "testsss?",
        "?estsss", // VFAT
        "ends with space ",
        "ends_with_period.", // DOS/Win32
        "CON",
        "Con",
        "coN",
        "cOn",
        "CoN",
        "con",
        "lpt1",
        "com9", // Reserved names (DOS/Win32)
        "con.txt",
        "lpt1.dat", // DOS/Win32 API (Reserved names are extension agnostic)
        "",
        "\0",
    ]; // POSIX

    #[test]
    fn filename_valid_portable_accepts_valid_names() {
        for path in VALID_FILENAMES {
            assert!(filename_valid_portable(OsStr::new(path)).is_ok(), "{:?}", path);
        }
    }

    #[test]
    fn filename_valid_portable_refuses_path_separators() {
        for path in PATHS_WITH_NATIVE_SEPARATORS {
            assert!(filename_valid_portable(OsStr::new(path)).is_err(), "{:?}", path);
        }
        for path in PATHS_WITH_FOREIGN_SEPARATORS {
            assert!(filename_valid_portable(OsStr::new(path)).is_err(), "{:?}", path);
        }
    }

    #[test]
    fn filename_valid_portable_refuses_invalid_characters() {
        for fname in INVALID_PORTABLE_FILENAMES {
            assert!(filename_valid_portable(OsStr::new(fname)).is_err(), "{:?}", fname);
        }
    }

    #[test]
    fn filename_valid_portable_refuses_empty_strings() {
        assert!(filename_valid_portable(OsStr::new("")).is_err());
    }

    #[test]
    fn filename_valid_portable_enforces_length_limits() {
        // 256 characters
        let mut test_str = std::str::from_utf8(&[b'X'; 256]).expect("parsing constant");
        assert!(filename_valid_portable(OsStr::new(test_str)).is_err());

        // 255 characters (maximum for NTFS, ext2/3/4, and a lot of others)
        test_str = std::str::from_utf8(&[b'X'; 255]).expect("parsing constant");
        assert!(filename_valid_portable(OsStr::new(test_str)).is_ok());
    }

    #[cfg(not(windows))]
    #[test]
    fn filename_valid_portable_accepts_non_utf8_bytes() {
        // Ensure that we don't refuse invalid UTF-8 that "bag of bytes" POSIX allows
        assert!(filename_valid_portable(OsStr::from_bytes(b"\xff")).is_ok());
    }
    #[cfg(windows)]
    #[test]
    fn filename_valid_portable_accepts_unpaired_surrogates() {
        unimplemented!()
        // TODO: Test with un-paired UTF-16 surrogates
    }

    // ---- path_valid_portable ----

    #[test]
    fn path_valid_portable_accepts_valid_names() {
        for path in VALID_FILENAMES {
            assert!(path_valid_portable(OsStr::new(path)).is_ok(), "{:?}", path);
        }

        // No filename (.file_stem() returns None)
        assert!(path_valid_portable(OsStr::new("foo/..")).is_ok());
    }

    #[test]
    fn path_valid_portable_accepts_native_path_separators() {
        for path in PATHS_WITH_NATIVE_SEPARATORS {
            assert!(path_valid_portable(OsStr::new(path)).is_ok(), "{:?}", path);
        }

        // Verify that repeated separators are getting collapsed before filename_valid_portable
        // sees them.
        assert!(path_valid_portable(OsStr::new("/path//with/repeated//separators")).is_ok());
    }

    #[test]
    fn path_valid_portable_refuses_foreign_path_separators() {
        for path in PATHS_WITH_FOREIGN_SEPARATORS {
            assert!(path_valid_portable(OsStr::new(path)).is_err(), "{:?}", path);
        }
    }

    #[test]
    fn path_valid_portable_refuses_invalid_characters() {
        for fname in INVALID_PORTABLE_FILENAMES {
            assert!(path_valid_portable(OsStr::new(fname)).is_err(), "{:?}", fname);
        }
    }

    #[test]
    fn path_valid_portable_enforces_length_limits() {
        let mut test_string = String::with_capacity(255 * 130);
        while test_string.len() < 32761 {
            test_string.push_str(std::str::from_utf8(&[b'X'; 255]).expect("utf8 from literal"));
            test_string.push('/');
        }

        // >32760 characters
        assert!(path_valid_portable(OsStr::new(&test_string)).is_err());

        // 32760 characters (maximum for FAT32/VFAT/exFAT)
        test_string.truncate(32760);
        assert!(path_valid_portable(OsStr::new(&test_string)).is_ok());

        // 256 characters with no path separators
        test_string.truncate(255);
        test_string.push('X');
        assert!(path_valid_portable(OsStr::new(&test_string)).is_err());

        // 255 characters with no path separators
        test_string.truncate(255);
        assert!(path_valid_portable(OsStr::new(&test_string)).is_ok());
    }

    #[cfg(not(windows))]
    #[test]
    fn path_valid_portable_accepts_non_utf8_bytes() {
        // Ensure that we don't refuse invalid UTF-8 that "bag of bytes" POSIX allows
        assert!(path_valid_portable(OsStr::from_bytes(b"/\xff/foo")).is_ok());
    }
    #[cfg(windows)]
    #[test]
    fn filename_valid_portable_accepts_unpaired_surrogates() {
        unimplemented!()
        // TODO: Test with un-paired UTF-16 surrogates
    }
}
