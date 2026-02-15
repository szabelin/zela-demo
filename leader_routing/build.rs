//! Build script for leader_routing.
//!
//! ## What This Generates
//! - `phf_schedule.rs`: PHF map for O(1) slot -> validator lookup
//! - `phf_geo.rs`: PHF map for O(1) validator -> region lookup
//! - `epoch.rkyv`: rkyv-serialized epoch metadata for zero-copy access
//!
//! ## Prerequisites
//! Run these Python scripts before building:
//! - `python scripts/fetch_schedule.py` -> `data/schedule.json`
//! - `python scripts/precompute_geo.py` -> `data/leader_geo.json`
//!
//! Without these files, stub data is generated.
//!
//! ## Data Flow
//! 1. Python scripts fetch data from Solana RPC and ip-api.com
//! 2. This build script reads the JSON files and generates PHF maps
//!
//! ## CI Usage
//! Set `LEADER_ROUTING_REQUIRE_DATA=1` to fail the build if data files are missing.

use serde::Deserialize;
use std::collections::HashMap;
use std::io::Write;
use std::{env, fs, path::Path};

/// Epoch metadata structure (matches Python output)
#[derive(Deserialize, rkyv::Archive, rkyv::Serialize)]
struct EpochMetadata {
    start_time_ms: u64,
    slot_duration_ms: u64,
    start_slot: u64,
    end_slot: u64,
}

/// Schedule JSON structure
#[derive(Deserialize)]
struct Schedule {
    metadata: EpochMetadata,
    /// Entries: [(slot_offset, pubkey_bytes), ...]
    entries: Vec<(u64, Vec<u8>)>,
}

fn main() {
    println!("cargo:rerun-if-changed=data/schedule.json");
    println!("cargo:rerun-if-changed=data/leader_geo.json");

    let schedule_path = "data/schedule.json";
    let geo_path = "data/leader_geo.json";

    // Check if data files exist
    let schedule_exists = Path::new(schedule_path).exists();
    let geo_exists = Path::new(geo_path).exists();

    if !schedule_exists || !geo_exists {
        // CI mode: fail if stub data would be used
        if env::var("LEADER_ROUTING_REQUIRE_DATA").is_ok() {
            eprintln!("=== BUILD FAILED: Missing data files ===");
            if !schedule_exists {
                eprintln!("  - data/schedule.json is missing");
            }
            if !geo_exists {
                eprintln!("  - data/leader_geo.json is missing");
            }
            eprintln!();
            eprintln!("To fix:");
            eprintln!("  1. pip install requests base58");
            eprintln!("  2. python scripts/fetch_schedule.py");
            eprintln!("  3. python scripts/precompute_geo.py");
            eprintln!("  4. cargo build");
            panic!("Data files required in CI mode");
        }

        // Create stub files for initial compilation
        create_stub_files();
        return;
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir);

    // Process schedule.json
    let schedule_json = fs::read_to_string(schedule_path)
        .expect("Failed to read schedule.json");
    let schedule: Schedule = serde_json::from_str(&schedule_json)
        .expect("Failed to parse schedule.json");

    generate_slot_to_validator_phf(&schedule, out_path);
    generate_epoch_metadata(&schedule.metadata, out_path);

    // Process leader_geo.json
    let geo_json = fs::read_to_string(geo_path)
        .expect("Failed to read leader_geo.json");
    let geo_map: HashMap<String, String> = serde_json::from_str(&geo_json)
        .expect("Failed to parse leader_geo.json");

    generate_validator_to_region_phf(&geo_map, out_path);
}

fn generate_slot_to_validator_phf(schedule: &Schedule, out_path: &Path) {
    let mut phf_builder = phf_codegen::Map::new();
    let mut valid_entries = 0;
    let mut skipped_entries = 0;

    for (slot_offset, pubkey_bytes) in &schedule.entries {
        // Validate pubkey length
        if pubkey_bytes.len() != 32 {
            eprintln!(
                "Warning: skipping entry with invalid pubkey length: {} (expected 32)",
                pubkey_bytes.len()
            );
            skipped_entries += 1;
            continue;
        }

        // PHF codegen requires values as Rust literals.
        // We format [u8; 32] as: [0x12, 0x34, 0x56, ...]
        // This produces valid Rust syntax that compiles directly into the PHF map.
        let byte_literal = format!(
            "[{}]",
            pubkey_bytes
                .iter()
                .map(|b| format!("0x{:02x}", b))
                .collect::<Vec<_>>()
                .join(", ")
        );

        phf_builder.entry(*slot_offset, &byte_literal);
        valid_entries += 1;
    }

    // Validate we have entries (warn if empty, but don't fail - stub is valid)
    if valid_entries == 0 && !schedule.entries.is_empty() {
        eprintln!("Warning: All {} entries were invalid, PHF map will be empty", schedule.entries.len());
    }

    let phf_path = out_path.join("phf_schedule.rs");
    let mut file = fs::File::create(&phf_path).expect("Failed to create phf_schedule.rs");

    writeln!(
        file,
        "/// Auto-generated PHF map: slot offset -> validator pubkey"
    )
    .expect("Failed to write");
    writeln!(
        file,
        "/// Generated by build.rs from data/schedule.json"
    )
    .expect("Failed to write");
    writeln!(
        file,
        "/// Valid entries: {}, Skipped: {}",
        valid_entries, skipped_entries
    )
    .expect("Failed to write");
    writeln!(
        file,
        "pub static SLOT_TO_VALIDATOR: phf::Map<u64, [u8; 32]> = {};",
        phf_builder.build()
    )
    .expect("Failed to write PHF map");

    // Log build results with size estimate
    // PHF map size: ~40 bytes overhead + ~40 bytes per entry (u64 key + [u8;32] value + metadata)
    let estimated_size_kb = (40 + valid_entries * 40) / 1024;

    if valid_entries > 0 {
        println!(
            "cargo:warning=Generated PHF map: {} entries, ~{}KB estimated size",
            valid_entries, estimated_size_kb
        );
    } else {
        println!("cargo:warning=PHF map is empty - using stub data");
    }
    if skipped_entries > 0 {
        println!("cargo:warning=Skipped {} invalid entries", skipped_entries);
    }
}

fn generate_epoch_metadata(metadata: &EpochMetadata, out_path: &Path) {
    let metadata_bytes = rkyv::to_bytes::<_, 256>(metadata)
        .expect("Failed to serialize epoch metadata to rkyv");

    let rkyv_path = out_path.join("epoch.rkyv");
    fs::write(&rkyv_path, &metadata_bytes).expect("Failed to write epoch.rkyv");
}

/// Map region name to u8 code for compact storage.
fn region_to_u8(region: &str) -> u8 {
    match region {
        "Frankfurt" => 0,
        "Dubai" => 1,
        "NewYork" => 2,
        "Tokyo" => 3,
        _ => 4, // Unknown
    }
}

fn generate_validator_to_region_phf(geo_map: &HashMap<String, String>, out_path: &Path) {
    let mut entries = Vec::new();
    let mut valid_entries = 0;
    let mut skipped_entries = 0;

    for (pubkey_b58, region) in geo_map {
        // Decode base58 pubkey to bytes
        let pubkey_bytes = match bs58::decode(pubkey_b58).into_vec() {
            Ok(bytes) if bytes.len() == 32 => bytes,
            Ok(bytes) => {
                eprintln!(
                    "Warning: skipping pubkey with invalid length: {} (got {})",
                    &pubkey_b58[..8.min(pubkey_b58.len())],
                    bytes.len()
                );
                skipped_entries += 1;
                continue;
            }
            Err(e) => {
                eprintln!(
                    "Warning: failed to decode pubkey {}: {}",
                    &pubkey_b58[..8.min(pubkey_b58.len())],
                    e
                );
                skipped_entries += 1;
                continue;
            }
        };

        // Format key as [u8; 32] literal
        let key_literal = format!(
            "[{}]",
            pubkey_bytes
                .iter()
                .map(|b| format!("0x{:02x}", b))
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Region as u8
        let region_code = region_to_u8(region);

        entries.push((key_literal, region_code));
        valid_entries += 1;
    }

    let phf_path = out_path.join("phf_geo.rs");
    let mut file = fs::File::create(&phf_path).expect("Failed to create phf_geo.rs");

    writeln!(file, "/// Auto-generated PHF map: validator pubkey -> region").unwrap();
    writeln!(file, "/// Generated by build.rs from data/leader_geo.json").unwrap();
    writeln!(file, "/// Valid entries: {}, Skipped: {}", valid_entries, skipped_entries).unwrap();

    // Use phf_map! macro directly for [u8; 32] keys
    writeln!(file, "pub static VALIDATOR_TO_REGION: phf::Map<[u8; 32], u8> = phf::phf_map! {{").unwrap();
    for (key, value) in &entries {
        writeln!(file, "    {} => {}u8,", key, value).unwrap();
    }
    writeln!(file, "}};").unwrap();

    println!(
        "cargo:warning=Generated geo PHF map: {} validators",
        valid_entries
    );
}

/// Create stub files for initial compilation without data files.
///
/// # Why Stub Files Are Needed
///
/// The Rust compiler needs valid PHF maps at compile time because they are
/// included via `include!()` macros. Without stub files:
/// 1. Fresh clones cannot compile (missing generated files)
/// 2. CI cannot build without running Python scripts first
/// 3. IDE analysis fails on the include!() statements
///
/// Stub files provide empty-but-valid PHF maps that compile successfully.
/// The runtime code handles empty maps gracefully (returns Unknown region,
/// returns None for slot lookup, etc.).
///
/// # Files Generated
/// - `phf_schedule.rs`: Empty slot -> validator map
/// - `phf_geo.rs`: Empty validator -> region map
/// - `epoch.rkyv`: Default epoch metadata (slot 0-432000)
fn create_stub_files() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir);

    // Stub PHF schedule map (empty) - allows compilation before fetch_schedule.py runs
    let phf_path = out_path.join("phf_schedule.rs");
    let mut file = fs::File::create(&phf_path).expect("Failed to create phf_schedule.rs");
    writeln!(file, "/// STUB: Run python scripts/fetch_schedule.py to generate real data")
        .expect("Failed to write");
    writeln!(
        file,
        "pub static SLOT_TO_VALIDATOR: phf::Map<u64, [u8; 32]> = phf::phf_map! {{}};"
    )
    .expect("Failed to write");

    // Stub PHF geo map (empty) - allows compilation before precompute_geo.py runs
    let geo_path = out_path.join("phf_geo.rs");
    let mut geo_file = fs::File::create(&geo_path).expect("Failed to create phf_geo.rs");
    writeln!(geo_file, "/// STUB: Run python scripts/precompute_geo.py to generate real data")
        .expect("Failed to write");
    writeln!(
        geo_file,
        "pub static VALIDATOR_TO_REGION: phf::Map<[u8; 32], u8> = phf::phf_map! {{}};"
    )
    .expect("Failed to write");

    // Stub epoch metadata with default Solana values
    let stub_metadata = EpochMetadata {
        start_time_ms: 0,
        slot_duration_ms: 400, // Solana's ~400ms slot time
        start_slot: 0,
        end_slot: 432000, // Standard epoch length
    };
    let metadata_bytes =
        rkyv::to_bytes::<_, 256>(&stub_metadata).expect("Failed to serialize stub metadata");
    let rkyv_path = out_path.join("epoch.rkyv");
    fs::write(&rkyv_path, &metadata_bytes).expect("Failed to write epoch.rkyv");

    println!("cargo:warning=Using stub data - run: python scripts/fetch_schedule.py && python scripts/precompute_geo.py");
}
