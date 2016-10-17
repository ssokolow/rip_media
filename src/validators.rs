use std::error::Error;
use std::fs::File;
use std::io::ErrorKind;

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
