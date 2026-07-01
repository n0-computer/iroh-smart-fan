use std::io::Read;

/// Bake a `len`-byte hex secret into the firmware under `name`. An explicit `name`
/// env var wins; otherwise a random secret is generated at build time and cached in
/// OUT_DIR (regenerated on `cargo clean` or if the env var changes). Either way it's
/// embedded, so it's STABLE across reboots. main.rs reads it back at compile time
/// via `env!("<name>")` / `option_env!("<name>")`.
fn baked_secret(name: &str, cache_file: &str, len: usize) {
    println!("cargo:rerun-if-env-changed={name}");
    let secret = std::env::var(name).unwrap_or_else(|_| {
        let cache = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join(cache_file);
        if let Ok(existing) = std::fs::read_to_string(&cache) {
            return existing;
        }
        let mut bytes = vec![0u8; len];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .unwrap_or_else(|e| panic!("generating {name}: read /dev/urandom: {e}"));
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        std::fs::write(&cache, &hex).unwrap_or_else(|e| panic!("caching {name}: {e}"));
        hex
    });
    println!("cargo:rustc-env={name}={secret}");
}

fn main() {
    // iroh node identity (32-byte key) — stable endpoint ID / ticket across reboots.
    baked_secret("IROH_SECRET", "iroh_secret.hex", 32);
    // Shared secret for authenticating fan-control API calls — same baking mechanism,
    // 8 bytes (16 hex chars): short enough to paste into the GUI, plenty for this.
    baked_secret("FAN_API_SECRET", "fan_api_secret.hex", 8);

    embuild::espidf::sysenv::output();
}
