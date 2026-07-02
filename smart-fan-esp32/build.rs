/// Pass an explicit `name` env var through to the compiled crate (as a
/// `cargo:rustc-env`) so `main.rs` can read it via `option_env!`. When it's unset —
/// the normal case — the device generates the secret on first boot and persists it in
/// NVS instead. Set one of these only to pin a *specific* identity / API secret.
fn optional_env(name: &str) {
    println!("cargo:rerun-if-env-changed={name}");
    if let Ok(value) = std::env::var(name) {
        println!("cargo:rustc-env={name}={value}");
    }
}

fn main() {
    optional_env("IROH_SECRET");
    optional_env("FAN_API_SECRET");

    embuild::espidf::sysenv::output();
}
