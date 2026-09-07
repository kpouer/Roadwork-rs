use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn walk_json(dir: &Path, base: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        return files;
    }
    for entry in fs::read_dir(dir).expect("Failed to read directory") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_json(&path, base));
        } else {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".json") {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                files.push(rel);
            }
        }
    }
    files
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let json_dir = manifest_dir.join("resources/country");
    let mut files = walk_json(&json_dir, &json_dir);
    files.sort();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut content = String::from("pub const COUNTRIES: &[(&str, &str)] = &[\n");
    for file in &files {
        content.push_str(&format!(
            "    (\"{file}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/resources/country/{file}\"))),\n"
        ));
    }
    content.push_str("];\n");
    fs::write(out_dir.join("countries.rs"), content).expect("Failed to write countries.rs");
    println!("cargo:rerun-if-changed={}", json_dir.display());
}
