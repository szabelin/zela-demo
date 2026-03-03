use std::collections::HashMap;
use std::io::Write;
use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=data/leader_geo.json");

    let geo_path = "data/leader_geo.json";

    if !Path::new(geo_path).exists() {
        if env::var("LEADER_ROUTING_REQUIRE_DATA").is_ok() {
            panic!("Missing data/leader_geo.json (required in CI mode)");
        }
        create_stub();
        return;
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let geo_json = fs::read_to_string(geo_path).unwrap();
    let geo_map: HashMap<String, String> = serde_json::from_str(&geo_json).unwrap();

    generate_phf(&geo_map, Path::new(&out_dir));
}

fn region_to_u8(region: &str) -> u8 {
    match region {
        "Frankfurt" => 0,
        "Dubai" => 1,
        "NewYork" => 2,
        "Tokyo" => 3,
        _ => 4,
    }
}

fn generate_phf(geo_map: &HashMap<String, String>, out_path: &Path) {
    let mut entries = Vec::new();

    for (pubkey_b58, region) in geo_map {
        let pubkey_bytes = match bs58::decode(pubkey_b58).into_vec() {
            Ok(bytes) if bytes.len() == 32 => bytes,
            _ => continue,
        };

        let key_literal = format!(
            "[{}]",
            pubkey_bytes.iter().map(|b| format!("0x{:02x}", b)).collect::<Vec<_>>().join(", ")
        );

        entries.push((key_literal, region_to_u8(region)));
    }

    let mut file = fs::File::create(out_path.join("phf_geo.rs")).unwrap();
    writeln!(file, "pub static VALIDATOR_TO_REGION: phf::Map<[u8; 32], u8> = phf::phf_map! {{").unwrap();
    for (key, value) in &entries {
        writeln!(file, "    {} => {}u8,", key, value).unwrap();
    }
    writeln!(file, "}};").unwrap();

    println!("cargo:warning=Generated PHF map: {} validators", entries.len());
}

fn create_stub() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let mut file = fs::File::create(Path::new(&out_dir).join("phf_geo.rs")).unwrap();
    writeln!(file, "pub static VALIDATOR_TO_REGION: phf::Map<[u8; 32], u8> = phf::phf_map! {{}};").unwrap();
    println!("cargo:warning=Using stub data");
}
