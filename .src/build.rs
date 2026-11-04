//! The Xmip mark linked into xmip-lsp as its Windows icon: the resource
//! the estate's `Sync-XmipEstate -Brand` makes from the mark and copies
//! here, handed to the MSVC linker beside the objects.

const RESOURCE: &str = ".src/xmip-x.res";

fn main() {
    println!("cargo:rerun-if-changed={RESOURCE}");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if let (true, true, Ok(root)) = (windows, msvc, std::env::var("CARGO_MANIFEST_DIR")) {
        let resource = std::path::Path::new(&root).join(RESOURCE);
        println!("cargo:rustc-link-arg-bins={}", resource.display());
    }
}
