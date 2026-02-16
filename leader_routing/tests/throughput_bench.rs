//! Throughput benchmark: Precomputed vs RPC mode
//!
//! Run with: cargo test --release --test throughput_bench -- --nocapture --ignored

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DURATION_SECS: u64 = 300; // 5 minutes
const NUM_WORKERS: usize = 10;

#[test]
#[ignore] // Run explicitly with --ignored flag
fn throughput_benchmark() {
    println!("\n=== Throughput Benchmark: Precomputed vs RPC ===\n");
    println!("Workers: {}", NUM_WORKERS);
    println!("Duration: {} seconds per mode\n", DURATION_SECS);

    // Run precomputed mode
    let precomputed = run_precomputed_bench();

    // Run RPC mode
    let rpc = run_rpc_bench();

    // Results
    println!("\n{:=<60}", "");
    println!("FINAL RESULTS");
    println!("{:=<60}\n", "");

    println!("PRECOMPUTED MODE:");
    println!("  Total calls:    {}", precomputed.0);
    println!("  Throughput:     {:.0} calls/sec", precomputed.1);

    println!("\nRPC MODE:");
    println!("  Total calls:    {}", rpc.0);
    println!("  Throughput:     {:.2} calls/sec", rpc.1);

    let speedup = precomputed.1 / rpc.1.max(0.001);
    println!("\nSPEEDUP: {:.0}x faster with precomputed mode", speedup);

    // Save results
    let results = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "duration_secs": DURATION_SECS,
        "workers": NUM_WORKERS,
        "precomputed": { "calls": precomputed.0, "throughput": precomputed.1 },
        "rpc": { "calls": rpc.0, "throughput": rpc.1 },
        "speedup": speedup
    });

    std::fs::write(
        "data/throughput_results.json",
        serde_json::to_string_pretty(&results).unwrap()
    ).expect("Failed to save results");

    println!("\nResults saved to data/throughput_results.json");
}

fn run_precomputed_bench() -> (u64, f64) {
    use leader_routing::{epoch, schedule, geo};

    println!("Running PRECOMPUTED mode for {}s with {} workers...", DURATION_SECS, NUM_WORKERS);

    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let duration = Duration::from_secs(DURATION_SECS);

    let handles: Vec<_> = (0..NUM_WORKERS)
        .map(|_| {
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                while start.elapsed() < duration {
                    let slot = epoch::current_slot();
                    if let Some(leader) = schedule::get_leader(slot) {
                        let _ = geo::get_region(&leader);
                    }
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    // Progress
    while start.elapsed() < duration {
        std::thread::sleep(Duration::from_secs(30));
        let c = counter.load(Ordering::Relaxed);
        println!("  Progress: {} calls ({:.0}/sec)", c, c as f64 / start.elapsed().as_secs_f64());
    }

    for h in handles { h.join().unwrap(); }

    let total = counter.load(Ordering::Relaxed);
    let throughput = total as f64 / start.elapsed().as_secs_f64();
    println!("  Completed: {} calls ({:.0}/sec)", total, throughput);

    (total, throughput)
}

fn run_rpc_bench() -> (u64, f64) {
    use leader_routing::geo;

    println!("\nRunning RPC mode for {}s with {} workers...", DURATION_SECS, NUM_WORKERS);

    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let duration = Duration::from_secs(DURATION_SECS);

    let handles: Vec<_> = (0..NUM_WORKERS)
        .map(|_| {
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .unwrap();

                while start.elapsed() < duration {
                    if let Ok(slot) = rpc_get_slot(&client) {
                        if let Ok(leader) = rpc_get_leader(&client, slot) {
                            let _ = geo::get_region(&leader);
                        }
                    }
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    // Progress
    while start.elapsed() < duration {
        std::thread::sleep(Duration::from_secs(30));
        let c = counter.load(Ordering::Relaxed);
        println!("  Progress: {} calls ({:.2}/sec)", c, c as f64 / start.elapsed().as_secs_f64());
    }

    for h in handles { h.join().unwrap(); }

    let total = counter.load(Ordering::Relaxed);
    let throughput = total as f64 / start.elapsed().as_secs_f64();
    println!("  Completed: {} calls ({:.2}/sec)", total, throughput);

    (total, throughput)
}

fn rpc_get_slot(client: &reqwest::blocking::Client) -> Result<u64, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getSlot", "params": []
    });
    let resp: serde_json::Value = client
        .post("https://api.mainnet-beta.solana.com")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    resp["result"].as_u64().ok_or("No result".into())
}

fn rpc_get_leader(client: &reqwest::blocking::Client, slot: u64) -> Result<[u8; 32], String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getSlotLeaders", "params": [slot, 1]
    });
    let resp: serde_json::Value = client
        .post("https://api.mainnet-beta.solana.com")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let b58 = resp["result"][0].as_str().ok_or("No leader")?;
    let bytes = bs58::decode(b58).into_vec().map_err(|e| e.to_string())?;
    bytes.try_into().map_err(|_| "Bad length".into())
}
