use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("src").display()
    );
    println!("cargo:rerun-if-changed=static");

    // 1. Build the WASM binary
    let output = Command::new("cargo")
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .current_dir(workspace_root)
        .output()
        .expect("Failed to run cargo. Is the wasm32-unknown-unknown target installed?\n  rustup target add wasm32-unknown-unknown");

    if !output.status.success() {
        panic!(
            "cargo build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // 2. Run wasm-bindgen
    let wasm_file = workspace_root.join("target/wasm32-unknown-unknown/release/Roadwork-rs.wasm");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let wasm_out = out_dir.join("wasm");
    fs::create_dir_all(&wasm_out).unwrap();

    let output = Command::new("wasm-bindgen")
        .args([
            "--target",
            "web",
            "--out-name",
            "Roadwork",
            "--out-dir",
            wasm_out.to_str().unwrap(),
            wasm_file.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run wasm-bindgen. Install with:\n  cargo install wasm-bindgen-cli");

    if !output.status.success() {
        panic!(
            "wasm-bindgen failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // 3. Embed index.html + generated WASM/JS files
    let static_dir = manifest_dir.join("static");
    let mut entries: Vec<String> = Vec::new();

    // index.html from static/
    let index_html = fs::read(static_dir.join("index.html")).expect("static/index.html not found");
    let bytes_literal: String = index_html
        .iter()
        .map(|b| format!("{b}"))
        .collect::<Vec<_>>()
        .join(",");
    entries.push(format!("(\"index.html\", &[{bytes_literal}]),"));

    // WASM/JS files from wasm-bindgen output
    for entry in fs::read_dir(&wasm_out).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            let abs = path.to_str().unwrap().to_string();
            entries.push(format!("(\"{name}\", include_bytes!(\"{abs}\")),"));
        }
    }

    let code = format!(
        "static STATIC_FILES: &[(&str, &[u8])] = &[{}];",
        entries.join("")
    );
    let dest_path = out_dir.join("static_files.rs");
    fs::write(dest_path, code).unwrap();
}
