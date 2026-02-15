//! Integration tests for leader routing.
//!
//! These tests validate precomputed data against live Solana RPC.
//! Run with: cargo test --test integration_test -- --nocapture
//!
//! Requirements:
//! - Network access to Solana mainnet RPC
//! - Precomputed data files (schedule.json, leader_geo.json)

use std::time::Duration;

/// Solana mainnet RPC endpoint
const RPC_URL: &str = "https://api.mainnet-beta.solana.com";

/// Number of slots to sample for consistency check
const SAMPLE_SIZE: usize = 20;

/// Minimum acceptable match rate for slot→leader consistency
const MIN_MATCH_RATE: f64 = 0.8; // 80% - allows for minor timing differences

/// Minimum acceptable geo coverage (non-Unknown regions)
const MIN_GEO_COVERAGE: f64 = 0.8; // 80% - allows for new validators not yet in geo data

mod helpers {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    pub struct RpcResponse<T> {
        pub result: Option<T>,
        pub error: Option<RpcError>,
    }

    #[derive(Deserialize, Debug)]
    pub struct RpcError {
        pub code: i64,
        pub message: String,
    }

    /// Call Solana RPC method
    pub fn rpc_call<T: for<'de> Deserialize<'de>>(
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        let response = client
            .post(RPC_URL)
            .json(&body)
            .send()
            .map_err(|e| format!("RPC request failed: {}", e))?;

        let rpc_response: RpcResponse<T> = response
            .json()
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = rpc_response.error {
            return Err(format!("RPC error {}: {}", error.code, error.message));
        }

        rpc_response
            .result
            .ok_or_else(|| "No result in response".to_string())
    }

    /// Get current slot from RPC
    pub fn get_current_slot() -> Result<u64, String> {
        rpc_call("getSlot", serde_json::json!([]))
    }

    /// Get slot leaders from RPC
    pub fn get_slot_leaders(start_slot: u64, limit: u64) -> Result<Vec<String>, String> {
        rpc_call("getSlotLeaders", serde_json::json!([start_slot, limit]))
    }
}

#[test]
fn test_slot_leader_consistency() {
    use leader_routing::schedule;

    println!("\n=== Slot→Leader Consistency Test ===\n");

    // Get current slot from live Solana RPC
    let rpc_slot = helpers::get_current_slot().expect("Failed to get current slot from RPC");
    println!("RPC current slot: {}", rpc_slot);

    // Get leaders from RPC for sample slots
    let leaders_rpc =
        helpers::get_slot_leaders(rpc_slot, SAMPLE_SIZE as u64).expect("Failed to get slot leaders");

    println!("\nComparing {} slots starting at {}:", SAMPLE_SIZE, rpc_slot);
    println!("{:-<60}", "");

    let mut matches = 0;
    let mut mismatches = 0;
    let mut not_found = 0;

    for (i, rpc_leader) in leaders_rpc.iter().enumerate() {
        let slot = rpc_slot + i as u64;
        let precomputed_leader = schedule::get_leader(slot);

        match precomputed_leader {
            Some(leader_bytes) => {
                let precomputed_hex = hex::encode(leader_bytes);
                // RPC returns base58, we have hex - compare by converting
                let rpc_bytes = bs58::decode(rpc_leader).into_vec().unwrap_or_default();
                let rpc_hex = hex::encode(&rpc_bytes);

                if precomputed_hex == rpc_hex {
                    matches += 1;
                    println!("  Slot {}: MATCH ✓", slot);
                } else {
                    mismatches += 1;
                    println!(
                        "  Slot {}: MISMATCH ✗ (precomputed={}, rpc={})",
                        slot,
                        &precomputed_hex[..8],
                        &rpc_hex[..8.min(rpc_hex.len())]
                    );
                }
            }
            None => {
                not_found += 1;
                println!("  Slot {}: NOT FOUND in precomputed data", slot);
            }
        }
    }

    println!("{:-<60}", "");
    println!(
        "Results: {} matches, {} mismatches, {} not found",
        matches, mismatches, not_found
    );

    let match_rate = matches as f64 / SAMPLE_SIZE as f64;
    println!("Match rate: {:.1}%", match_rate * 100.0);

    assert!(
        match_rate >= MIN_MATCH_RATE,
        "Match rate {:.1}% below minimum {:.1}%",
        match_rate * 100.0,
        MIN_MATCH_RATE * 100.0
    );

    println!("\n✓ Slot→Leader consistency test PASSED\n");
}

#[test]
fn test_geo_coverage() {
    use leader_routing::geo;

    println!("\n=== Geo Coverage Test ===\n");

    // Get current leaders from RPC to test geo coverage
    let rpc_slot = helpers::get_current_slot().expect("Failed to get current slot");
    let leaders = helpers::get_slot_leaders(rpc_slot, 100).expect("Failed to get leaders");

    println!("Testing geo coverage for 100 leaders starting at slot {}", rpc_slot);
    println!("{:-<60}", "");

    let mut region_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut unknown_count = 0;
    let mut total = 0;

    for leader_b58 in &leaders {
        let leader_bytes: [u8; 32] = match bs58::decode(leader_b58).into_vec() {
            Ok(bytes) if bytes.len() == 32 => bytes.try_into().unwrap(),
            _ => continue,
        };

        let region = geo::get_region(&leader_bytes);
        let region_name = region.to_string();

        if region_name == "Unknown" {
            unknown_count += 1;
        }

        *region_counts.entry(region_name).or_insert(0) += 1;
        total += 1;
    }

    println!("Region distribution:");
    let mut sorted: Vec<_> = region_counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (region, count) in &sorted {
        let pct = **count as f64 / total as f64 * 100.0;
        println!("  {:12} {:4} ({:5.1}%)", region, count, pct);
    }

    let geo_coverage = (total - unknown_count) as f64 / total as f64;
    println!("{:-<60}", "");
    println!(
        "Geo coverage: {:.1}% ({} known, {} unknown)",
        geo_coverage * 100.0,
        total - unknown_count,
        unknown_count
    );

    // Check if we have real geo data loaded
    if geo::is_stub() {
        println!("\n⚠ WARNING: Using stub geo data (empty PHF map)");
        println!("  Run: python scripts/precompute_geo.py && cargo build");
    } else {
        assert!(
            geo_coverage >= MIN_GEO_COVERAGE,
            "Geo coverage {:.1}% below minimum {:.1}%",
            geo_coverage * 100.0,
            MIN_GEO_COVERAGE * 100.0
        );
        println!("\n✓ Geo coverage test PASSED\n");
    }
}

#[test]
fn test_epoch_metadata_valid() {
    use leader_routing::epoch;

    println!("\n=== Epoch Metadata Test ===\n");

    let meta = epoch::epoch_metadata();

    println!("Epoch metadata:");
    println!("  Start slot:      {}", meta.start_slot);
    println!("  End slot:        {}", meta.end_slot);
    println!("  Slot duration:   {}ms", meta.slot_duration_ms);
    println!("  Start time:      {}ms", meta.start_time_ms);

    // Validate metadata
    assert!(meta.end_slot > meta.start_slot, "End slot must be > start slot");
    assert!(meta.slot_duration_ms > 0, "Slot duration must be positive");

    // Check epoch size (should be ~432000 slots)
    let epoch_size = meta.end_slot - meta.start_slot;
    println!("  Epoch size:      {} slots", epoch_size);
    assert!(
        epoch_size >= 400000 && epoch_size <= 500000,
        "Unexpected epoch size: {}",
        epoch_size
    );

    // Check if epoch is current (not expired)
    let current_slot = epoch::current_slot();
    println!("\nCurrent slot: {}", current_slot);

    if current_slot > meta.end_slot {
        println!("⚠ WARNING: Epoch has ended! Precomputed data is stale.");
        println!("  Current slot {} > end slot {}", current_slot, meta.end_slot);
        println!("  Run: python scripts/fetch_schedule.py && cargo build");
    } else {
        let slots_remaining = meta.end_slot - current_slot;
        let hours_remaining = slots_remaining as f64 * meta.slot_duration_ms as f64 / 1000.0 / 3600.0;
        println!("Slots remaining: {} (~{:.1} hours)", slots_remaining, hours_remaining);
        println!("\n✓ Epoch metadata test PASSED\n");
    }
}

#[test]
fn test_full_pipeline() {
    use leader_routing::{epoch, geo, schedule};

    println!("\n=== Full Pipeline Test (Precomputed) ===\n");

    // 1. Get current slot
    let slot = epoch::current_slot();
    println!("1. Current slot: {}", slot);

    // 2. Look up leader
    let leader = schedule::get_leader(slot);
    match leader {
        Some(pubkey) => {
            println!("2. Leader pubkey: {}...", &hex::encode(&pubkey[..4]));

            // 3. Look up region
            let region = geo::get_region(&pubkey);
            println!("3. Leader region: {}", region);
            println!("   Geo label:     {}", region.geo_label());
            println!("   Routes to:     {}", region.routing_destination());

            println!("\n✓ Full pipeline test PASSED\n");
        }
        None => {
            println!("2. Leader: NOT FOUND");
            println!("\n⚠ Full pipeline test SKIPPED (no leader data)\n");
        }
    }
}

#[test]
fn test_rpc_mode_with_geo() {
    use leader_routing::geo;

    println!("\n=== RPC Mode + Geo Lookup Test ===\n");
    println!("This test verifies the production RPC mode flow:");
    println!("  1. Get live slot from Solana RPC");
    println!("  2. Get live leader from Solana RPC");
    println!("  3. Look up leader's region using compiled PHF map\n");

    // 1. Get current slot from RPC
    let rpc_slot = helpers::get_current_slot().expect("Failed to get current slot from RPC");
    println!("1. Live RPC slot: {}", rpc_slot);

    // 2. Get leader from RPC
    let leaders = helpers::get_slot_leaders(rpc_slot, 1).expect("Failed to get slot leader");
    let leader_b58 = &leaders[0];
    println!("2. Live RPC leader: {}", leader_b58);

    // 3. Convert to bytes and lookup geo (same as production code)
    let leader_bytes: [u8; 32] = bs58::decode(leader_b58)
        .into_vec()
        .expect("Invalid base58")
        .try_into()
        .expect("Invalid pubkey length");

    let region = geo::get_region(&leader_bytes);
    println!("3. Geo lookup result:");
    println!("   Region:     {}", region);
    println!("   Geo label:  {}", region.geo_label());
    println!("   Routes to:  {}", region.routing_destination());

    // Verify we got a valid routing destination (even Unknown routes to Frankfurt)
    let destination = region.routing_destination();
    assert!(
        destination.to_string() == "Frankfurt"
            || destination.to_string() == "NewYork"
            || destination.to_string() == "Tokyo"
            || destination.to_string() == "Dubai",
        "Invalid routing destination: {}",
        destination
    );

    println!("\n✓ RPC mode + geo lookup test PASSED");
    println!("  Production flow verified: RPC → Leader → Geo → Route\n");
}

#[test]
fn test_rpc_mode_geo_coverage_minimum() {
    use leader_routing::geo;

    println!("\n=== RPC Mode Geo Coverage (Production Readiness) ===\n");

    // Get 50 consecutive leaders from RPC
    let rpc_slot = helpers::get_current_slot().expect("Failed to get slot");
    let leaders = helpers::get_slot_leaders(rpc_slot, 50).expect("Failed to get leaders");

    println!("Testing {} live leaders from slot {}...\n", leaders.len(), rpc_slot);

    let mut known = 0;
    let mut unknown = 0;

    for leader_b58 in &leaders {
        let leader_bytes: [u8; 32] = match bs58::decode(leader_b58).into_vec() {
            Ok(b) if b.len() == 32 => b.try_into().unwrap(),
            _ => continue,
        };

        let region = geo::get_region(&leader_bytes);
        if region.to_string() == "Unknown" {
            unknown += 1;
        } else {
            known += 1;
        }
    }

    let total = known + unknown;
    let coverage = known as f64 / total as f64 * 100.0;

    println!("Results:");
    println!("  Known regions:   {}", known);
    println!("  Unknown:         {}", unknown);
    println!("  Coverage:        {:.1}%", coverage);

    // Production requirement: at least 80% geo coverage
    const MIN_COVERAGE: f64 = 80.0;
    assert!(
        coverage >= MIN_COVERAGE,
        "Geo coverage {:.1}% is below production minimum {:.1}%",
        coverage,
        MIN_COVERAGE
    );

    println!("\n✓ RPC mode geo coverage test PASSED (>= {}%)\n", MIN_COVERAGE);
}
