//! The runtime's native library, loaded by path and asked to validate and
//! to answer the designer.
//!
//! This is the server's one route to Xmip: `xmip_validate_v1` and section
//! 10's designer exports (ADR-0064) as `include/xmip_operate.h` declares
//! them, reached through the C ABI exactly as the desktop GUI reaches it
//! through `Xmip.Abi` (ADR-0014, amendment 2026-09-10). The server does not
//! link the runtime's crates and the extension does not run the `xmip`
//! command; the boundary is the header, and this file is the only one that
//! crosses it, so it is the only one that lifts the crate's
//! `deny(unsafe_code)`.
//!
//! The shape mirrors `Operator.cs` in the .NET binding: the library is copied
//! to a temporary file before it is loaded, one call is made at a time, and
//! an answer that does not fit the room given is asked for again at its
//! true length, so nothing is truncated silently.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};

use abi::ffi::{Str, status};
use abi::operate::catalogue::{CatalogueFn, TECHNOLOGY_CATALOGUE_ENTRYPOINT};
use abi::operate::design::DesignFn;
use abi::operate::{ValidateFn, XMIP_VALIDATE_ENTRYPOINT};
use libloading::{Library, Symbol};

/// The export the header names for validation, section 6, as the binding
/// declares it: `xmip-core-abi`'s name and shape, never this crate's own
/// (open problem 25).
const ENTRYPOINT: &[u8] = XMIP_VALIDATE_ENTRYPOINT.as_bytes();

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

/// What a designer export answered: `XMIP_OK` and its answer, or
/// `XMIP_E_INVALID` and the refusal, one sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: i32,
    pub text: String,
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
        sweep_copies();
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
                "{} does not export {XMIP_VALIDATE_ENTRYPOINT}",
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
            .map_err(|error| format!("{XMIP_VALIDATE_ENTRYPOINT} is gone: {error}"))?;

        let text = borrowed(configuration);

        // SAFETY: `text` borrows `configuration` for the call; `asked` hands
        // over a buffer and its true capacity, as the header requires.
        let (status, report) =
            asked(|out, cap, needed| unsafe { validate(text, out, cap, needed) });

        Ok(Validation { status, report })
    }

    /// Ask one of section 10's exports: `input`, and `argument` where the
    /// export takes one, in; its answer or its refusal back.
    ///
    /// # Errors
    /// When the library is unloaded or does not export `entrypoint` — a
    /// runtime built before the designer existed.
    // The call itself, as for validate: two borrowed strings in, a buffer
    // and its capacity out, the contract xmip_operate.h section 10 states.
    #[allow(unsafe_code)]
    pub fn design(&self, entrypoint: &str, input: &str, argument: &str) -> Result<Answer, String> {
        let _held = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let library = self
            .library
            .as_ref()
            .ok_or("the runtime library is unloaded")?;

        // SAFETY: the header fixes the type of every section 10 export.
        let design: Symbol<DesignFn> =
            unsafe { library.get(entrypoint.as_bytes()) }.map_err(|_| {
                format!(
                    "{} does not export {entrypoint}: build the runtime again for the designer",
                    self.source.display()
                )
            })?;
        let (input, argument) = (borrowed(input), borrowed(argument));

        // SAFETY: both texts borrow for the call; `asked` hands over a buffer
        // and its true capacity, as the header requires.
        let (status, text) =
            asked(|out, cap, needed| unsafe { design(input, argument, out, cap, needed) });

        Ok(Answer { status, text })
    }

    /// Ask section 12's export for the technologies the runtime carries and
    /// the settings each declares (ADR-0064, amendment 2026-09-26): every
    /// one when `technology` is empty, that one alone otherwise.
    ///
    /// # Errors
    /// When the library is unloaded or does not export the catalogue — a
    /// runtime built before the technologies declared their settings.
    // The call itself: a borrowed name in, a buffer and its capacity out,
    // the contract xmip_operate.h section 12 states.
    #[allow(unsafe_code)]
    pub fn catalogue(&self, technology: &str) -> Result<Answer, String> {
        let _held = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let library = self
            .library
            .as_ref()
            .ok_or("the runtime library is unloaded")?;

        // SAFETY: the header fixes the export's type.
        let catalogue: Symbol<CatalogueFn> =
            unsafe { library.get(TECHNOLOGY_CATALOGUE_ENTRYPOINT.as_bytes()) }.map_err(|_| {
                format!(
                    "{} does not export {TECHNOLOGY_CATALOGUE_ENTRYPOINT}: build the runtime again",
                    self.source.display()
                )
            })?;
        let technology = borrowed(technology);

        // SAFETY: the name borrows for the call; `asked` hands over a buffer
        // and its true capacity, as the header requires.
        let (status, text) =
            asked(|out, cap, needed| unsafe { catalogue(technology, out, cap, needed) });

        Ok(Answer { status, text })
    }
}

fn borrowed(text: &str) -> Str {
    Str {
        ptr: text.as_ptr(),
        len: text.len(),
    }
}

/// Room for what a design or a report is in practice, so one call answers.
const ROOM: usize = 64 * 1024;

/// The header's text shape, asked so nothing is truncated: once with room
/// for what an answer usually is, and again with room for its true length
/// only when it did not fit — every call does the whole work again, and a
/// second one doubles what the developer waits for.
fn asked(call: impl Fn(*mut u8, usize, *mut usize) -> i32) -> (i32, String) {
    let mut text = vec![0u8; ROOM];
    let mut needed = 0usize;
    let mut status = call(text.as_mut_ptr(), text.len(), &raw mut needed);

    if needed > text.len() {
        text.resize(needed, 0);
        status = call(text.as_mut_ptr(), text.len(), &raw mut needed);
    }
    text.truncate(needed.min(text.len()));

    (status, String::from_utf8_lossy(&text).into_owned())
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // The runtime stays resident once loaded (ADR-0027, amendment
        // 2026-09-30), so on Windows its copy stays locked while this process
        // lives and is removed by a later load's sweep; elsewhere it goes now.
        drop(self.library.take());
        let _ = std::fs::remove_file(&self.copy);
    }
}

/// The prefix every copy this server loads from carries.
const COPY_PREFIX: &str = "xmip-lsp-";

/// Remove every copy another process's load left that is no longer in use;
/// one a process still holds stays, its removal refused, and is tried again
/// next time. This process's own are its own to remove.
fn sweep_copies() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let own = format!("{COPY_PREFIX}{}-", std::process::id());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(COPY_PREFIX) && !name.starts_with(&own) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn temporary_copy_path(path: &Path) -> PathBuf {
    let name = path.file_name().map_or_else(
        || "runtime".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let sequence = COPIES.fetch_add(1, Ordering::Relaxed);

    std::env::temp_dir().join(format!(
        "{COPY_PREFIX}{}-{sequence}-{name}",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::built::runtime_library as built_runtime;

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
        let path = std::env::temp_dir().join(format!(
            "{COPY_PREFIX}{}-not-a-library.dll",
            std::process::id()
        ));
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
            .validate("[service]\nname = \"edge\"\ndata = \"../data/one\n")
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
        // Resident once loaded: on Windows the copy stays locked until this
        // process ends, and a later load sweeps it; elsewhere it goes now.
        assert_eq!(copy.is_file(), cfg!(windows), "the copy after the drop");
    }

    #[test]
    fn a_load_sweeps_the_copies_no_process_holds() {
        let Some(path) = built_runtime() else {
            return;
        };
        let left = std::env::temp_dir().join(format!("{COPY_PREFIX}left-behind.dll"));
        std::fs::write(&left, b"an earlier load's copy").expect("written");
        let runtime = Runtime::load(&path).expect("loads");
        assert!(!left.is_file(), "swept as the next copy was made");
        assert!(runtime.copy.is_file(), "its own copy stays");
    }
}
