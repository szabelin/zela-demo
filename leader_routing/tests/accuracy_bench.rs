//! Accuracy and leader-only throughput benchmark
//!
//! Run with: cargo test --release --test accuracy_bench -- --nocapture --ignored

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ACCURACY_SAMPLE_SIZE: usize = 100;
const LEADER_BENCH_DURATION_SECS: u64 = 300; // 5 minutes
const NUM_WORKERS: usize = 10;

#[test]
#[ignore]
fn accuracy_and_leader_bench() {
    println!("\n=== Accuracy & Leader-Only Benchmark ===\n");

    // Part 1: Accuracy test (100 slots)
    let accuracy = run_accuracy_test();

    // Part 2: Leader-only throughput (no geo lookup)
    let leader_only = run_leader_only_bench();

    // Part 3: Full pipeline throughput (with geo lookup)
    let full_pipeline = run_full_pipeline_bench();

    // Save results
    let results = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "accuracy": {
            "sample_size": ACCURACY_SAMPLE_SIZE,
            "matches": accuracy.0,
            "mismatches": accuracy.1,
            "not_found": accuracy.2,
            "match_rate_percent": accuracy.3
        },
        "leader_only": {
            "duration_secs": LEADER_BENCH_DURATION_SECS,
            "workers": NUM_WORKERS,
            "total_calls": leader_only.0,
            "throughput_per_sec": leader_only.1
        },
        "full_pipeline": {
            "duration_secs": LEADER_BENCH_DURATION_SECS,
            "workers": NUM_WORKERS,
            "total_calls": full_pipeline.0,
            "throughput_per_sec": full_pipeline.1
        },
        "geo_overhead_percent": ((leader_only.1 - full_pipeline.1) / leader_only.1 * 100.0)
    });

    std::fs::write(
        "data/accuracy_results.json",
        serde_json::to_string_pretty(&results).unwrap()
    ).expect("Failed to save results");

    println!("\n{:=<60}", "");
    println!("Results saved to data/accuracy_results.json");
}

fn run_accuracy_test() -> (usize, usize, usize, f64) {
    use leader_routing::schedule;

    println!("=== Accuracy Test ({} slots) ===\n", ACCURACY_SAMPLE_SIZE);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    // Get current slot from RPC
    let rpc_slot = rpc_get_slot(&client).expect("Failed to get slot");
    println!("RPC current slot: {}", rpc_slot);

    // Get leaders from RPC
    let rpc_leaders = rpc_get_leaders(&client, rpc_slot, ACCURACY_SAMPLE_SIZE as u64)
        .expect("Failed to get leaders");

    println!("Comparing {} slots starting at {}...\n", ACCURACY_SAMPLE_SIZE, rpc_slot);

    let mut matches = 0;
    let mut mismatches = 0;
    let mut not_found = 0;

    for (i, rpc_leader) in rpc_leaders.iter().enumerate() {
        let slot = rpc_slot + i as u64;
        let precomputed = schedule::get_leader(slot);

        match precomputed {
            Some(leader_bytes) => {
                let precomputed_hex = hex::encode(leader_bytes);
                let rpc_bytes = bs58::decode(rpc_leader).into_vec().unwrap_or_default();
                let rpc_hex = hex::encode(&rpc_bytes);

                if precomputed_hex == rpc_hex {
                    matches += 1;
                } else {
                    mismatches += 1;
                    println!("  MISMATCH at slot {}", slot);
                }
            }
            None => {
                not_found += 1;
                println!("  NOT FOUND at slot {}", slot);
            }
        }
    }

    let match_rate = matches as f64 / ACCURACY_SAMPLE_SIZE as f64 * 100.0;

    println!("Results: {} matches, {} mismatches, {} not found", matches, mismatches, not_found);
    println!("Match rate: {:.1}%\n", match_rate);

    (matches, mismatches, not_found, match_rate)
}

fn run_leader_only_bench() -> (u64, f64) {
    use leader_routing::{epoch, schedule};

    println!("=== Leader-Only Benchmark (no geo) ===");
    println!("Duration: {}s, Workers: {}\n", LEADER_BENCH_DURATION_SECS, NUM_WORKERS);

    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let duration = Duration::from_secs(LEADER_BENCH_DURATION_SECS);

    let handles: Vec<_> = (0..NUM_WORKERS)
        .map(|_| {
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                while start.elapsed() < duration {
                    let slot = epoch::current_slot();
                    let _ = schedule::get_leader(slot);
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    // Progress
    while start.elapsed() < duration {
        std::thread::sleep(Duration::from_secs(60));
        let c = counter.load(Ordering::Relaxed);
        println!("  Progress: {} calls ({:.0}/sec)", c, c as f64 / start.elapsed().as_secs_f64());
    }

    for h in handles { h.join().unwrap(); }

    let total = counter.load(Ordering::Relaxed);
    let throughput = total as f64 / start.elapsed().as_secs_f64();
    println!("  Completed: {} calls ({:.0}/sec)\n", total, throughput);

    (total, throughput)
}

fn run_full_pipeline_bench() -> (u64, f64) {
    use leader_routing::{epoch, schedule, geo};

    println!("=== Full Pipeline Benchmark (with geo) ===");
    println!("Duration: {}s, Workers: {}\n", LEADER_BENCH_DURATION_SECS, NUM_WORKERS);

    let counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let duration = Duration::from_secs(LEADER_BENCH_DURATION_SECS);

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
        std::thread::sleep(Duration::from_secs(60));
        let c = counter.load(Ordering::Relaxed);
        println!("  Progress: {} calls ({:.0}/sec)", c, c as f64 / start.elapsed().as_secs_f64());
    }

    for h in handles { h.join().unwrap(); }

    let total = counter.load(Ordering::Relaxed);
    let throughput = total as f64 / start.elapsed().as_secs_f64();
    println!("  Completed: {} calls ({:.0}/sec)\n", total, throughput);

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

fn rpc_get_leaders(client: &reqwest::blocking::Client, slot: u64, count: u64) -> Result<Vec<String>, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getSlotLeaders", "params": [slot, count]
    });
    let resp: serde_json::Value = client
        .post("https://api.mainnet-beta.solana.com")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;

    let arr = resp["result"].as_array().ok_or("No result array".to_string())?;
    let mut leaders = Vec::new();
    for v in arr {
        let s = v.as_str().ok_or("Invalid leader".to_string())?;
        leaders.push(s.to_string());
    }
    Ok(leaders)
}
