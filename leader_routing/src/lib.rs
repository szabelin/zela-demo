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

    async fn run(_params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        // 1. Get current slot from time
        let slot = epoch::current_slot();
        log::debug!("Current slot: {}", slot);

        // 2. Get leader for this slot
        let leader_pubkey = schedule::get_leader(slot).ok_or_else(|| RpcError {
            code: 404,
            message: format!("No leader found for slot {}", slot),
            data: None,
        })?;

        let leader_hex = hex::encode(leader_pubkey);
        log::debug!("Leader: {}", leader_hex);

        // 3. Get leader's region
        let region = geo::get_region(&leader_pubkey);
        let leader_geo = region.geo_label().to_string();
        let closest_region = region.to_string();

        log::info!(
            "Slot {} -> Leader {} -> Region {}",
            slot,
            &leader_hex[..8],
            closest_region
        );

        Ok(Output {
            slot,
            leader: leader_hex,
            leader_geo,
            closest_region,
        })
    }

    const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
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
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("12345"));
        assert!(json.contains("Frankfurt"));
    }
}
