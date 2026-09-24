// Vendored for libheifer: the assembly build path (nasm/cc) is removed; only
// the environment steps of upstream's build.rs remain.
use std::env;

fn main() {
    println!("cargo:rustc-env=PROFILE={}", env::var("PROFILE").unwrap());
    if let Ok(value) = env::var("CARGO_CFG_TARGET_FEATURE") {
        println!("cargo:rustc-env=CARGO_CFG_TARGET_FEATURE={value}");
    }
    println!(
        "cargo:rustc-env=CARGO_ENCODED_RUSTFLAGS={}",
        env::var("CARGO_ENCODED_RUSTFLAGS").unwrap()
    );
}
