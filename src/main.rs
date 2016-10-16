#[macro_use]
extern crate clap;
use clap::App;

fn main() {
    App::new("rip_media")
        .about("Simple frontend for backing up physical media")
        .version(crate_version!())
        .get_matches();
}
