use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct DescriptorMetadata {
    country: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Descriptor {
    metadata: DescriptorMetadata,
}

#[derive(Debug, Serialize)]
struct IndexEntry {
    key: String,
    path: String,
    country: String,
    name: String,
    size: u64,
    modified: String,
}

#[derive(Debug, Serialize)]
struct Index {
    version: u32,
    #[serde(rename = "generatedAt")]
    generated_at: String,
    count: usize,
    files: Vec<IndexEntry>,
}

fn normalize_country(country: &str) -> &str {
    if country.starts_with("USA") {
        "USA"
    } else {
        country
    }
}

fn iso_timestamp(meta: &fs::Metadata) -> String {
    meta.modified()
        .map(|t| {
            let datetime: chrono::DateTime<Utc> = t.into();
            datetime.to_rfc3339()
        })
        .unwrap_or_default()
}

fn walk_json(dir: &Path, base: &Path, entries: &mut Vec<IndexEntry>) {
    if !dir.is_dir() {
        return;
    }
    for entry in fs::read_dir(dir).expect("Failed to read directory") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();
        if path.is_dir() {
            let dir_name = path.file_name().unwrap().to_string_lossy();
            if dir_name == "broken" {
                continue;
            }
            walk_json(&path, base, entries);
        } else {
            let name = path.file_name().unwrap().to_string_lossy();
            if !name.ends_with(".json") || name == "index.json" {
                continue;
            }
            let content = fs::read_to_string(&path).expect("Failed to read descriptor");
            let descriptor: Descriptor =
                serde_json::from_str(&content).expect("Failed to parse descriptor");

            let country_raw = descriptor.metadata.country.as_deref().unwrap_or("Unknown");
            let country = normalize_country(country_raw).to_string();

            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let key = name.trim_end_matches(".json").to_string();
            let meta = fs::metadata(&path).expect("Failed to get metadata");

            entries.push(IndexEntry {
                key,
                path: rel,
                country,
                name: descriptor.metadata.name,
                size: meta.len(),
                modified: iso_timestamp(&meta),
            });
        }
    }
}

fn main() {
    let script_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = script_dir.parent().unwrap().parent().unwrap();
    let roadwork_dir = workspace_root.join("opendata/roadwork");

    if !roadwork_dir.exists() {
        eprintln!("Directory does not exist: {}", roadwork_dir.display());
        std::process::exit(1);
    }

    let mut files = Vec::new();
    walk_json(&roadwork_dir, &roadwork_dir, &mut files);
    files.sort_by(|a, b| a.key.cmp(&b.key));

    let index = Index {
        version: 1,
        generated_at: Utc::now().to_rfc3339(),
        count: files.len(),
        files,
    };

    let index_json = serde_json::to_string_pretty(&index).expect("Failed to serialize index");
    let index_path = roadwork_dir.join("index.json");
    fs::write(&index_path, index_json).expect("Failed to write index.json");

    println!(
        "Generated {} ({} files)",
        index_path.strip_prefix(workspace_root).unwrap().display(),
        index.count
    );
}
