// SPDX-License-Identifier: LGPL-3.0-or-later
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // rav1d's x86 assembly addresses its `#[no_mangle]` tables with
    // PC-relative relocations. In an ELF shared library those symbols are
    // exported and preemptible, which the linker rejects, so the library's
    // own definitions must bind locally.
    let family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let vendor = std::env::var("CARGO_CFG_TARGET_VENDOR").unwrap_or_default();
    if std::env::var_os("CARGO_FEATURE_AV1").is_some() && family == "unix" && vendor != "apple" {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-Bsymbolic");
    }
}
