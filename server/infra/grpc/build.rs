use std::error::Error;
use std::process::{exit, Command};

// From
// https://github.com/neoeinstein/protoc-gen-prost/blob/fe8e21a9d319c305cda0cfddd146ccddc73d36dd/example/build-with-buf/build.rs

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let status = Command::new("buf")
    .args(&[
        "generate",
        "buf.build/gigantic-minecraft/seichi-game-data",
        "--output",
        &out_dir,
    ])

    if !status.success() {
        exit(status.code().unwrap_or(-1))
    }

    Ok(())
}
