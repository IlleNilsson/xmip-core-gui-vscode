//! The runtime's native library, loaded by path and asked to validate.
//!
//! This is the server's one route to Xmip: `xmip_validate_v1` as
//! `include/xmip_operate.h` declares it, reached through the C ABI exactly as
//! the desktop GUI reaches it through `Xmip.Abi` (ADR-0014, amendment
//! 2026-09-10). The server does not link the runtime's crates and the
//! extension does not run the `xmip` command; the boundary is the header, and
//! this file is the only one that crosses it, so it is the only one that
//! lifts the crate's `deny(unsafe_code)`.
//!
//! The shape mirrors `Operator.cs` in the .NET binding: the library is copied
//! to a temporary file before it is loaded, one call is made at a time, and
//! the report is asked for twice — once for its length, once for its text —
//! so nothing is truncated silently.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};

use abi::ffi::{Str, status};
use abi::runtime_library;
use libloading::{Library, Symbol};

/// The export the header names for validation, section 6.
pub const ENTRYPOINT: &[u8] = b"xmip_validate_v1\0";

/// `int32_t xmip_validate_v1(XmipStr, uint8_t *, size_t, size_t *)`.
type ValidateFn = unsafe extern "C" fn(Str, *mut u8, usize, *mut usize) -> i32;

/// What one validation answered: the status the header defines and the
/// report text, one problem per line, empty when the configuration is good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation {
    /// `XMIP_OK`, `XMIP_E_INVALID` or `XMIP_E_MALFORMED`.
    pub status: i32,
    /// The runtime's own words for what is wrong, or nothing.
    pub report: String,
}

impl Validation {
    /// Whether the runtime would start from this configuration.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.status == status::OK
    }
}

/// A loaded runtime library. Dropping it unloads the library and removes the
/// temporary copy it was loaded from.
pub struct Runtime {
    library: Option<Library>,
    copy: PathBuf,
    source: PathBuf,
    gate: Mutex<()>,
}

static COPIES: AtomicU32 = AtomicU32::new(0);

impl Runtime {
    /// Load the library at `path`. The reason, in words a developer reads in
    /// the editor, when it cannot be.
    ///
    /// # Errors
    /// No file at `path`; the copy could not be written; the library could
    /// not be loaded; or it does not export [`ENTRYPOINT`].
    // Loading a library runs its initialisers and trusts its exports' types;
    // nothing in Rust can check either. This is the site the crate's lint
    // exists to point at.
    #[allow(unsafe_code)]
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Err(format!("no runtime library at {}", path.display()));
        }

        // Load a copy, never the build output itself. A loaded library is
        // locked for as long as this process lives, and in development the
        // path is the runtime's own target/debug — an editor left open would
        // make the next `cargo build` fail on a locked file.
        let copy = temporary_copy_path(path);
        std::fs::copy(path, &copy)
            .map_err(|error| format!("could not copy {} for loading: {error}", path.display()))?;

        // SAFETY: the file is a copy of the runtime the developer named, and
        // the estate's runtime has no load-time behaviour beyond the loader's.
        let library = unsafe { Library::new(&copy) }
            .map_err(|error| format!("{} could not be loaded: {error}", path.display()))?;

        // SAFETY: only the export's presence is checked here; the type is the
        // header's, and the first call is where it is trusted.
        if unsafe { library.get::<ValidateFn>(ENTRYPOINT) }.is_err() {
            return Err(format!(
                "{} does not export xmip_validate_v1",
                path.display()
            ));
        }

        Ok(Self {
            library: Some(library),
            copy,
            source: path.to_path_buf(),
            gate: Mutex::new(()),
        })
    }

    /// The path this runtime was loaded from, for a surface to show.
    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Validate configuration text without applying it: the same runtime
    /// that would start it, publishing nothing. ADR-0027 clause 9.
    ///
    /// # Errors
    /// When the library has been unloaded, which only a drop in progress can
    /// cause.
    // The call itself: a borrowed string in, a buffer and its capacity out,
    // exactly the contract xmip_operate.h states.
    #[allow(unsafe_code)]
    pub fn validate(&self, configuration: &str) -> Result<Validation, String> {
        let _held = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let library = self
            .library
            .as_ref()
            .ok_or("the runtime library is unloaded")?;

        // SAFETY: the export was found at load and the header fixes its type.
        let validate: Symbol<ValidateFn> = unsafe { library.get(ENTRYPOINT) }
            .map_err(|error| format!("xmip_validate_v1 is gone: {error}"))?;

        let text = Str {
            ptr: configuration.as_ptr(),
            len: configuration.len(),
        };
        let mut needed = 0usize;

        // SAFETY: `text` borrows `configuration` for the call; a null report
        // with capacity 0 asks only for the length, as the header allows.
        let status = unsafe { validate(text, std::ptr::null_mut(), 0, &raw mut needed) };

        if needed == 0 {
            return Ok(Validation {
                status,
                report: String::new(),
            });
        }

        let mut report = vec![0u8; needed];

        // SAFETY: `report` has room for exactly `needed` bytes and `needed`
        // is writable; the runtime writes at most the capacity it is given.
        let status = unsafe { validate(text, report.as_mut_ptr(), report.len(), &raw mut needed) };

        report.truncate(needed.min(report.len()));

        Ok(Validation {
            status,
            report: String::from_utf8_lossy(&report).into_owned(),
        })
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Unload first: on Windows a loaded library cannot be deleted.
        drop(self.library.take());
        let _ = std::fs::remove_file(&self.copy);
    }
}

fn temporary_copy_path(path: &Path) -> PathBuf {
    let name = path.file_name().map_or_else(
        || runtime_library::file_name().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let sequence = COPIES.fetch_add(1, Ordering::Relaxed);

    std::env::temp_dir().join(format!("xmip-lsp-{}-{sequence}-{name}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The estate's runtime as `cargo build` leaves it, three levels up from
    /// this crate. Absent when nobody built it, and that is not a failure.
    fn built_runtime() -> Option<PathBuf> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../platform/runtime/target/debug")
            .join(runtime_library::file_name());

        if path.is_file() {
            Some(path)
        } else {
            println!("skipped: no runtime library at {}", path.display());
            None
        }
    }

    /// The node the GUI starts, `gui/samples/edge-01.xmip.toml` beside this
    /// repository: one fixture, shared, never copied (ADR-0052). Read at test
    /// time rather than included, because the sample is only where the built
    /// runtime is — in the estate — and a checkout of this repository alone
    /// must still compile its tests.
    fn sample_node() -> String {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../samples/edge-01.xmip.toml");

        std::fs::read_to_string(&sample)
            .unwrap_or_else(|error| panic!("no sample node at {}: {error}", sample.display()))
    }

    #[test]
    fn a_missing_library_is_refused_with_its_path() {
        let path = Path::new("Z:/no/such/xmip_core_runtime.dll");
        let reason = Runtime::load(path).err().expect("refused");

        assert!(reason.starts_with("no runtime library at"), "{reason}");
        assert!(reason.contains("xmip_core_runtime.dll"));
    }

    #[test]
    fn a_file_that_is_not_a_library_is_refused_and_its_copy_removed() {
        let path = std::env::temp_dir().join("xmip-lsp-not-a-library.dll");
        std::fs::write(&path, b"not a library").expect("writes");

        let reason = Runtime::load(&path).err().expect("refused");

        assert!(reason.contains("could not be loaded"), "{reason}");
        std::fs::remove_file(&path).expect("removes");
    }

    #[test]
    fn the_built_runtime_validates_the_sample_node_and_refuses_a_broken_one() {
        let Some(path) = built_runtime() else {
            return;
        };
        let runtime = Runtime::load(&path).expect("loads");

        assert_eq!(runtime.source(), path);

        let good = runtime.validate(&sample_node()).expect("calls");
        assert!(good.is_valid(), "unexpected report: {}", good.report);
        assert!(good.report.is_empty());

        let broken = runtime
            .validate("[service]\nname = \"edge\"\ncluster_name = \"lab\n")
            .expect("calls");
        assert_eq!(broken.status, status::INVALID);
        assert!(!broken.report.is_empty());
        assert!(broken.report.contains("line 3"), "{}", broken.report);

        let incomplete = runtime
            .validate("[service]\nname = \"edge\"\n")
            .expect("calls");
        assert_eq!(incomplete.status, status::INVALID);
        assert!(!incomplete.report.is_empty());

        let copy = runtime.copy.clone();
        assert!(copy.is_file(), "loaded from a copy");
        drop(runtime);
        assert!(!copy.is_file(), "the copy is removed on drop");
    }
}
