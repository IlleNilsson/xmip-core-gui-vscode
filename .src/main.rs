//! `xmip-lsp`: the language server behind the VS Code extension, the
//! developer's face of the configuration tool (ADR-0014, amendment
//! 2026-09-10). It speaks the Language Server Protocol over stdio and
//! validates a node configuration through the runtime's C ABI — the same
//! `xmip_validate_v1` the desktop GUI calls, reached by loading the runtime's
//! native library and nothing else.
//!
//! The library path comes from `--runtime <path>`, then the environment
//! variable `XMIP_RUNTIME_LIBRARY`, then the library's name beside this
//! binary — the rule every surface keeps, `abi::runtime_library`, with the
//! flag standing where a .NET surface reads its configuration key.

mod diagnostic;
mod framing;
mod runtime;
mod server;

use std::io::{self, BufReader, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use abi::runtime_library;
use server::Server;

fn main() -> ExitCode {
    let variable = std::env::var(runtime_library::ENVIRONMENT_VARIABLE).ok();
    let path = match runtime_path(std::env::args().skip(1), variable.as_deref()) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("{}: {reason}", server::NAME);
            return ExitCode::from(2);
        }
    };

    eprintln!("{}: runtime library {}", server::NAME, path.display());

    match serve(Server::new(path)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{}: {error}", server::NAME);
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

/// The runtime library path from the arguments, the variable, or beside the
/// binary (ADR-0052 clause 1).
///
/// # Errors
/// An argument this server does not take, or `--runtime` with nothing after it.
fn runtime_path(
    mut arguments: impl Iterator<Item = String>,
    variable: Option<&str>,
) -> Result<PathBuf, String> {
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

    Ok(runtime_library::choose(
        flag,
        variable,
        runtime_library::beside_executable(),
    ))
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
    fn the_flag_wins_over_the_variable_which_wins_over_the_default() {
        let flag = runtime_path(arguments(&["--runtime", "C:/a.dll"]), Some("C:/b.dll"));
        assert_eq!(flag.expect("a path"), PathBuf::from("C:/a.dll"));

        let joined = runtime_path(arguments(&["--runtime=C:/c.dll"]), None);
        assert_eq!(joined.expect("a path"), PathBuf::from("C:/c.dll"));

        let variable = runtime_path(arguments(&[]), Some("C:/b.dll"));
        assert_eq!(variable.expect("a path"), PathBuf::from("C:/b.dll"));

        let default = runtime_path(arguments(&[]), Some(""));
        assert_eq!(
            default.expect("a path"),
            runtime_library::beside_executable()
        );
    }

    #[test]
    fn a_flag_without_a_path_and_an_unknown_argument_are_refused() {
        assert!(runtime_path(arguments(&["--runtime"]), None).is_err());
        assert!(runtime_path(arguments(&["--port", "1"]), None).is_err());
    }
}
