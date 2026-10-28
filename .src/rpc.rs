//! The JSON-RPC messages the server writes: an answer, a refusal and the
//! diagnostics it publishes for a document.

use serde_json::{Value, json};

/// The answer to the request `id`.
pub fn response(id: Option<&Value>, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.cloned().unwrap_or(Value::Null), "result": result })
}

/// The refusal of the request `id`, with its code and its words.
pub fn error(id: Option<&Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.cloned().unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
    })
}

/// The diagnostics of the document at `uri`, published to the client.
pub fn publish_diagnostics(uri: &str, diagnostics: &[Value]) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "diagnostics": diagnostics },
    })
}
