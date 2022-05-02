use std::error::Error;
use std::process::{exit, Command};

// From
// https://github.com/neoeinstein/protoc-gen-prost/blob/fe8e21a9d319c305cda0cfddd146ccddc73d36dd/example/build-with-buf/build.rs

fn main() -> Result<(), Box<dyn Error>> {
    let status = Command::new("buf")
        .arg("generate")
        .arg("buf.build/gigantic-minecraft/seichi-game-data")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .unwrap();

    if !status.success() {
        exit(status.code().unwrap_or(-1))
    }

    Ok(())
}
