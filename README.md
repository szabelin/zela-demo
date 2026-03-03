# Leader Routing

Zela procedure that routes requests to the closest edge server based on the current Solana leader's geographic location.

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

## Building

```bash
# Generate geo data
python scripts/precompute_geo.py

# Build
cargo build --release

# Test
cargo test
```
