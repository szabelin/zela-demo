mod region;

use region::Region;
use serde::{Deserialize, Serialize};
use zela_std::{zela_custom_procedure, rpc_client::RpcClient, CustomProcedure, RpcError};

include!(concat!(env!("OUT_DIR"), "/phf_geo.rs"));

pub fn get_region(pubkey: &[u8; 32]) -> Region {
    VALIDATOR_TO_REGION.get(pubkey).map(|&c| Region::from(c)).unwrap_or(Region::Unknown)
}

pub struct LeaderRouting;

#[derive(Deserialize, Debug, Default)]
pub struct Input {}

#[derive(Serialize, Debug)]
pub struct Output {
    pub slot: u64,
    pub leader: String,
    pub leader_geo: String,
    pub closest_region: String,
}

impl CustomProcedure for LeaderRouting {
    type Params = Input;
    type ErrorData = ();
    type SuccessData = Output;

    async fn run(_params: Self::Params) -> Result<Self::SuccessData, RpcError<Self::ErrorData>> {
        let client = RpcClient::new();

        let slot = client.get_slot().await.map_err(|e| RpcError {
            code: 500,
            message: format!("get_slot failed: {}", e),
            data: None,
        })?;

        let leaders = client.get_slot_leaders(slot, 1).await.map_err(|e| RpcError {
            code: 500,
            message: format!("get_slot_leaders failed: {}", e),
            data: None,
        })?;

        let leader_pubkey = leaders.first().ok_or_else(|| RpcError {
            code: 404,
            message: format!("No leader for slot {}", slot),
            data: None,
        })?;

        let leader_b58 = leader_pubkey.to_string();
        let region = get_region(&leader_pubkey.to_bytes());

        Ok(Output {
            slot,
            leader: leader_b58,
            leader_geo: region.geo_label().to_string(),
            closest_region: region.routing_destination().to_string(),
        })
    }

    const LOG_MAX_LEVEL: log::LevelFilter = log::LevelFilter::Debug;
}

zela_custom_procedure!(LeaderRouting);
