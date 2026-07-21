use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let static_dir = manifest_dir.join("static");
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let out_dir = workspace_root.join("target/wme");
    let wasm_crate_dir = manifest_dir.parent().unwrap().join("wme-wasm");

    println!("cargo:rerun-if-changed=static");
    println!("cargo:rerun-if-changed=../wme-wasm/src");

    fs::create_dir_all(&out_dir).unwrap();

    // --- Build WASM via wasm-pack ---
    let pkg_dir = out_dir.join("pkg");

    if pkg_dir.exists() {
        fs::remove_dir_all(&pkg_dir).unwrap();
    }

    println!("cargo:warning=Building WASM module via wasm-pack...");

    let status = Command::new("wasm-pack")
        .args([
            "build",
            "--target",
            "no-modules",
            "--release",
            "--out-dir",
            pkg_dir.to_str().unwrap(),
            wasm_crate_dir.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to run wasm-pack. Install with: cargo install wasm-pack");

    if !status.success() {
        panic!("wasm-pack build failed");
    }

    // --- Embed WASM binary as base64 in JS ---
    let wasm_bytes = fs::read(pkg_dir.join("roadwork_wasm_bg.wasm"))
        .expect("Failed to read WASM binary for base64 encoding");
    let b64 = base64_encode(&wasm_bytes);
    let wasm_js = format!("const WASM_BYTES = \"{}\";\n", b64);
    fs::write(out_dir.join("wasm_bytes.js"), &wasm_js).expect("Failed to write wasm_bytes.js");

    // --- Copy static files into extension directory ---
    fs::copy(
        static_dir.join("manifest.json"),
        out_dir.join("manifest.json"),
    )
    .expect("Failed to copy manifest.json");
    fs::copy(static_dir.join("content.js"), out_dir.join("content.js"))
        .expect("Failed to copy content.js");
    fs::copy(
        static_dir.join("roadwork-wme.js"),
        out_dir.join("inject.js"),
    )
    .expect("Failed to copy inject.js");
    fs::copy(static_dir.join("style.css"), out_dir.join("style.css"))
        .expect("Failed to copy style.css");
    fs::copy(
        pkg_dir.join("roadwork_wasm.js"),
        out_dir.join("wasm_bindgen.js"),
    )
    .expect("Failed to copy wasm_bindgen.js");
    fs::copy(
        static_dir.join("wasm-iframe.html"),
        out_dir.join("wasm-iframe.html"),
    )
    .expect("Failed to copy wasm-iframe.html");
    fs::copy(
        static_dir.join("wasm-init.js"),
        out_dir.join("wasm-init.js"),
    )
    .expect("Failed to copy wasm-init.js");

    println!(
        "cargo:warning=Extension generated at: {}",
        out_dir.display(),
    );
}
