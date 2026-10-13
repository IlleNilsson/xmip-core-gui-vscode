//! The edits the editor makes to the cluster's `xmip.toml`, each one of
//! `xmip_cluster_edit_v1`'s (`xmip_operate.h` section 10): the server says
//! which edit, in the shape `xmip-core-configure`'s `ClusterEdit` reads, and
//! the runtime makes it and answers the text. No TOML is written here but
//! the one value a developer typed in answer to a question, written as the
//! kind the key declares.

use abi::operate::design::CLUSTER_EDIT_ENTRYPOINT;
use serde_json::{Value, json};

use crate::designer::{Unanswered, ask};
use crate::runtime::Runtime;

/// `text` with `edit` made to it by the runtime.
///
/// # Errors
/// The runtime could not be asked, or refused the edit in its own words.
pub fn made(runtime: &Runtime, text: &str, edit: &Value) -> Result<String, Unanswered> {
    ask(runtime, CLUSTER_EDIT_ENTRYPOINT, text, &edit.to_string())
}

/// Rename the entry or node at `section`, and every name standing for it.
#[must_use]
pub fn rename(section: &[String], name: &str) -> Value {
    json!({ "rename": { "section": section, "name": name } })
}

/// Remove the entry or node at `section`, with what names it when
/// `references`.
#[must_use]
pub fn remove(section: &[String], references: bool) -> Value {
    json!({ "remove-entry": { "section": section, "references": references } })
}

/// Make a reference name `name`.
#[must_use]
pub fn refer(holder: &Value, key: &Value, index: &Value, name: &str) -> Value {
    json!({ "refer": { "section": holder, "key": key, "index": index, "name": name } })
}

/// Rename a key of a table, its value kept.
#[must_use]
pub fn rename_key(section: &[String], key: &[String], to: &str) -> Value {
    json!({ "rename-key": { "section": section, "key": key, "to": to } })
}

/// Write `value`, TOML, at `key` of the table at `section`.
#[must_use]
pub fn set(section: &[String], key: &[String], value: &str) -> Value {
    json!({ "set": { "section": section, "key": key, "value": value } })
}

/// Add the entry `name` to the list at `section`, with `values`: each
/// `{ key, value }`, the value TOML.
#[must_use]
pub fn add_entry(section: &[String], name: &str, values: &[Value]) -> Value {
    json!({ "add-entry": { "section": section, "name": name, "values": values } })
}

/// Declare the node `name`.
#[must_use]
pub fn add_node(name: &str) -> Value {
    json!({ "add-node": { "name": name } })
}

/// What a developer typed for a key of `kind`, as the TOML value it is:
/// `true` or `false` for a boolean, a whole number for an integer, and
/// text — a quoted string — for every other kind.
///
/// # Errors
/// A boolean or an integer that is not one.
pub fn literal(kind: &str, typed: &str) -> Result<String, Unanswered> {
    let typed = typed.trim();
    match kind {
        "boolean" if typed == "true" || typed == "false" => Ok(typed.to_string()),
        "integer" if typed.parse::<i64>().is_ok() => Ok(typed.to_string()),
        "boolean" | "integer" => Err(Unanswered::Refused(format!("'{typed}' is not a {kind}"))),
        // A JSON string's escapes are TOML's basic string's.
        _ => Ok(Value::from(typed).to_string()),
    }
}

/// The value written in place of a missing key of `kind` until the
/// developer writes its own: the kind's empty value — no text, nothing
/// counted, not so, the first word it reads.
#[must_use]
pub fn placeholder(kind: &str, choices: &[String]) -> String {
    match kind {
        "boolean" => "false".to_string(),
        "integer" => "0".to_string(),
        "choice" if !choices.is_empty() => Value::from(choices[0].as_str()).to_string(),
        "array" => "[]".to_string(),
        "table" => "{}".to_string(),
        _ => "\"\"".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_answer_is_the_value_its_kind_reads() {
        assert_eq!(literal("boolean", " true ").ok().as_deref(), Some("true"));
        assert_eq!(literal("integer", "44").ok().as_deref(), Some("44"));
        assert!(literal("integer", "many").is_err());
        assert_eq!(
            literal("text", "C:\\in \"x\"").ok().as_deref(),
            Some("\"C:\\\\in \\\"x\\\"\"")
        );
        assert_eq!(placeholder("choice", &["light".to_string()]), "\"light\"");
        assert_eq!(placeholder("text", &[]), "\"\"");
    }
}
