//! # Leader Routing
//!
//! A Zela procedure that determines which server region is closest to
//! the current Solana leader validator.
//!
//! ## How it works
//!
//! 1. Get current slot from Solana RPC (source of truth)
//! 2. Get leader for that slot from Solana RPC
//! 3. Look up leader's region (O(1) PHF lookup)
//! 4. Return the closest region
//!
//! ## Performance
//!
//! - Geo lookup is O(1) via compiled PHF map
//! - RPC calls are the latency bottleneck (~100-200ms)

pub mod geo;
pub mod region;

use serde::{Deserialize, Serialize};
use zela_std::{zela_custom_procedure, rpc_client::RpcClient, CustomProcedure, RpcError};

/// Zela procedure entry point.
pub struct LeaderRouting;

/// Input parameters (currently none required).
#[derive(Deserialize, Debug, Default)]
pub struct Input {}

/// Output data.
#[derive(Serialize, Debug)]
pub struct Output {
    /// Current Solana slot.
    pub slot: u64,
    /// Leader validator pubkey (base58 encoded).
    pub leader: String,
    /// Geographic location of the leader.
    pub leader_geo: String,
    /// Closest Zela region to the leader.
    pub closest_region: String,
}

impl CustomProcedure for LeaderRouting {
    type Params = Input;
    type ErrorData = ();
    type SuccessData = Output;

    async fn run(_params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        let client = RpcClient::new();

        // Get current slot from RPC (source of truth)
        let slot = client.get_slot().await.map_err(|e| RpcError {
            code: 500,
            message: format!("RPC get_slot failed: {}", e),
            data: None,
        })?;

        // Get leader for this slot
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

        let leader_b58 = leader_pubkey.to_string();
        let leader_bytes: [u8; 32] = leader_pubkey.to_bytes();
        let region = geo::get_region(&leader_bytes);

        log::info!("slot={} leader={}... region={}", slot, &leader_b58[..8], region);

        Ok(Output {
            slot,
            leader: leader_b58,
            leader_geo: region.geo_label().to_string(),
            closest_region: region.to_string(),
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
