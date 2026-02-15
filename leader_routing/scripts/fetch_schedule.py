#!/usr/bin/env python3
"""
Fetch Solana leader schedule and epoch metadata for the Zela leader routing procedure.

Usage:
    python scripts/fetch_schedule.py [RPC_URL]

Outputs:
    data/schedule.json - Leader schedule and epoch metadata for build.rs
"""

import json
import sys
import time
import logging
import base58
import requests
from typing import Dict, List, Any

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s: %(message)s",
    datefmt="%H:%M:%S"
)
log = logging.getLogger(__name__)


def fetch_with_retry(rpc_url: str, method: str, params: List[Any], retries: int = 3) -> Any:
    """Fetch from Solana RPC with retry and exponential backoff.

    Handles network errors, timeouts, and RPC-level errors with detailed logging.
    Uses exponential backoff: 1s, 2s, 4s between retries.

    Args:
        rpc_url: Solana RPC endpoint URL
        method: RPC method name (e.g., "getEpochInfo", "getLeaderSchedule")
        params: Method parameters (usually empty list)
        retries: Maximum retry attempts (default: 3)

    Returns:
        The "result" field from the JSON-RPC response

    Raises:
        Exception: After all retries exhausted, with context about failures
    """
    last_error = None
    for attempt in range(retries):
        try:
            log.debug(f"RPC call: {method} (attempt {attempt + 1}/{retries})")
            resp = requests.post(
                rpc_url,
                json={"jsonrpc": "2.0", "id": 1, "method": method, "params": params},
                timeout=30
            )
            resp.raise_for_status()
            result = resp.json()

            if "error" in result:
                error_msg = result['error']
                log.error(f"RPC error response: {error_msg}")
                raise Exception(f"RPC error: {error_msg}")

            log.debug(f"RPC call {method} succeeded")
            return result["result"]

        except requests.exceptions.Timeout as e:
            last_error = e
            log.warning(f"Attempt {attempt + 1}/{retries} timed out after 30s: {e}")
        except requests.exceptions.ConnectionError as e:
            last_error = e
            log.warning(f"Attempt {attempt + 1}/{retries} connection failed: {e}")
        except requests.exceptions.HTTPError as e:
            last_error = e
            log.warning(f"Attempt {attempt + 1}/{retries} HTTP error: {e}")
        except Exception as e:
            last_error = e
            log.warning(f"Attempt {attempt + 1}/{retries} failed: {type(e).__name__}: {e}")

        if attempt < retries - 1:
            wait_time = 2 ** attempt
            log.info(f"Retrying in {wait_time}s...")
            time.sleep(wait_time)

    error_type = type(last_error).__name__ if last_error else "Unknown"
    error_msg = str(last_error) if last_error else "No error details"
    log.error(f"All {retries} attempts failed for {method}. Last error: {error_type}: {error_msg}")
    raise last_error or Exception(f"Failed to call {method} after {retries} attempts")


def pubkey_to_bytes(pubkey_b58: str) -> List[int]:
    """Convert base58 pubkey to list of bytes.

    Returns empty list if decoding fails.
    """
    try:
        decoded = base58.b58decode(pubkey_b58)
        if len(decoded) != 32:
            log.warning(f"Pubkey {pubkey_b58[:8]}... has invalid length: {len(decoded)}")
            return []
        return list(decoded)
    except Exception as e:
        log.warning(f"Failed to decode pubkey {pubkey_b58[:8]}...: {e}")
        return []


def main():
    rpc_url = sys.argv[1] if len(sys.argv) > 1 else "https://api.mainnet-beta.solana.com"
    log.info(f"Using RPC: {rpc_url}")

    # Fetch epoch info
    log.info("Fetching epoch info...")
    epoch_info = fetch_with_retry(rpc_url, "getEpochInfo", [])

    current_slot = epoch_info["absoluteSlot"]
    slot_index = epoch_info["slotIndex"]
    slots_in_epoch = epoch_info["slotsInEpoch"]

    start_slot = current_slot - slot_index
    end_slot = start_slot + slots_in_epoch - 1

    log.info(f"Epoch: start_slot={start_slot}, end_slot={end_slot}, slots={slots_in_epoch}")

    # Fetch leader schedule
    log.info("Fetching leader schedule...")
    leader_schedule = fetch_with_retry(rpc_url, "getLeaderSchedule", [])

    if leader_schedule is None:
        log.error("Leader schedule not available")
        sys.exit(1)

    # Invert: pubkey -> [slot_offsets] to [(slot_offset, pubkey_bytes)]
    log.info(f"Processing {len(leader_schedule)} validators...")
    entries: List[List[Any]] = []

    for pubkey_b58, slot_offsets in leader_schedule.items():
        pubkey_bytes = pubkey_to_bytes(pubkey_b58)
        if len(pubkey_bytes) != 32:
            log.warning(f"Skipping invalid pubkey {pubkey_b58}: length {len(pubkey_bytes)}")
            continue

        for slot_offset in slot_offsets:
            entries.append([slot_offset, pubkey_bytes])

    # Sort by slot offset
    entries.sort(key=lambda x: x[0])

    log.info(f"Total entries: {len(entries)}")

    # Get approximate epoch start time
    # Note: This is an approximation. For precise timing, use getBlockTime on start_slot
    # For now, we use current time minus elapsed slots * 400ms
    current_time_ms = int(time.time() * 1000)
    elapsed_ms = slot_index * 400
    start_time_ms = current_time_ms - elapsed_ms

    output = {
        "metadata": {
            "start_time_ms": start_time_ms,
            "slot_duration_ms": 400,
            "start_slot": start_slot,
            "end_slot": end_slot,
        },
        "entries": entries
    }

    # Write output
    output_path = "data/schedule.json"
    with open(output_path, "w") as f:
        json.dump(output, f)

    file_size = len(json.dumps(output))
    log.info(f"Wrote {output_path} ({file_size / 1024:.1f} KB, {len(entries)} entries)")


if __name__ == "__main__":
    main()
