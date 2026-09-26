fn main() {
    let lib = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.build/brotli-install/lib");
    let lib = lib.canonicalize().expect("build the oracle first: python3 tools/build_reference.py");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=dylib=brotlidec");
    println!("cargo:rustc-link-lib=dylib=brotlienc");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
}
