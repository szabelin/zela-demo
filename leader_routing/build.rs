//! Build script for leader_routing.
//!
//! ## What This Generates
//! - `phf_schedule.rs`: PHF map for O(1) slot -> validator lookup
//! - `epoch.rkyv`: rkyv-serialized epoch metadata for zero-copy access
//!
//! ## Prerequisites
//! Run `python scripts/fetch_schedule.py` before building to generate `data/schedule.json`.
//! Without this file, stub data is generated (empty PHF map, placeholder epoch metadata).
//!
//! ## Data Flow
//! 1. Python script fetches leader schedule from Solana RPC
//! 2. Outputs `data/schedule.json` with epoch metadata and (slot_offset, pubkey) entries
//! 3. This build script reads the JSON and generates:
//!    - PHF map: `pub static SLOT_TO_VALIDATOR: phf::Map<u64, [u8; 32]>`
//!    - rkyv blob: EpochMetadata { start_time_ms, slot_duration_ms, start_slot, end_slot }
//!
//! ## Assumptions
//! - Solana slot duration is 400ms (configurable in schedule.json)
//! - Pubkeys are 32 bytes (Ed25519 public keys)
//! - Epoch typically contains ~432,000 slots (~2-3 days)
//! - PHF map size: ~40 bytes overhead + ~40 bytes per entry
//!
//! ## Error Handling
//! - Missing schedule.json: Creates stub files, emits cargo:warning
//! - Invalid pubkey length (!= 32): Skips entry, logs warning
//! - Empty schedule: Generates empty PHF map (valid but useless)
//!
//! ## CI Usage
//! Set `LEADER_ROUTING_REQUIRE_DATA=1` to fail the build if schedule.json is missing.
//! This prevents accidentally deploying with stub data in production.

use serde::Deserialize;
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

    let schedule_path = "data/schedule.json";

    // Check if schedule.json exists
    if !Path::new(schedule_path).exists() {
        // CI mode: fail if stub data would be used
        if env::var("LEADER_ROUTING_REQUIRE_DATA").is_ok() {
            eprintln!("=== BUILD FAILED: Missing schedule data ===");
            eprintln!("LEADER_ROUTING_REQUIRE_DATA is set but data/schedule.json is missing.");
            eprintln!();
            eprintln!("To fix:");
            eprintln!("  1. Install Python dependencies: pip install requests base58");
            eprintln!("  2. Run: python scripts/fetch_schedule.py [RPC_URL]");
            eprintln!("  3. Rebuild: cargo build");
            eprintln!();
            eprintln!("Or unset LEADER_ROUTING_REQUIRE_DATA to use stub data for development.");
            panic!("schedule.json required in CI mode");
        }

        // Create a minimal stub for initial compilation
        // This allows `cargo check` to work without running the Python script
        create_stub_files();
        return;
    }

    // Read and parse schedule.json
    let json = fs::read_to_string(schedule_path)
        .expect("Failed to read data/schedule.json - run: python scripts/fetch_schedule.py");

    let schedule: Schedule = serde_json::from_str(&json)
        .expect("Failed to parse schedule.json - check JSON format");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir);

    // Generate PHF map for slot -> validator
    generate_phf_map(&schedule, out_path);

    // Serialize epoch metadata to rkyv
    generate_epoch_metadata(&schedule.metadata, out_path);
}

fn generate_phf_map(schedule: &Schedule, out_path: &Path) {
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

/// Create stub files for initial compilation without schedule.json
fn create_stub_files() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = Path::new(&out_dir);

    // Stub PHF map (empty)
    let phf_path = out_path.join("phf_schedule.rs");
    let mut file = fs::File::create(&phf_path).expect("Failed to create phf_schedule.rs");
    writeln!(file, "/// STUB: Run python scripts/fetch_schedule.py to generate real data")
        .expect("Failed to write");
    writeln!(
        file,
        "pub static SLOT_TO_VALIDATOR: phf::Map<u64, [u8; 32]> = phf::phf_map! {{}};"
    )
    .expect("Failed to write");

    // Stub epoch metadata
    let stub_metadata = EpochMetadata {
        start_time_ms: 0,
        slot_duration_ms: 400,
        start_slot: 0,
        end_slot: 432000,
    };
    let metadata_bytes =
        rkyv::to_bytes::<_, 256>(&stub_metadata).expect("Failed to serialize stub metadata");
    let rkyv_path = out_path.join("epoch.rkyv");
    fs::write(&rkyv_path, &metadata_bytes).expect("Failed to write epoch.rkyv");

    println!("cargo:warning=Using stub data - run: python scripts/fetch_schedule.py");
}
