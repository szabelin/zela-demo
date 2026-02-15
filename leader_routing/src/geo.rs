//! Validator-to-region lookup.
//!
//! Currently returns Frankfurt for all validators. This stub is needed
//! for Step 1 to provide a complete end-to-end flow.
//!
//! Possible optimization (Step 6): Add PHF lookup for validator -> region
//! based on geolocated validator IPs.

use crate::region::Region;

/// Returns true if this module is using stub data.
///
/// Used by tests to verify the implementation status.
pub const IS_STUB: bool = true;

/// Get the region for a validator pubkey.
///
/// Currently a stub that returns Frankfurt for all validators.
/// Full implementation deferred to Step 6.
///
/// # Arguments
/// * `_pubkey` - The 32-byte validator pubkey (currently unused)
///
/// # Returns
/// The region where the validator is located (currently always Frankfurt).
#[allow(unused_variables)]
pub fn get_region(pubkey: &[u8; 32]) -> Region {
    // STUB: All validators map to Frankfurt
    // This will be replaced with PHF lookup in Step 6
    Region::Frankfurt
}

/// Get the geographic label for a validator.
pub fn get_geo_label(pubkey: &[u8; 32]) -> &'static str {
    get_region(pubkey).geo_label()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_returns_frankfurt() {
        let pubkey = [0u8; 32];
        assert_eq!(get_region(&pubkey), Region::Frankfurt);
    }

    #[test]
    fn test_geo_label() {
        let pubkey = [0u8; 32];
        assert_eq!(get_geo_label(&pubkey), "Europe/Frankfurt");
    }

    #[test]
    fn test_is_stub_implementation() {
        // This test documents that geo.rs is currently a stub.
        // When Step 6 is implemented, IS_STUB should be set to false
        // and this test should be updated to verify real lookups.
        assert!(
            IS_STUB,
            "geo.rs stub flag should be true until Step 6 is implemented"
        );
    }

    #[test]
    fn test_all_pubkeys_return_same_region_in_stub() {
        // Stub returns Frankfurt for all pubkeys
        // This behavior will change in Step 6
        let pubkey1 = [0u8; 32];
        let pubkey2 = [0xff; 32];
        let mut pubkey3 = [0u8; 32];
        pubkey3[0] = 0x12;
        pubkey3[31] = 0x34;

        assert_eq!(get_region(&pubkey1), Region::Frankfurt);
        assert_eq!(get_region(&pubkey2), Region::Frankfurt);
        assert_eq!(get_region(&pubkey3), Region::Frankfurt);
    }
}
