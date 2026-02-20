//! Integration tests for leader routing.
//!
//! These tests validate geo lookup against live Solana RPC.
//! Run with: cargo test --test integration_test -- --nocapture
//!
//! Requirements:
//! - Network access to Solana mainnet RPC
//! - Precomputed geo data (leader_geo.json)

use std::time::Duration;

/// Solana mainnet RPC endpoint
const RPC_URL: &str = "https://api.mainnet-beta.solana.com";

/// Minimum acceptable geo coverage (non-Unknown regions)
const MIN_GEO_COVERAGE: f64 = 0.8; // 80%

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
fn test_rpc_to_geo_pipeline() {
    use leader_routing::geo;

    println!("\n=== RPC to Geo Pipeline Test ===\n");
    println!("Flow: RPC(slot) -> RPC(leader) -> PHF(geo) -> region\n");

    // 1. Get current slot from RPC
    let slot = helpers::get_current_slot().expect("Failed to get current slot from RPC");
    println!("1. Live RPC slot: {}", slot);

    // 2. Get leader from RPC
    let leaders = helpers::get_slot_leaders(slot, 1).expect("Failed to get slot leader");
    let leader_b58 = &leaders[0];
    println!("2. Live RPC leader: {}", leader_b58);

    // 3. Convert to bytes and lookup geo
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

    // Verify valid routing destination
    let destination = region.routing_destination().to_string();
    assert!(
        ["Frankfurt", "NewYork", "Tokyo", "Dubai"].contains(&destination.as_str()),
        "Invalid routing destination: {}",
        destination
    );

    println!("\n[OK] RPC to geo pipeline test PASSED\n");
}

#[test]
fn test_geo_coverage() {
    use leader_routing::geo;

    println!("\n=== Geo Coverage Test ===\n");

    // Get current leaders from RPC
    let slot = helpers::get_current_slot().expect("Failed to get current slot");
    let leaders = helpers::get_slot_leaders(slot, 100).expect("Failed to get leaders");

    println!("Testing geo coverage for 100 leaders starting at slot {}", slot);
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

    if geo::is_stub() {
        println!("\n[WARN] Using stub geo data (empty PHF map)");
        println!("  Run: python scripts/precompute_geo.py && cargo build");
    } else {
        assert!(
            geo_coverage >= MIN_GEO_COVERAGE,
            "Geo coverage {:.1}% below minimum {:.1}%",
            geo_coverage * 100.0,
            MIN_GEO_COVERAGE * 100.0
        );
        println!("\n[OK] Geo coverage test PASSED\n");
    }
}

#[test]
fn test_multiple_slots_geo_lookup() {
    use leader_routing::geo;

    println!("\n=== Multi-Slot Geo Lookup Test ===\n");

    let slot = helpers::get_current_slot().expect("Failed to get slot");
    let leaders = helpers::get_slot_leaders(slot, 50).expect("Failed to get leaders");

    println!("Testing {} live leaders from slot {}...\n", leaders.len(), slot);

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

    const MIN_COVERAGE: f64 = 80.0;
    assert!(
        coverage >= MIN_COVERAGE,
        "Geo coverage {:.1}% is below minimum {:.1}%",
        coverage,
        MIN_COVERAGE
    );

    println!("\n[OK] Multi-slot geo lookup test PASSED (>= {}%)\n", MIN_COVERAGE);
}
