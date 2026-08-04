use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let json_dir = manifest_dir.join("../../opendata/json");
    let mut files: Vec<String> = fs::read_dir(&json_dir)
        .expect("Failed to read opendata/json directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".json") && name != "index.json")
        .collect();
    files.sort();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut content = String::from("pub const DESCRIPTORS: &[(&str, &str)] = &[\n");
    for file in &files {
        let key = file.trim_end_matches(".json");
        content.push_str(&format!(
            "    (\"{key}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../opendata/json/{file}\"))),\n"
        ));
    }
    content.push_str("];\n");
    fs::write(out_dir.join("descriptors.rs"), content).expect("Failed to write descriptors.rs");
    println!("cargo:rerun-if-changed={}", json_dir.display());
}
