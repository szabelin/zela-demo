# Zela Leader Routing

High-level plan for the procedure (subject to change):

1. Python script that fetches the leader schedule, packs it into a compact binary format (Borsh), plus Rust code that loads it with PHM (or equivalent) + unit tests that prove loading + lookups work correctly.

2. Implement the Zela Rust entry point (SDK integration). Focus on three binary-encoded lookups (all zero-copy post-load):
   - **Time-based Slot**: Metadata blob with `(start_time_ms: u64, slot_duration_ms: u64=400, start_slot: u64)`. Rust fn: `offset = floor((now_ms - start_ms) / 400); slot = start_slot + offset` (clamp to epoch end).
   - **Slot-to-Validator**: Inverted schedule `Vec<(u64 offset, [u8;32] pubkey)>` in Borsh → deserialize to `&'static` slice → PHF `Map<u64, &'static [u8;32]>` for O(1) hit.
   - **Validator-to-Location**: Stub for now (binary `Vec<([u8;32] pubkey, Region enum)>` in Borsh → PHF `Map<[u8;32], Region>`; fallback "UNKNOWN"/Frankfurt if miss). Defer full geo script.

3. Deploy the procedure to Zela.

4. Add integration tests that repeatedly call the procedure and compare results against real `getSlot` / `getSlotLeaders` (oracle verification).

5. Deploy integration tests to my own RPC node in Frankfurt, run for five minutes, produce a report, and add the report to this repo.

6. Discuss possible further optimizations for edge cases like offline or moved validators (covering ~3-5% cases, e.g., fallback RPC or IP-based geo resolution), but avoid premature implementation—add on need basis only.

7. Write Zela first impressions / feedback.

8. Review and push final commit.
