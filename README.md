# Leader Routing

A Zela procedure that determines the optimal server region based on the current Solana leader validator's geographic location. Minimizes transaction latency by routing to the closest edge server.

## How It Works

1. **Get current slot** from Solana RPC (source of truth)
2. **Get leader** for that slot from Solana RPC
3. **Geo lookup** via compiled PHF map (O(1), 5181 validators)
4. **Return** closest region

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ Solana RPC  │ ──► │ getSlot     │ ──► │ Current Slot│
│ (mainnet)   │     │ getLeaders  │     │ + Leader    │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                    ┌──────────────┐     ┌─────▼───────┐
                    │ Geo PHF      │ ◄── │ Leader      │
                    │ (5181 vals)  │     │ Pubkey      │
                    └──────────────┘     └──────┬──────┘
                                               │
                                         ┌─────▼───────┐
                                         │ Region      │
                                         └─────────────┘
```

## Usage

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"...","params":{}}' \
  https://executor.zela.io
```

## Response

```json
{
  "slot": 401344090,
  "leader": "DRpbCBMxVnDK7maPGv4USk3L6K1cFkB2U33Dbzhx1Fgq",
  "leader_geo": "Europe/Frankfurt",
  "closest_region": "Frankfurt"
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

## Geo Data Refresh

Validator geo data should be refreshed periodically to capture new validators:

```bash
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
cargo test --test integration_test -- --nocapture
```

## Performance

- Geo lookup is O(1) via compiled PHF map
- RPC calls are the latency bottleneck (~100-200ms total)
- Binary size: ~1.5MB (geo PHF map only)

## License

MIT
