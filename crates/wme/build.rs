use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3));
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

fn copy_with_replacements(
    src_dir: &std::path::Path,
    out_dir: &std::path::Path,
    filename: &str,
    dest_filename: &str,
    version: &str,
    build_date: &str,
) {
    let src = src_dir.join(filename);
    let dest = out_dir.join(dest_filename);
    let content = fs::read_to_string(&src).unwrap_or_else(|e| {
        panic!("Failed to read {}: {e}", src.display());
    });
    let updated = content
        .replace("__VERSION__", version)
        .replace("__BUILD_DATE__", build_date);
    fs::write(&dest, &updated).unwrap_or_else(|e| {
        panic!("Failed to write {}: {e}", dest.display());
    });
}

fn generate_icons(icon_src: &Path, out_dir: &Path) {
    let img = image::open(icon_src)
        .unwrap_or_else(|e| panic!("Failed to open icon source {}: {e}", icon_src.display()));
    let icons_dir = out_dir.join("icons");
    fs::create_dir_all(&icons_dir).unwrap();
    for (size, name) in [
        (16u32, "icon-16.png"),
        (24u32, "icon-24.png"),
        (32u32, "icon-32.png"),
        (48u32, "icon-48.png"),
        (128u32, "icon-128.png"),
    ] {
        let resized = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let dest = icons_dir.join(name);
        resized.save(&dest).unwrap_or_else(|e| {
            panic!("Failed to write {}: {e}", dest.display());
        });
    }
}

fn create_webstore_zip(out_dir: &Path, version: &str) {
    let zip_name = format!("roadwork-wme-{version}-webstore.zip");
    let zip_path = out_dir.join(&zip_name);
    let file = fs::File::create(&zip_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(out_dir, &mut files);
    for path in files {
        let rel = path
            .strip_prefix(out_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == zip_name || rel == "meta.js" {
            continue;
        }
        if rel.starts_with("pkg/") || rel.starts_with("ts/") {
            continue;
        }
        let bytes = fs::read(&path).unwrap();
        zip.start_file(&rel, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    println!(
        "cargo:warning=Chrome Web Store package ready at: {}",
        zip_path.display(),
    );
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn format_build_date() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string()
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let static_dir = manifest_dir.join("static");
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let out_dir = workspace_root.join("target/wme");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let build_date = format_build_date();

    // Expose build date to the crate at compile time
    println!("cargo:rustc-env=WME_BUILD_DATE={build_date}");

    // When this crate is built for wasm32 (from the wasm-pack call below), skip the
    // whole assembly: build scripts run on the host, so this prevents infinite recursion.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("wasm32") {
        return;
    }

    println!("cargo:rerun-if-changed=static");
    println!("cargo:rerun-if-changed=tsconfig.json");
    println!("cargo:rerun-if-changed=src");
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("media/icon.png").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates/core").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates/service").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates/db").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates/sync").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("crates/egui").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("opendata/roadwork").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("Cargo.lock").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("Cargo.toml").display()
    );

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
            manifest_dir.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to run wasm-pack. Install with: cargo install wasm-pack");

    if !status.success() {
        panic!("wasm-pack build failed");
    }

    // --- Build the egui desktop/web app for the extension tab ---
    println!("cargo:warning=Building egui app for the extension tab...");

    let egui_build = Command::new("cargo")
        .args([
            "build",
            "-p",
            "roadwork-egui",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .current_dir(workspace_root)
        .output()
        .expect("Failed to run cargo. Is the wasm32-unknown-unknown target installed?\n  rustup target add wasm32-unknown-unknown");

    if !egui_build.status.success() {
        panic!(
            "cargo build failed:\n{}",
            String::from_utf8_lossy(&egui_build.stderr)
        );
    }

    let app_dir = out_dir.join("app");
    fs::create_dir_all(&app_dir).unwrap();

    let egui_wasm = workspace_root.join("target/wasm32-unknown-unknown/release/Roadwork-rs.wasm");
    let egui_bindgen = Command::new("wasm-bindgen")
        .args([
            "--target",
            "web",
            "--out-name",
            "Roadwork",
            "--out-dir",
            app_dir.to_str().unwrap(),
            egui_wasm.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run wasm-bindgen. Install with:\n  cargo install wasm-bindgen-cli");

    if !egui_bindgen.status.success() {
        panic!(
            "wasm-bindgen failed:\n{}",
            String::from_utf8_lossy(&egui_bindgen.stderr)
        );
    }

    fs::copy(static_dir.join("app.html"), app_dir.join("index.html"))
        .expect("Failed to copy app.html");
    fs::copy(static_dir.join("app.js"), app_dir.join("app.js")).expect("Failed to copy app.js");

    // --- Compile TypeScript sources ---
    println!("cargo:warning=Compiling TypeScript sources...");

    let tsc_bin = workspace_root.join("node_modules/.bin/tsc");
    let ts_status = Command::new(&tsc_bin)
        .args(["-p", manifest_dir.join("tsconfig.json").to_str().unwrap()])
        .status()
        .expect(
            "Failed to run tsc. TypeScript must be installed: run `npm install` at the repository root",
        );

    if !ts_status.success() {
        panic!("TypeScript compilation failed");
    }

    let ts_out_dir = out_dir.join("ts");

    // --- Embed WASM binary as base64 in JS ---
    let wasm_bytes = fs::read(pkg_dir.join("roadwork_wme_bg.wasm"))
        .expect("Failed to read WASM binary for base64 encoding");
    let b64 = base64_encode(&wasm_bytes);
    let wasm_js = format!("const WASM_BYTES = \"{}\";\n", b64);
    fs::write(out_dir.join("wasm_bytes.js"), &wasm_js).expect("Failed to write wasm_bytes.js");

    // --- Copy static files into extension directory ---
    copy_with_replacements(
        &static_dir,
        &out_dir,
        "manifest.json",
        "manifest.json",
        &version,
        &build_date,
    );
    copy_with_replacements(
        &static_dir,
        &out_dir,
        "meta.js",
        "meta.js",
        &version,
        &build_date,
    );
    // --- Copy locale files and embed them into locale-data.js ---
    let locales_src = static_dir.join("locales");
    let locales_out = out_dir.join("locales");
    fs::create_dir_all(&locales_out).unwrap();
    for entry in fs::read_dir(&locales_src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            let dest = locales_out.join(path.file_name().unwrap());
            fs::copy(&path, &dest).unwrap_or_else(|e| {
                panic!("Failed to copy {}: {e}", path.display());
            });
        }
    }

    // Build a JS object with all locales embedded for the inject script
    let mut locale_entries: Vec<String> = Vec::new();
    for entry in fs::read_dir(&locales_out).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            let json = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
            locale_entries.push(format!("\"{}\": {}", stem, json));
        }
    }
    let locale_data_js = format!(
        "window.__ROADWORK_LOCALE_DATA_ALL__ = {{ {} }};\n",
        locale_entries.join(", ")
    );
    fs::write(out_dir.join("locale-data.js"), &locale_data_js)
        .expect("Failed to write locale-data.js");

    // --- Concatenate locale-data.js + i18n.js + roadwork-wme.js into inject.js ---
    let locale_data_js =
        fs::read_to_string(out_dir.join("locale-data.js")).expect("Failed to read locale-data.js");
    let i18n_js = fs::read_to_string(ts_out_dir.join("i18n.js")).expect("Failed to read i18n.js");
    let rw_js = fs::read_to_string(ts_out_dir.join("roadwork-wme.js"))
        .expect("Failed to read roadwork-wme.js");
    let combined = format!("{}\n{}\n{}\n", locale_data_js, i18n_js, rw_js);
    let combined = combined
        .replace("__VERSION__", &version)
        .replace("__BUILD_DATE__", &build_date);
    fs::write(out_dir.join("inject.js"), &combined).expect("Failed to write inject.js");
    fs::copy(ts_out_dir.join("content.js"), out_dir.join("content.js"))
        .expect("Failed to copy content.js");
    fs::copy(static_dir.join("style.css"), out_dir.join("style.css"))
        .expect("Failed to copy style.css");
    fs::copy(
        pkg_dir.join("roadwork_wme.js"),
        out_dir.join("wasm_bindgen.js"),
    )
    .expect("Failed to copy wasm_bindgen.js");
    fs::copy(
        static_dir.join("wasm-iframe.html"),
        out_dir.join("wasm-iframe.html"),
    )
    .expect("Failed to copy wasm-iframe.html");
    fs::copy(
        ts_out_dir.join("wasm-init.js"),
        out_dir.join("wasm-init.js"),
    )
    .expect("Failed to copy wasm-init.js");
    fs::copy(
        ts_out_dir.join("wasm-worker.js"),
        out_dir.join("wasm-worker.js"),
    )
    .expect("Failed to copy wasm-worker.js");

    // --- Generate extension icons for the Chrome Web Store ---
    println!("cargo:warning=Generating extension icons...");
    generate_icons(&workspace_root.join("media/icon.png"), &out_dir);

    // --- Package the extension for the Chrome Web Store ---
    println!("cargo:warning=Packaging extension for the Chrome Web Store...");
    create_webstore_zip(&out_dir, &version);

    println!(
        "cargo:warning=Extension generated at: {}",
        out_dir.display(),
    );
}
