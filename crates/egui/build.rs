use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-changed=/dev/null/roadwork-always-rebuild");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let mut x = nanos ^ ((std::process::id() as u64) << 32) | 1;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;

    let r = (x & 0xFF) as u8;
    let g = ((x >> 8) & 0xFF) as u8;
    let b = ((x >> 16) & 0xFF) as u8;

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let content = format!(
        "pub const BUILD_COLOR_R: u8 = {r};\npub const BUILD_COLOR_G: u8 = {g};\npub const BUILD_COLOR_B: u8 = {b};\n"
    );
    fs::write(out_dir.join("build_color.rs"), content).expect("Failed to write build_color.rs");
}
