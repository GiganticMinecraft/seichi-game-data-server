#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
