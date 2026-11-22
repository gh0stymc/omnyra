//! Bakes the OAuth client IDs from the repo-root `.env` into the binary (`option_env!`).
//! Variables already present in the environment (e.g. CI secrets) take precedence.
//! Values are never printed except as cargo directives.

const KEYS: &[&str] = &[
    "OMNYRA_GOOGLE_CLIENT_ID",
    "OMNYRA_GOOGLE_CLIENT_SECRET",
    "OMNYRA_TWITCH_CLIENT_ID",
    "OMNYRA_INSTAGRAM_CLIENT_ID",
    "OMNYRA_TIKTOK_CLIENT_KEY",
    "OMNYRA_BROKER_URL",
];

fn main() {
    println!("cargo:rerun-if-changed=../.env");
    for key in KEYS {
        println!("cargo:rerun-if-env-changed={key}");
    }
    if let Ok(text) = std::fs::read_to_string("../.env") {
        for line in text.lines() {
            let Some((k, v)) = line.trim().split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            if KEYS.contains(&k) && std::env::var_os(k).is_none() && !v.is_empty() {
                println!("cargo:rustc-env={k}={v}");
            }
        }
    }
    tauri_build::build()
}
