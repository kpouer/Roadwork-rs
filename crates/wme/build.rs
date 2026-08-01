use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

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
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // days since epoch
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let h = time_secs / 3600;
    let m = (time_secs % 3600) / 60;
    let s = time_secs % 60;

    // Simple date calculation (Zeller-like, valid from 2000-03-01)
    let y = 2000i64;
    let mut year = y;
    let mut remaining = days as i64 - (date_to_days(y, 3, 1) as i64);
    if remaining < 0 {
        year = 1999;
        remaining = days as i64 - (date_to_days(1999, 3, 1) as i64);
    }
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 3usize;
    let mut day = remaining;
    for (i, &md) in month_days.iter().enumerate().cycle().skip(2) {
        if day < md as i64 {
            month = i + 1;
            break;
        }
        day -= md as i64;
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year,
        month,
        day + 1,
        h,
        m,
        s
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn date_to_days(year: i64, month: u32, day: u32) -> u64 {
    let mut y = year;
    let mut m = month as i64;
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe) as u64
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
    copy_with_replacements(
        &ts_out_dir,
        &out_dir,
        "roadwork-wme.js",
        "inject.js",
        &version,
        &build_date,
    );
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
