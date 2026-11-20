//! The runtime library the tests load, found once for every test that
//! needs one: `XMIP_RUNTIME_LIBRARY` where it names a file — the variable the
//! .NET surfaces' one rule reads too (ADR-0052) — else xmip-core-operate's
//! library where cargo built it in this build directory, found above the
//! test program (`file::built`, the estate's one rule).
//! Absent when neither is there, which the asking test says and skips.

use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::path::{Path, PathBuf};

/// The variable naming the runtime library.
const VARIABLE: &str = "XMIP_RUNTIME_LIBRARY";

/// The runtime library to test against, if one is built.
pub fn runtime_library() -> Option<PathBuf> {
    let named = std::env::var_os(VARIABLE).map(PathBuf::from);
    let name = format!("{DLL_PREFIX}xmip_core_operate{DLL_SUFFIX}");
    let path = named
        .filter(|path| path.is_file())
        .or_else(|| file::built::found(&name))
        .unwrap_or_else(|| PathBuf::from(&name));

    if path.is_file() {
        Some(path)
    } else {
        println!(
            "skipped: no runtime library at {} and no {VARIABLE}",
            path.display()
        );
        None
    }
}

/// The variable naming the test cluster's file, as configure's fixture
/// reads it.
const CLUSTER_VARIABLE: &str = "XMIP_TEST_CLUSTER";

/// The test cluster's `cluster.xmip`: the file `XMIP_TEST_CLUSTER` names, else
/// the estate's `test/cluster.xmip`, five levels up from this crate. Its nodes'
/// names are read from it through the runtime, never written in a test
/// (`test/NodeName.Test.ps1`).
pub fn test_cluster() -> String {
    let path = std::env::var_os(CLUSTER_VARIABLE).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../../test/cluster.xmip"),
        PathBuf::from,
    );
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("no test cluster at {}: {error}", path.display()))
}
