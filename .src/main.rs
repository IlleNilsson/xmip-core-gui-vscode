//! `xmip-lsp`: the language server behind the VS Code extension, the
//! developer's face of the configuration tool (ADR-0014, amendment
//! 2026-09-10). It speaks the Language Server Protocol over stdio and
//! validates a node configuration through the runtime's C ABI — the same
//! `xmip_validate_v1` the desktop GUI calls, reached by loading the runtime's
//! native library and nothing else.
//!
//! The library is the one `--runtime <path>` names, and nothing else: the
//! extension passes its `xmip.runtime.library` setting. The server finds no
//! library on its own. Runtime discovery is one rule, and it is the .NET
//! surfaces' (`RuntimeLibrary` in `Xmip.Surface`), because a surface must find
//! the runtime before it can call anything in it; a second writing of that
//! rule here was a copy, and it went (ADR-0052, amendment 2026-09-24).
//!
//! **It audits** (ADR-0062) through `xmip-core-audit`, as `xmip-lsp`: `start`
//! with the library it was told, `stop` with the exit code (a warning when
//! the client left without a shutdown), a refused argument as the failure to
//! `start`, a broken stdio stream as the failure to `serve`, every panic as
//! `unhandled`, and what `server.rs` does to the runtime. The records go to
//! `XMIP_AUDIT_DIRECTORY`, else the operating system's log.

mod diagnostic;
mod framing;
mod runtime;
mod server;

use std::collections::BTreeMap;
use std::io::{self, BufReader, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use server::{Server, kept};
use xaudit::program_audit::ProgramAudit;
use xcore::{ExecutionPhase, Severity};

fn main() -> ExitCode {
    let audit = ProgramAudit::new(server::NAME, None);
    audit.watch_panics();

    let path = match runtime_path(std::env::args().skip(1)) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("{}: {reason}", server::NAME);
            kept(audit.failed("start", &reason));
            return ExitCode::from(2);
        }
    };

    let library = if let Some(path) = &path {
        eprintln!("{}: runtime library {}", server::NAME, path.display());
        path.display().to_string()
    } else {
        eprintln!("{}: {}", server::NAME, server::NOT_NAMED);
        "none named".to_string()
    };
    let told = BTreeMap::from([("library".to_string(), library)]);
    kept(audit.record(
        "start",
        ExecutionPhase::Begin,
        Severity::Information,
        None,
        told,
    ));

    match serve(Server::new(path, audit.clone())) {
        Ok(code) => {
            // Exit without a shutdown is 1, which the protocol allows and
            // which is worth a reader's notice, not an error.
            let severity = if code == 0 {
                Severity::Information
            } else {
                Severity::Warning
            };
            let exit = BTreeMap::from([("exit".to_string(), code.to_string())]);
            kept(audit.record("stop", ExecutionPhase::Finished, severity, None, exit));
            ExitCode::from(code)
        }
        Err(error) => {
            eprintln!("{}: {error}", server::NAME);
            kept(audit.failed("serve", &error.to_string()));
            ExitCode::from(1)
        }
    }
}

/// Read messages from stdin and answer on stdout until the client says
/// `exit` or the input ends.
fn serve(mut server: Server) -> io::Result<u8> {
    let mut reader = BufReader::new(io::stdin().lock());
    let mut writer = io::stdout().lock();
    let mut out = Vec::new();

    while let Some(message) = framing::read_message(&mut reader)? {
        out.clear();
        let exit = server.handle(&message, &mut out);

        for reply in &out {
            framing::write_message(&mut writer, reply)?;
        }

        if let Some(code) = exit {
            return Ok(u8::try_from(code).unwrap_or(1));
        }
    }

    writer.flush()?;

    // The client went away without saying exit: clean only after a shutdown.
    Ok(u8::from(!server.is_shut_down()))
}

/// The runtime library the arguments name, or `None` when they name none.
///
/// # Errors
/// An argument this server does not take, or `--runtime` with nothing after it.
fn runtime_path(mut arguments: impl Iterator<Item = String>) -> Result<Option<PathBuf>, String> {
    let mut flag = None;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--runtime" => {
                let value = arguments.next().ok_or("--runtime needs a path after it")?;
                flag = Some(PathBuf::from(value));
            }
            "--version" => return Err(format!("{} {}", server::NAME, env!("CARGO_PKG_VERSION"))),
            _ => match argument.strip_prefix("--runtime=") {
                Some(value) => flag = Some(PathBuf::from(value)),
                None => {
                    return Err(format!(
                        "unknown argument {argument}; only --runtime <path>"
                    ));
                }
            },
        }
    }

    Ok(flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn the_flag_names_the_library_and_nothing_else_does() {
        let flag = runtime_path(arguments(&["--runtime", "C:/a.dll"]));
        assert_eq!(flag.expect("a path"), Some(PathBuf::from("C:/a.dll")));

        let joined = runtime_path(arguments(&["--runtime=C:/c.dll"]));
        assert_eq!(joined.expect("a path"), Some(PathBuf::from("C:/c.dll")));

        assert_eq!(runtime_path(arguments(&[])).expect("no path"), None);
    }

    #[test]
    fn a_flag_without_a_path_and_an_unknown_argument_are_refused() {
        assert!(runtime_path(arguments(&["--runtime"])).is_err());
        assert!(runtime_path(arguments(&["--port", "1"])).is_err());
    }
}
