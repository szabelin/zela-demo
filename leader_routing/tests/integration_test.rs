use std::time::Duration;
use serde::Deserialize;

const RPC_URL: &str = "https://api.mainnet-beta.solana.com";

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
}

fn rpc_call<T: for<'de> Deserialize<'de>>(method: &str, params: serde_json::Value) -> Result<T, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });

    let response: RpcResponse<T> = client
        .post(RPC_URL)
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;

    response.result.ok_or("No result".to_string())
}

#[test]
fn test_leader_geo_lookup() {
    let slot: u64 = rpc_call("getSlot", serde_json::json!([])).unwrap();
    let leaders: Vec<String> = rpc_call("getSlotLeaders", serde_json::json!([slot, 1])).unwrap();

    let leader_bytes: [u8; 32] = bs58::decode(&leaders[0])
        .into_vec()
        .unwrap()
        .try_into()
        .unwrap();

    let region = leader_routing::get_region(&leader_bytes);
    let dest = region.routing_destination().to_string();

    assert!(["Frankfurt", "NewYork", "Tokyo", "Dubai"].contains(&dest.as_str()));
}

#[test]
fn test_geo_coverage() {
    let slot: u64 = rpc_call("getSlot", serde_json::json!([])).unwrap();
    let leaders: Vec<String> = rpc_call("getSlotLeaders", serde_json::json!([slot, 100])).unwrap();

    let mut known = 0;
    let mut total = 0;

    for leader_b58 in &leaders {
        let bytes: [u8; 32] = match bs58::decode(leader_b58).into_vec() {
            Ok(b) if b.len() == 32 => b.try_into().unwrap(),
            _ => continue,
        };

        if leader_routing::get_region(&bytes).to_string() != "Unknown" {
            known += 1;
        }
        total += 1;
    }

    let coverage = known as f64 / total as f64;
    assert!(coverage >= 0.8, "Coverage {:.1}% below 80%", coverage * 100.0);
}
