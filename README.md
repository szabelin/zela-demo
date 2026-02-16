# Leader Routing

**Built with modern AI tools (Claude) and validated using [llm-consensus-rs](https://github.com/szabelin/llm-consensus-rs) - a multi-LLM consensus server for code quality verification.**

---

A Zela procedure that determines the optimal server region based on the current Solana leader validator's geographic location. Minimizes transaction latency by routing to the closest edge server.

## How It Works

Three precomputed data structures compiled into the binary:

1. **Time → Slot**: Epoch metadata (rkyv zero-copy) calculates current slot from system time
2. **Slot → Validator**: PHF map (432K entries) for O(1) leader lookup
3. **Validator → Region**: PHF map (5181 validators) for O(1) geo lookup

## Execution Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| `precomputed` | PHF lookup only (default) | Production - 0ms latency |
| `rpc` | Live Solana RPC only | Baseline comparison |
| `verify` | Both + compare | Testing/validation |

## Usage

```bash
# Default (precomputed)
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"...","params":{}}' \
  https://executor.zela.io

# Verify mode - compare precomputed vs RPC
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"...","params":{"mode":"verify"}}' \
  https://executor.zela.io
```

## Response

```json
{
  "slot": 400516563,
  "leader": "abc123...",
  "leader_geo": "Europe/Frankfurt",
  "closest_region": "Frankfurt",
  "debug": {
    "precomputed_slot": 400516560,
    "rpc_slot": 400516563,
    "slot_drift": 3,
    "precomputed_leader": "abc123...",
    "rpc_leader": "abc123...",
    "leaders_match": true
  }
}
```

## Regions

| Region | Coverage | Routing |
|--------|----------|---------|
| Frankfurt | Europe, Africa, Middle East | Frankfurt |
| NewYork | Americas | NewYork |
| Tokyo | Asia Pacific | Tokyo |
| Dubai | Middle East (specific) | Dubai |
| Unknown | Fallback | Frankfurt |

## Data Refresh

Epoch data expires every ~2 days. Refresh before deployment:

```bash
# Fetch current epoch leader schedule
python scripts/fetch_schedule.py

# Geolocate all validators (~2.5 hours, rate limited)
python scripts/precompute_geo.py

# Rebuild
cargo build --release
```

## Testing

```bash
# Unit tests
cargo test

# Integration tests (validates against live Solana mainnet)
cargo test --test integration_test

# Load test (10 threads, compares rpc vs precomputed)
./scripts/load_test.sh
```

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│ System Time │ ──► │ Epoch Meta   │ ──► │ Current Slot│
└─────────────┘     │ (rkyv)       │     └──────┬──────┘
                    └──────────────┘            │
                                                ▼
                    ┌──────────────┐     ┌─────────────┐
                    │ Schedule PHF │ ◄── │ Slot Lookup │
                    │ (432K slots) │     └──────┬──────┘
                    └──────────────┘            │
                                                ▼
                    ┌──────────────┐     ┌─────────────┐
                    │ Geo PHF      │ ◄── │ Leader      │
                    │ (5181 vals)  │     └──────┬──────┘
                    └──────────────┘            │
                                                ▼
                                         ┌─────────────┐
                                         │ Region      │
                                         └─────────────┘
```

## Performance

- Zero WASM startup cost (PHF compiled-in, rkyv zero-copy)
- All lookups O(1)
- No runtime allocation in precomputed mode
- Binary size: ~17MB (mostly PHF maps)

## Benchmark Results

### Accuracy: 100% Match Rate

Precomputed leader schedule validated against live Solana mainnet RPC:

| Sample Size | Matches | Mismatches | Not Found | Match Rate |
|-------------|---------|------------|-----------|------------|
| 100 slots   | 100     | 0          | 0         | **100%**   |

The PHF-compiled schedule returns identical leaders to live Solana RPC for every slot tested.

### Throughput: 150,000x Faster Than RPC

Benchmark configuration: 10 parallel workers, 5 minutes per mode.

| Mode | What It Does | Calls/sec | Total (5 min) |
|------|--------------|-----------|---------------|
| **Leader (PHF)** | PHF(slot→leader) + PHF(leader→geo) | **8.2 million** | 2.45 billion |
| **RPC** | HTTP(getSlot) + HTTP(getSlotLeaders) + geo | 54 | 16,245 |

**Speedup: 150,000x**

### Why RPC Is Slow

RPC mode requires two HTTP round-trips to Solana mainnet per lookup:
1. `getSlot` → ~50-100ms
2. `getSlotLeaders` → ~50-100ms

Total: ~150-200ms per lookup = max ~50-60 calls/sec with 10 threads.

### Leader-Only vs Full Pipeline

| Mode | What It Does | Calls/sec |
|------|--------------|-----------|
| Leader-only | PHF(slot→leader) | 8.9 million |
| Full pipeline | PHF(slot→leader) + PHF(leader→geo) | 8.2 million |

Geo lookup adds only **8% overhead** - both PHF maps are highly optimized.

### Raw Benchmark Data

Results persisted to:
- `leader_routing/data/throughput_results.json` - Leader vs RPC throughput
- `leader_routing/data/accuracy_results.json` - Accuracy and leader-only benchmarks

## License

MIT
