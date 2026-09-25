//! The language server: what it answers and what it publishes.
//!
//! A message in, zero or more messages out, and the process ends when the
//! client says so. Documents are held whole — the server asks for full
//! synchronisation — because validation needs the whole text and a node
//! configuration is small. Only documents named `.toml` are validated; the
//! rest are held and never reported on.
//!
//! What it does to the runtime is audited (ADR-0062): a library loaded as
//! `load-runtime`, and a library that could not be loaded or a validation
//! the runtime could not answer as a failure of `load-runtime` or `validate`
//! — once per reason, because the server retries on every keystroke and the
//! same reason a hundred times is one failure.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use xaudit::AuditError;
use xaudit::emit::AuditOutcome;
use xaudit::program_audit::ProgramAudit;
use xcore::{ExecutionPhase, Severity};

use crate::diagnostic;
use crate::runtime::{Runtime, Validation};

/// The name the server reports to the client.
pub const NAME: &str = "xmip-lsp";

/// JSON-RPC: the method is not one this server knows.
const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC: the parameters are not what the method takes.
const INVALID_PARAMS: i64 = -32602;
/// The runtime library could not be reached; the message says why.
const RUNTIME_UNAVAILABLE: i64 = -32001;

/// What every validation says when no runtime library was named.
pub const NOT_NAMED: &str = "no runtime library was named: set xmip.runtime.library to the \
    runtime's native library, or start the server with --runtime <path>";

/// The server's state between messages.
pub struct Server {
    runtime_path: Option<PathBuf>,
    runtime: Option<Runtime>,
    documents: HashMap<String, String>,
    shut_down: bool,
    audit: ProgramAudit,
    /// The last failure audited, so one repeated on every validation is
    /// recorded once; cleared when the runtime loads.
    audited: Option<String>,
}

impl Server {
    /// A server that will load the runtime library at `runtime_path` the
    /// first time a document needs validating — and try again on every
    /// validation until it succeeds, because the developer may build the
    /// runtime after opening the editor. With no path, every validation says
    /// that none was named: the server is told where the runtime is and
    /// finds nothing on its own (ADR-0052, amendment 2026-09-24). What it
    /// does to the runtime is recorded in `audit`.
    #[must_use]
    pub fn new(runtime_path: Option<PathBuf>, audit: ProgramAudit) -> Self {
        Self {
            runtime_path,
            runtime: None,
            documents: HashMap::new(),
            shut_down: false,
            audit,
            audited: None,
        }
    }

    /// Whether the client has asked the server to shut down.
    #[must_use]
    pub fn is_shut_down(&self) -> bool {
        self.shut_down
    }

    /// Handle one message, appending what goes back to the client to `out`.
    /// `Some(code)` when the client has said `exit`: 0 after a shutdown,
    /// 1 without one, as the protocol specifies.
    pub fn handle(&mut self, message: &Value, out: &mut Vec<Value>) -> Option<i32> {
        let method = message["method"].as_str().unwrap_or_default();
        let params = &message["params"];
        let id = message.get("id").filter(|id| !id.is_null());

        match method {
            "initialize" => out.push(response(id, &initialize_result())),
            "shutdown" => {
                self.shut_down = true;
                out.push(response(id, &Value::Null));
            }
            "exit" => return Some(i32::from(!self.shut_down)),
            "textDocument/didOpen" => self.opened(params, out),
            "textDocument/didChange" => self.changed(params, out),
            "textDocument/didSave" => self.saved(params, out),
            "textDocument/didClose" => self.closed(params, out),
            "xmip/validate" => out.push(self.validate_request(id, params)),
            _ => {
                if let Some(id) = id {
                    let reason = format!("{method} is not a method xmip-lsp handles");
                    out.push(error(Some(id), METHOD_NOT_FOUND, &reason));
                }
            }
        }

        None
    }

    fn opened(&mut self, params: &Value, out: &mut Vec<Value>) {
        let uri = uri_of(params);
        let text = params["textDocument"]["text"].as_str().unwrap_or_default();

        self.documents.insert(uri.clone(), text.to_string());
        self.publish(&uri, out);
    }

    fn changed(&mut self, params: &Value, out: &mut Vec<Value>) {
        let uri = uri_of(params);

        // Full synchronisation: the last change carries the whole text.
        if let Some(text) = params["contentChanges"]
            .as_array()
            .and_then(|changes| changes.last())
            .and_then(|change| change["text"].as_str())
        {
            self.documents.insert(uri.clone(), text.to_string());
        }

        self.publish(&uri, out);
    }

    fn saved(&mut self, params: &Value, out: &mut Vec<Value>) {
        let uri = uri_of(params);

        if let Some(text) = params["text"].as_str() {
            self.documents.insert(uri.clone(), text.to_string());
        }

        self.publish(&uri, out);
    }

    fn closed(&mut self, params: &Value, out: &mut Vec<Value>) {
        let uri = uri_of(params);

        if self.documents.remove(&uri).is_some() && is_toml(&uri) {
            out.push(publish_diagnostics(&uri, &[]));
        }
    }

    /// Validate the document at `uri` and publish what the runtime said,
    /// or the one diagnostic that says the runtime could not be reached.
    fn publish(&mut self, uri: &str, out: &mut Vec<Value>) {
        if !is_toml(uri) {
            return;
        }

        let Some(text) = self.documents.get(uri).cloned() else {
            return;
        };

        let diagnostics = match self.validate(&text) {
            Ok(report) => diagnostic::diagnostics(&report, &text),
            Err(reason) => {
                let message = format!("{NAME}: {reason}");
                vec![diagnostic::diagnostic(
                    &message,
                    &diagnostic::range(0, 0, &text),
                )]
            }
        };

        out.push(publish_diagnostics(uri, &diagnostics));
    }

    /// `xmip/validate`: the raw report for the text in the params, or for
    /// the document the params name. The extension's command reads this.
    fn validate_request(&mut self, id: Option<&Value>, params: &Value) -> Value {
        let text = params["text"]
            .as_str()
            .map(str::to_string)
            .or_else(|| self.documents.get(&uri_of(params)).cloned());

        let Some(text) = text else {
            return error(
                id,
                INVALID_PARAMS,
                "xmip/validate needs a text or an open uri",
            );
        };

        match self.validated(&text) {
            Ok((validation, source)) => response(
                id,
                &json!({
                    "status": validation.status,
                    "valid": validation.is_valid(),
                    "report": validation.report,
                    "runtime": source.display().to_string(),
                }),
            ),
            Err(reason) => error(id, RUNTIME_UNAVAILABLE, &reason),
        }
    }

    fn validate(&mut self, text: &str) -> Result<String, String> {
        self.validated(text)
            .map(|(validation, _)| validation.report)
    }

    /// What the runtime said of `text`, and the library that said it; a
    /// runtime that could not answer is audited as the failure to `validate`.
    fn validated(&mut self, text: &str) -> Result<(Validation, PathBuf), String> {
        let runtime = self.runtime()?;
        let source = runtime.source().to_path_buf();
        let answered = runtime.validate(text);

        match answered {
            Ok(validation) => Ok((validation, source)),
            Err(reason) => {
                self.failed("validate", &reason);
                Err(reason)
            }
        }
    }

    fn runtime(&mut self) -> Result<&Runtime, String> {
        if self.runtime.is_none() {
            let loading = self
                .runtime_path
                .as_ref()
                .ok_or_else(|| NOT_NAMED.to_string())
                .and_then(|path| Runtime::load(path));
            let loaded = match loading {
                Ok(loaded) => loaded,
                Err(reason) => {
                    self.failed("load-runtime", &reason);
                    return Err(reason);
                }
            };
            let library = loaded.source().display().to_string();
            eprintln!("{NAME}: loaded {library}");
            kept(self.audit.record(
                "load-runtime",
                ExecutionPhase::Finished,
                Severity::Information,
                None,
                BTreeMap::from([("library".to_string(), library)]),
            ));
            self.audited = None;
            self.runtime = Some(loaded);
        }

        self.runtime
            .as_ref()
            .ok_or_else(|| "no runtime".to_string())
    }

    /// Audit `reason` as the failure of `action`, unless it is the failure
    /// audited last.
    fn failed(&mut self, action: &str, reason: &str) {
        if self.audited.as_deref() == Some(reason) {
            return;
        }
        self.audited = Some(reason.to_string());
        kept(self.audit.failed(action, reason));
    }
}

/// A record neither the audit sink nor the operating system's log kept is
/// said on stderr, which the editor shows in the server's output channel.
pub fn kept(outcome: Result<AuditOutcome, AuditError>) {
    if let Err(error) = outcome {
        eprintln!("{NAME}: audit: {error}");
    }
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": {
                "openClose": true,
                "change": 1,
                "save": { "includeText": true },
            },
        },
        "serverInfo": { "name": NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

fn uri_of(params: &Value) -> String {
    params["textDocument"]["uri"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn is_toml(uri: &str) -> bool {
    Path::new(uri)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
}

fn response(id: Option<&Value>, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.cloned().unwrap_or(Value::Null), "result": result })
}

fn error(id: Option<&Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.cloned().unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
    })
}

fn publish_diagnostics(uri: &str, diagnostics: &[Value]) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "diagnostics": diagnostics },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};

    const NOWHERE: &str = "Z:/no/such/xmip_core_runtime.dll";

    /// An audit into a directory of the test's own, so a test never writes
    /// to the operating system's log. Numbered, because tests run at once
    /// and one clearing another's directory sent a record to that log.
    fn audit(name: &str) -> ProgramAudit {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "xmip-lsp-audit-{name}-{}-{number}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        ProgramAudit::new(NAME, Some(&directory))
    }

    /// A server whose runtime is nowhere, so every validation reports that.
    fn server() -> Server {
        Server::new(Some(PathBuf::from(NOWHERE)), audit("server"))
    }

    fn handle(server: &mut Server, message: &Value) -> (Vec<Value>, Option<i32>) {
        let mut out = Vec::new();
        let exit = server.handle(message, &mut out);
        (out, exit)
    }

    #[test]
    fn initialize_answers_with_full_synchronisation_and_the_server_name() {
        let (out, exit) = handle(
            &mut server(),
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        );

        assert!(exit.is_none());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["id"], 1);
        assert_eq!(
            out[0]["result"]["capabilities"]["textDocumentSync"]["change"],
            1
        );
        assert_eq!(out[0]["result"]["serverInfo"]["name"], "xmip-lsp");
    }

    #[test]
    fn exit_after_shutdown_is_zero_and_without_it_is_one() {
        let mut fresh = server();
        assert_eq!(handle(&mut fresh, &json!({"method": "exit"})).1, Some(1));

        let mut polite = server();
        let (out, _) = handle(&mut polite, &json!({"id": 2, "method": "shutdown"}));
        assert_eq!(out[0]["result"], Value::Null);
        assert!(polite.is_shut_down());
        assert_eq!(handle(&mut polite, &json!({"method": "exit"})).1, Some(0));
    }

    #[test]
    fn an_unknown_request_is_refused_and_an_unknown_notification_ignored() {
        let mut target = server();
        let (out, _) = handle(
            &mut target,
            &json!({"id": 3, "method": "textDocument/hover"}),
        );

        assert_eq!(out[0]["error"]["code"], METHOD_NOT_FOUND);

        let (out, _) = handle(&mut target, &json!({"method": "$/cancelRequest"}));
        assert!(out.is_empty());
    }

    #[test]
    fn opening_a_toml_document_without_a_runtime_publishes_the_reason_at_line_zero() {
        let (out, _) = handle(
            &mut server(),
            &json!({"method": "textDocument/didOpen", "params": {"textDocument": {
                "uri": "file:///c:/node/edge-01.xmip.toml", "text": "[service]\n"}}}),
        );

        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["method"], "textDocument/publishDiagnostics");
        let diagnostics = out[0]["params"]["diagnostics"].as_array().expect("a list");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["range"]["start"]["line"], 0);
        assert!(
            diagnostics[0]["message"]
                .as_str()
                .expect("text")
                .contains("no runtime library")
        );
    }

    #[test]
    fn a_document_that_is_not_toml_is_held_and_never_reported_on() {
        let mut target = server();
        let (out, _) = handle(
            &mut target,
            &json!({"method": "textDocument/didOpen", "params": {"textDocument": {
                "uri": "file:///c:/node/notes.md", "text": "# notes\n"}}}),
        );

        assert!(out.is_empty());
        assert_eq!(target.documents.len(), 1);
    }

    #[test]
    fn change_and_save_republish_and_close_clears() {
        let mut target = server();
        let uri = "file:///c:/node/edge-01.xmip.toml";
        handle(
            &mut target,
            &json!({"method": "textDocument/didOpen", "params": {"textDocument": {
                "uri": uri, "text": "a = 1\n"}}}),
        );

        let (out, _) = handle(
            &mut target,
            &json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": uri},
                "contentChanges": [{"text": "a = 1\nb = 2\n"}]}}),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(target.documents[uri], "a = 1\nb = 2\n");

        let (out, _) = handle(
            &mut target,
            &json!({"method": "textDocument/didSave", "params": {"textDocument": {"uri": uri}}}),
        );
        assert_eq!(out[0]["method"], "textDocument/publishDiagnostics");

        let (out, _) = handle(
            &mut target,
            &json!({"method": "textDocument/didClose", "params": {"textDocument": {"uri": uri}}}),
        );
        assert!(
            out[0]["params"]["diagnostics"]
                .as_array()
                .expect("a list")
                .is_empty()
        );
        assert!(target.documents.is_empty());
    }

    #[test]
    fn validate_without_text_or_an_open_document_is_invalid_params() {
        let (out, _) = handle(
            &mut server(),
            &json!({"id": 4, "method": "xmip/validate", "params": {"textDocument": {
                "uri": "file:///c:/node/none.toml"}}}),
        );

        assert_eq!(out[0]["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn validate_with_text_and_no_runtime_says_the_runtime_is_unavailable() {
        let (out, _) = handle(
            &mut server(),
            &json!({"id": 5, "method": "xmip/validate", "params": {"text": "[service]\n"}}),
        );

        assert_eq!(out[0]["id"], 5);
        assert_eq!(out[0]["error"]["code"], RUNTIME_UNAVAILABLE);
    }

    #[test]
    fn a_server_told_no_runtime_says_so_and_looks_nowhere() {
        let mut target = Server::new(None, audit("none"));
        let (out, _) = handle(
            &mut target,
            &json!({"id": 7, "method": "xmip/validate", "params": {"text": "[service]\n"}}),
        );

        assert_eq!(out[0]["error"]["code"], RUNTIME_UNAVAILABLE);
        assert_eq!(out[0]["error"]["message"], NOT_NAMED);
    }

    /// ADR-0062: a runtime that cannot be loaded is audited, and the retry
    /// every validation makes is not a new failure each time.
    #[test]
    fn a_runtime_that_cannot_be_loaded_is_audited_once_per_reason() {
        let audit = audit("unloadable");
        let mut target = Server::new(Some(PathBuf::from(NOWHERE)), audit.clone());
        for id in 0..3 {
            let request = json!({"id": id, "method": "xmip/validate", "params": {"text": "x"}});
            handle(&mut target, &request);
        }

        let file = audit.file().expect("a file sink");
        let text = std::fs::read_to_string(&file).expect("the failure was recorded");
        assert_eq!(
            text.matches("action = \"load-runtime\"").count(),
            1,
            "{text}"
        );
        assert!(text.contains("phase = \"failure\""), "{text}");
        assert!(text.contains("no runtime library at"), "{text}");
        let _ = std::fs::remove_dir_all(file.parent().expect("a directory"));
    }

    #[test]
    fn validate_over_the_built_runtime_returns_the_raw_report() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../platform/runtime/target/debug")
            .join(format!("{DLL_PREFIX}xmip_core_runtime{DLL_SUFFIX}"));

        if !path.is_file() {
            println!("skipped: no runtime library at {}", path.display());
            return;
        }

        let mut target = Server::new(Some(path), audit("built"));
        let (out, _) = handle(
            &mut target,
            &json!({"id": 6, "method": "xmip/validate", "params": {"text": "[service]\n"}}),
        );

        assert_eq!(out[0]["result"]["valid"], false);
        assert!(
            !out[0]["result"]["report"]
                .as_str()
                .expect("text")
                .is_empty()
        );
    }
}
