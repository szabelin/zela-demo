#!/usr/bin/env python3
"""
Precompute validator geographic locations for leader routing.

Fetches validator IPs from Solana getClusterNodes, geolocates via ip-api.com,
and maps to Zela regions.

Usage:
    python scripts/precompute_geo.py [RPC_URL] [OUTPUT_PATH]

Arguments:
    RPC_URL     - Solana RPC endpoint (default: mainnet)
    OUTPUT_PATH - Output JSON file (default: data/leader_geo.json)

Outputs:
    data/leader_geo.json - Validator pubkey -> region mapping
"""

import json
import os
import sys
import time
import logging
import requests
from typing import Dict, Optional

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s: %(message)s",
    datefmt="%H:%M:%S"
)
log = logging.getLogger(__name__)

# ip-api.com rate limit: 45 requests per minute
IP_API_RATE_LIMIT = 45
IP_API_DELAY = 60.0 / IP_API_RATE_LIMIT  # ~1.33 seconds between requests

# Country code to Zela region mapping
COUNTRY_TO_REGION: Dict[str, str] = {
    # North/South America -> NewYork
    "US": "NewYork", "CA": "NewYork", "MX": "NewYork",
    "BR": "NewYork", "AR": "NewYork", "CL": "NewYork",
    "CO": "NewYork", "PE": "NewYork", "VE": "NewYork",

    # Europe -> Frankfurt
    "DE": "Frankfurt", "FR": "Frankfurt", "GB": "Frankfurt",
    "NL": "Frankfurt", "BE": "Frankfurt", "CH": "Frankfurt",
    "AT": "Frankfurt", "PL": "Frankfurt", "CZ": "Frankfurt",
    "SE": "Frankfurt", "NO": "Frankfurt", "DK": "Frankfurt",
    "FI": "Frankfurt", "IE": "Frankfurt", "PT": "Frankfurt",
    "ES": "Frankfurt", "IT": "Frankfurt", "GR": "Frankfurt",
    "RO": "Frankfurt", "HU": "Frankfurt", "BG": "Frankfurt",
    "UA": "Frankfurt", "RU": "Frankfurt",  # Western Russia

    # Middle East, Africa, India -> Dubai
    "AE": "Dubai", "SA": "Dubai", "QA": "Dubai",
    "KW": "Dubai", "BH": "Dubai", "OM": "Dubai",
    "IL": "Dubai", "TR": "Dubai", "EG": "Dubai",
    "ZA": "Dubai", "NG": "Dubai", "KE": "Dubai",
    "IN": "Dubai", "PK": "Dubai", "BD": "Dubai",

    # East/SE Asia, Oceania -> Tokyo
    "JP": "Tokyo", "KR": "Tokyo", "CN": "Tokyo",
    "HK": "Tokyo", "TW": "Tokyo", "SG": "Tokyo",
    "MY": "Tokyo", "TH": "Tokyo", "VN": "Tokyo",
    "PH": "Tokyo", "ID": "Tokyo", "AU": "Tokyo",
    "NZ": "Tokyo",
}


def fetch_cluster_nodes(rpc_url: str) -> list:
    """Fetch validator nodes from Solana RPC."""
    log.info("Fetching cluster nodes...")

    resp = requests.post(
        rpc_url,
        json={"jsonrpc": "2.0", "id": 1, "method": "getClusterNodes", "params": []},
        timeout=30
    )
    resp.raise_for_status()
    result = resp.json()

    if "error" in result:
        raise Exception(f"RPC error: {result['error']}")

    nodes = result["result"]
    log.info(f"Found {len(nodes)} cluster nodes")
    return nodes


def is_private_ip(ip: str) -> bool:
    """Check if IP is in private/local range."""
    # 10.0.0.0/8
    if ip.startswith("10."):
        return True
    # 172.16.0.0/12 (172.16.x.x - 172.31.x.x)
    if ip.startswith("172."):
        parts = ip.split(".")
        if len(parts) >= 2:
            second_octet = int(parts[1])
            if 16 <= second_octet <= 31:
                return True
    # 192.168.0.0/16
    if ip.startswith("192.168."):
        return True
    # 127.0.0.0/8 (loopback)
    if ip.startswith("127."):
        return True
    return False


def extract_ip(gossip_addr: str) -> Optional[str]:
    """Extract IP from gossip address (ip:port format)."""
    if not gossip_addr:
        return None
    try:
        ip = gossip_addr.rsplit(":", 1)[0]
        if is_private_ip(ip):
            return None
        return ip
    except Exception:
        return None


# Cache for geolocation results (IP -> country code)
# Note: In-memory cache is sufficient since this script runs once per epoch (~2-3 days).
# For persistent caching across runs, consider saving _geo_cache to a JSON file.
_geo_cache: Dict[str, Optional[str]] = {}


def geolocate_ip(ip: str, retry_count: int = 0) -> Optional[str]:
    """Geolocate IP using ip-api.com. Returns country code or None.

    Results are cached to avoid redundant API calls.
    Handles rate limit responses from ip-api.com with max 3 retries.
    """
    MAX_RETRIES = 3

    # Check cache first
    if ip in _geo_cache:
        return _geo_cache[ip]

    try:
        resp = requests.get(
            f"http://ip-api.com/json/{ip}?fields=status,countryCode,message",
            timeout=10
        )
        resp.raise_for_status()
        data = resp.json()

        # Handle rate limit response
        if data.get("status") == "fail" and "rate limit" in data.get("message", "").lower():
            if retry_count >= MAX_RETRIES:
                log.error(f"Max retries ({MAX_RETRIES}) exceeded for rate limiting")
                _geo_cache[ip] = None
                return None
            log.warning(f"Rate limited by ip-api.com, waiting 60 seconds... (retry {retry_count + 1}/{MAX_RETRIES})")
            time.sleep(60)
            return geolocate_ip(ip, retry_count + 1)

        if data.get("status") == "success":
            country = data.get("countryCode")
            _geo_cache[ip] = country
            return country
        _geo_cache[ip] = None
        return None
    except Exception as e:
        log.warning(f"Geolocation failed for {ip}: {e}")
        _geo_cache[ip] = None
        return None


def country_to_region(country_code: Optional[str]) -> str:
    """Map country code to Zela region. Returns 'Unknown' if not mapped."""
    if not country_code:
        return "Unknown"
    return COUNTRY_TO_REGION.get(country_code, "Unknown")


def main():
    rpc_url = sys.argv[1] if len(sys.argv) > 1 else "https://api.mainnet-beta.solana.com"
    output_path = sys.argv[2] if len(sys.argv) > 2 else "data/leader_geo.json"

    log.info(f"Using RPC: {rpc_url}")
    log.info(f"Output: {output_path}")

    # Validate output path is writable
    output_dir = os.path.dirname(output_path) or "."
    if not os.access(output_dir, os.W_OK) and os.path.exists(output_dir):
        log.error(f"Output directory not writable: {output_dir}")
        sys.exit(1)

    # Fetch cluster nodes
    nodes = fetch_cluster_nodes(rpc_url)

    # Process each validator
    geo_map: Dict[str, str] = {}
    stats = {"success": 0, "failed": 0, "skipped": 0}

    for i, node in enumerate(nodes):
        pubkey = node.get("pubkey")
        gossip = node.get("gossip")

        if not pubkey:
            stats["skipped"] += 1
            continue

        ip = extract_ip(gossip)
        if not ip:
            log.debug(f"No valid IP for {pubkey[:8]}...")
            geo_map[pubkey] = "Unknown"
            stats["skipped"] += 1
            continue

        # Rate limit for ip-api.com
        if i > 0 and stats["success"] > 0:
            time.sleep(IP_API_DELAY)

        # Geolocate
        country = geolocate_ip(ip)
        region = country_to_region(country)
        geo_map[pubkey] = region

        if region != "Unknown":
            stats["success"] += 1
            log.debug(f"{pubkey[:8]}... -> {ip} -> {country} -> {region}")
        else:
            stats["failed"] += 1
            log.debug(f"{pubkey[:8]}... -> {ip} -> Unknown")

        # Progress log every 50 validators
        if (i + 1) % 50 == 0:
            log.info(f"Progress: {i + 1}/{len(nodes)} validators processed")

    # Summary
    log.info(f"Completed: {stats['success']} geolocated, {stats['failed']} unknown, {stats['skipped']} skipped")

    # Region distribution
    region_counts: Dict[str, int] = {}
    for region in geo_map.values():
        region_counts[region] = region_counts.get(region, 0) + 1
    log.info(f"Region distribution: {region_counts}")

    # Ensure output directory exists
    output_dir = os.path.dirname(output_path)
    if output_dir:
        os.makedirs(output_dir, exist_ok=True)

    # Write output
    with open(output_path, "w") as f:
        json.dump(geo_map, f)

    file_size = len(json.dumps(geo_map))
    log.info(f"Wrote {output_path} ({file_size / 1024:.1f} KB, {len(geo_map)} entries)")


if __name__ == "__main__":
    main()
