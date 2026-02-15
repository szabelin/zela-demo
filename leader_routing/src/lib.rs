//! # Leader Routing
//!
//! A Zela procedure that determines which server region is closest to
//! the current Solana leader validator.
//!
//! ## How it works
//!
//! 1. Calculate current slot from system time (zero-copy epoch metadata)
//! 2. Look up leader for that slot (O(1) PHF lookup)
//! 3. Look up leader's region (stub: returns Frankfurt)
//! 4. Return the closest region
//!
//! ## Modes
//!
//! - `precomputed`: Use PHF lookup only (production, 0ms)
//! - `rpc`: Use live Solana RPC only (baseline)
//! - `verify`: Run both and compare (testing)
//!
//! ## Performance
//!
//! - Zero WASM startup cost (PHF compiled-in, rkyv zero-copy)
//! - All lookups O(1)
//! - No runtime allocation (in precomputed mode)

pub mod epoch;
pub mod geo;
pub mod region;
pub mod schedule;

use serde::{Deserialize, Serialize};
use zela_std::{zela_custom_procedure, rpc_client::RpcClient, CustomProcedure, RpcError};

/// Zela procedure entry point.
pub struct LeaderRouting;

/// Execution mode.
#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Use precomputed PHF lookup only (production).
    #[default]
    Precomputed,
    /// Use live Solana RPC only (baseline comparison).
    Rpc,
    /// Run both and compare results (testing/verification).
    Verify,
}

/// Input parameters.
#[derive(Deserialize, Debug, Default)]
pub struct Input {
    /// Execution mode (default: precomputed).
    #[serde(default)]
    pub mode: Mode,
}

/// Debug information for verify mode.
#[derive(Serialize, Debug)]
pub struct DebugInfo {
    /// Slot from precomputed calculation.
    pub precomputed_slot: u64,
    /// Slot from live RPC.
    pub rpc_slot: u64,
    /// Whether slots match.
    pub slots_match: bool,
    /// Leader from precomputed lookup.
    pub precomputed_leader: Option<String>,
    /// Leader from RPC getSlotLeaders.
    pub rpc_leader: Option<String>,
    /// Whether leaders match.
    pub leaders_match: bool,
}

/// Output data.
#[derive(Serialize, Debug)]
pub struct Output {
    /// Current Solana slot.
    pub slot: u64,
    /// Leader validator pubkey (hex encoded).
    pub leader: String,
    /// Geographic location of the leader.
    pub leader_geo: String,
    /// Closest Zela region to the leader.
    pub closest_region: String,
    /// Debug info (only present in verify mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<DebugInfo>,
}

impl CustomProcedure for LeaderRouting {
    type Params = Input;
    type ErrorData = ();
    type SuccessData = Output;

    async fn run(params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        match params.mode {
            Mode::Precomputed => run_precomputed().await,
            Mode::Rpc => run_rpc().await,
            Mode::Verify => run_verify().await,
        }
    }

    const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
}

/// Precomputed mode: PHF lookup only (production path).
async fn run_precomputed() -> Result<Output, RpcError<()>> {
    let slot = epoch::current_slot();
    let meta = epoch::epoch_metadata();

    // Check epoch boundary
    if slot > meta.end_slot {
        return Err(RpcError {
            code: 410,
            message: format!(
                "Epoch ended. Redeploy required. computed_slot={}, end_slot={}",
                slot, meta.end_slot
            ),
            data: None,
        });
    }

    let leader_pubkey = schedule::get_leader(slot).ok_or_else(|| RpcError {
        code: 404,
        message: format!("No leader found for slot {}", slot),
        data: None,
    })?;

    let leader_hex = hex::encode(leader_pubkey);
    let region = geo::get_region(&leader_pubkey);

    log::info!("Precomputed: slot={} leader={}...", slot, &leader_hex[..8]);

    Ok(Output {
        slot,
        leader: leader_hex,
        leader_geo: region.geo_label().to_string(),
        closest_region: region.to_string(),
        debug: None,
    })
}

/// RPC mode: live Solana RPC only (baseline comparison).
async fn run_rpc() -> Result<Output, RpcError<()>> {
    let client = RpcClient::new();

    let slot = client.get_slot().await.map_err(|e| RpcError {
        code: 500,
        message: format!("RPC get_slot failed: {}", e),
        data: None,
    })?;

    // Get leader for this slot using getSlotLeaders
    let leaders = client
        .get_slot_leaders(slot, 1)
        .await
        .map_err(|e| RpcError {
            code: 500,
            message: format!("RPC get_slot_leaders failed: {}", e),
            data: None,
        })?;

    let leader_pubkey = leaders.first().ok_or_else(|| RpcError {
        code: 404,
        message: format!("No leader returned for slot {}", slot),
        data: None,
    })?;

    let leader_hex = leader_pubkey.to_string();
    let leader_bytes: [u8; 32] = leader_pubkey.to_bytes();
    let region = geo::get_region(&leader_bytes);

    log::info!("RPC: slot={} leader={}...", slot, &leader_hex[..8]);

    Ok(Output {
        slot,
        leader: leader_hex,
        leader_geo: region.geo_label().to_string(),
        closest_region: region.to_string(),
        debug: None,
    })
}

/// Verify mode: run both and compare (testing).
async fn run_verify() -> Result<Output, RpcError<()>> {
    let client = RpcClient::new();

    // 1. Precomputed slot
    let precomputed_slot = epoch::current_slot();

    // 2. RPC slot
    let rpc_slot = client.get_slot().await.map_err(|e| RpcError {
        code: 500,
        message: format!("RPC get_slot failed: {}", e),
        data: None,
    })?;

    // 3. Precomputed leader
    let precomputed_leader = schedule::get_leader(precomputed_slot);
    let precomputed_leader_hex = precomputed_leader.map(hex::encode);

    // 4. RPC leader
    let rpc_leaders = client
        .get_slot_leaders(rpc_slot, 1)
        .await
        .map_err(|e| RpcError {
            code: 500,
            message: format!("RPC get_slot_leaders failed: {}", e),
            data: None,
        })?;
    let rpc_leader = rpc_leaders.first();
    let rpc_leader_hex = rpc_leader.map(|p| p.to_string());

    // 5. Compare
    let slots_match = precomputed_slot == rpc_slot;
    let leaders_match = precomputed_leader_hex == rpc_leader_hex;

    log::info!(
        "Verify: precomputed_slot={} rpc_slot={} match={}",
        precomputed_slot, rpc_slot, slots_match
    );
    log::info!(
        "Verify: precomputed_leader={:?} rpc_leader={:?} match={}",
        precomputed_leader_hex, rpc_leader_hex, leaders_match
    );

    // Use RPC values as authoritative for output
    let leader_hex = rpc_leader_hex.clone().unwrap_or_else(|| "unknown".to_string());
    let leader_bytes: [u8; 32] = rpc_leader
        .map(|p| p.to_bytes())
        .unwrap_or([0u8; 32]);
    let region = geo::get_region(&leader_bytes);

    Ok(Output {
        slot: rpc_slot,
        leader: leader_hex,
        leader_geo: region.geo_label().to_string(),
        closest_region: region.to_string(),
        debug: Some(DebugInfo {
            precomputed_slot,
            rpc_slot,
            slots_match,
            precomputed_leader: precomputed_leader_hex,
            rpc_leader: rpc_leader_hex,
            leaders_match,
        }),
    })
}

// Wire up the Zela procedure
zela_custom_procedure!(LeaderRouting);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_serializes() {
        let output = Output {
            slot: 12345,
            leader: "abc123".to_string(),
            leader_geo: "Europe/Frankfurt".to_string(),
            closest_region: "Frankfurt".to_string(),
            debug: None,
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("12345"));
        assert!(json.contains("Frankfurt"));
        // debug should be omitted when None
        assert!(!json.contains("debug"));
    }

    #[test]
    fn test_output_with_debug_serializes() {
        let output = Output {
            slot: 12345,
            leader: "abc123".to_string(),
            leader_geo: "Europe/Frankfurt".to_string(),
            closest_region: "Frankfurt".to_string(),
            debug: Some(DebugInfo {
                precomputed_slot: 12345,
                rpc_slot: 12345,
                slots_match: true,
                precomputed_leader: Some("abc123".to_string()),
                rpc_leader: Some("abc123".to_string()),
                leaders_match: true,
            }),
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("debug"));
        assert!(json.contains("slots_match"));
        assert!(json.contains("leaders_match"));
    }

    #[test]
    fn test_mode_deserialize() {
        let input: Input = serde_json::from_str(r#"{"mode": "precomputed"}"#).unwrap();
        assert_eq!(input.mode, Mode::Precomputed);

        let input: Input = serde_json::from_str(r#"{"mode": "rpc"}"#).unwrap();
        assert_eq!(input.mode, Mode::Rpc);

        let input: Input = serde_json::from_str(r#"{"mode": "verify"}"#).unwrap();
        assert_eq!(input.mode, Mode::Verify);

        // Default mode
        let input: Input = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(input.mode, Mode::Precomputed);
    }
}
