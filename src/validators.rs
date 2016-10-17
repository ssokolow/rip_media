use std::error::Error;
use std::fs::File;
use std::path::Path;

pub fn inpath(value: String) -> Result<(), String> {
    // Return a more helpful nonexistant path message than "entity not found"
    if !Path::new(&value).exists() {
        return Err(format!("Path does not exist: {}", value));
    }

    // Test that the given path can be opened for reading and adjust failure messages
    File::open(&value).map(|_| ()).map_err(|e|
        format!("{}: {}", &value, e.description().to_string()))
}

pub fn set_size(value: String) -> Result<(), String> {
    if let Ok(num) = value.parse::<u32>() {
        if num >= 1_u32 {
            return Ok(());
        } else {
            return Err(format!("Set size must be 1 or greater (not \"{}\")", value));
        }
    }
    Err(format!("Set size must be an integer (whole number), not \"{}\"", value))
}
