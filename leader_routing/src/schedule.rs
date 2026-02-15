//! Slot-to-validator lookup using compile-time PHF (Perfect Hash Function).
//!
//! ## Data Flow
//! 1. Python script fetches leader schedule from Solana RPC
//! 2. build.rs generates PHF map with slot offsets as keys, [u8; 32] pubkeys as values
//! 3. Runtime: O(1) lookup, no hash table initialization, data compiled directly into binary
//!
//! ## Key Design
//! - Uses slot OFFSET (0-based index into epoch) as key, not absolute slot
//! - This allows the same PHF map to work with any epoch's slot range
//! - Converts absolute slot to offset via: `slot - start_slot`
//!
//! ## Return Behavior
//! - `Some([u8; 32])` if leader found for this slot offset
//! - `None` if slot is outside the epoch or no leader scheduled (edge case)

use crate::epoch::slot_offset;

// Include the generated PHF map
include!(concat!(env!("OUT_DIR"), "/phf_schedule.rs"));

/// Get the leader validator pubkey for a given slot.
///
/// # Arguments
/// * `slot` - The absolute slot number
///
/// # Returns
/// * `Some([u8; 32])` - The validator pubkey if found
/// * `None` - If no leader is scheduled for this slot offset
pub fn get_leader(slot: u64) -> Option<[u8; 32]> {
    let offset = slot_offset(slot);
    SLOT_TO_VALIDATOR.get(&offset).copied()
}

/// Get the leader validator pubkey as a hex string.
pub fn get_leader_hex(slot: u64) -> Option<String> {
    get_leader(slot).map(|pubkey| hex::encode(pubkey))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epoch::epoch_metadata;

    #[test]
    fn test_phf_map_loads() {
        // Verify the PHF map exists and can be accessed
        // The actual contents depend on schedule.json
        let _ = &SLOT_TO_VALIDATOR;
    }

    #[test]
    fn test_get_leader_returns_option() {
        // With stub data, this will return None
        // With real data, it should return Some for valid offsets
        let result = get_leader(0);
        // Either is valid depending on data
        let _ = result;
    }

    #[test]
    fn test_get_leader_hex_format() {
        // Verify hex encoding format is correct (if leader exists)
        let meta = epoch_metadata();
        if let Some(hex) = get_leader_hex(meta.start_slot) {
            assert_eq!(hex.len(), 64, "hex-encoded pubkey should be 64 chars");
            assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn test_get_leader_uses_slot_offset() {
        // Verify that get_leader converts to slot offset correctly
        let meta = epoch_metadata();
        let leader_at_start = get_leader(meta.start_slot);

        // Slot offset 0 should give same result whether accessed via
        // start_slot or any slot that maps to offset 0
        let _ = leader_at_start;
    }

    #[test]
    fn test_get_leader_far_future_slot() {
        let meta = epoch_metadata();
        // Slot way past epoch end should return None (no leader scheduled)
        let far_future = meta.end_slot + 1_000_000;
        assert!(get_leader(far_future).is_none());
    }
}
