//! The editor's requests over the cluster's `xmip.toml` (ADR-0064,
//! amendment 2026-10-13: *Create, Edit, Delete. Suggestions, Lookups,
//! Validations*), each answered from the runtime's reading of the document
//! ([`Model`]) and made, where it changes the text, by the runtime's one
//! cluster edit:
//!
//! - suggestions, `textDocument/completion` (`completion.rs`, with a
//!   Location's technologies and settings from `settings.rs`);
//! - lookups: `textDocument/definition`, `references`, `hover`,
//!   `documentSymbol`, `prepareRename` and `rename` (`lookup.rs`);
//! - create, rename and delete: `textDocument/codeAction` and the server's
//!   commands through `workspace/executeCommand`, with `xmip/places` and
//!   `xmip/entries` for the extension's commands to choose from
//!   (`action.rs`);
//! - quick fixes for what the runtime's validation reports (`quickfix.rs`).

use serde_json::{Value, json};

use crate::designer::{Unanswered, text_edit};
use crate::model::Model;
use crate::rpc::{error, response};
use crate::runtime::Runtime;
use crate::server::{INVALID_PARAMS, NOT_NAMED, REFUSED, RUNTIME_UNAVAILABLE, Server, is_toml};
use crate::text::offset;
use crate::{action, completion, lookup, quickfix, settings};

const COMPLETION: &str = "textDocument/completion";
const HOVER: &str = "textDocument/hover";
const DEFINITION: &str = "textDocument/definition";
const REFERENCES: &str = "textDocument/references";
const SYMBOLS: &str = "textDocument/documentSymbol";
const PREPARE_RENAME: &str = "textDocument/prepareRename";
const RENAME: &str = "textDocument/rename";
const CODE_ACTION: &str = "textDocument/codeAction";
/// The protocol's request to run one of the server's commands.
pub const EXECUTE: &str = "workspace/executeCommand";
const PLACES: &str = "xmip/places";
const ENTRIES: &str = "xmip/entries";

/// The methods this file answers.
pub const METHODS: [&str; 11] = [
    COMPLETION,
    HOVER,
    DEFINITION,
    REFERENCES,
    SYMBOLS,
    PREPARE_RENAME,
    RENAME,
    CODE_ACTION,
    EXECUTE,
    PLACES,
    ENTRIES,
];

/// What the server tells the client it answers of these.
#[must_use]
pub fn capabilities() -> Value {
    json!({
        "completionProvider": { "triggerCharacters": ["\"", "[", ".", "=", "{", ","] },
        "hoverProvider": true,
        "definitionProvider": true,
        "referencesProvider": true,
        "documentSymbolProvider": true,
        "renameProvider": { "prepareProvider": true },
        "codeActionProvider": { "codeActionKinds": ["quickfix", "refactor"] },
        "executeCommandProvider": { "commands": action::COMMANDS },
    })
}

/// What `method` answers when the document cannot be read: nothing, for
/// what the editor asks as the cursor moves; `None` for what a developer
/// asked for, which says why.
#[must_use]
pub fn quiet(method: &str) -> Option<Value> {
    match method {
        COMPLETION | REFERENCES | SYMBOLS | CODE_ACTION | PLACES | ENTRIES => Some(json!([])),
        HOVER | DEFINITION | PREPARE_RENAME => Some(Value::Null),
        _ => None,
    }
}

/// The document `method` is over: its uri, from the arguments of a command.
#[must_use]
pub fn uri(method: &str, params: &Value) -> String {
    let at = if method == EXECUTE {
        &params["arguments"][0]["uri"]
    } else {
        &params["textDocument"]["uri"]
    };
    at.as_str().unwrap_or_default().to_string()
}

/// A command's text with its change made, for the server to send.
pub struct Changed {
    pub edit: Value,
    pub label: String,
}

/// Answer `method` over `model`, the document at `uri`.
///
/// # Errors
/// [`Unanswered`], saying why.
pub fn answer(
    runtime: &Runtime,
    model: &Model,
    method: &str,
    params: &Value,
    uri: &str,
) -> Result<(Value, Option<Changed>), Unanswered> {
    let at = offset(&model.text, &params["position"])
        .or_else(|| offset(&model.text, &params["range"]["start"]))
        .unwrap_or_default();
    let answered = match method {
        COMPLETION => {
            let mut items = settings::answer(
                runtime,
                COMPLETION,
                params,
                Some(&model.text),
                model.units(),
            )
            .ok()
            .and_then(|items| items.as_array().cloned())
            .unwrap_or_default();
            let roles = || runtime.role_words().unwrap_or_default();
            items.extend(completion::completion(model, at, &roles));
            json!(items)
        }
        HOVER => match settings::answer(runtime, HOVER, params, Some(&model.text), model.units()) {
            Ok(said) if !said.is_null() => said,
            _ => lookup::hover(model, at).unwrap_or(Value::Null),
        },
        DEFINITION => lookup::definition(model, uri, at),
        REFERENCES => {
            let declaration = params["context"]["includeDeclaration"] == true;
            lookup::references(model, uri, at, declaration)
        }
        SYMBOLS => lookup::symbols(model),
        PREPARE_RENAME => lookup::prepare_rename(model, at),
        RENAME => {
            let name = params["newName"].as_str().unwrap_or_default();
            lookup::rename(runtime, model, uri, at, name)?
        }
        CODE_ACTION => {
            let diagnostics = params["context"]["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut actions = quickfix::fixes(runtime, model, uri, &diagnostics);
            actions.extend(action::at(model, uri, at));
            json!(actions)
        }
        PLACES => json!(action::places(model, uri)),
        ENTRIES => json!(action::entries(model, uri)),
        EXECUTE => {
            let command = params["command"].as_str().unwrap_or_default();
            let edited = action::run(runtime, &model.text, command, &params["arguments"][0])?;
            let edit = json!({ "changes": { uri: [text_edit(&model.text, &edited)] } });
            let label = format!("Xmip: {command}");
            return Ok((Value::Null, Some(Changed { edit, label })));
        }
        _ => {
            return Err(Unanswered::Params(format!(
                "{method} is not a language request"
            )));
        }
    };
    Ok((answered, None))
}

/// Whether `method` reads the document around the cursor even while the
/// line being typed does not read: a suggestion.
#[must_use]
pub fn typed(method: &str) -> bool {
    method == COMPLETION
}

/// The server's side of the editor's requests: which document, its reading
/// kept until its text changes, and what goes back to the client.
impl Server {
    /// The editor's requests over a document (`language.rs`, ADR-0064
    /// amendment 2026-10-13), over the runtime's reading of it; a command
    /// that changes the text asks the client to apply the edit, and one the
    /// runtime refuses says why.
    pub(crate) fn language_request(
        &mut self,
        method: &str,
        id: Option<&Value>,
        params: &Value,
        out: &mut Vec<Value>,
    ) {
        let uri = uri(method, params);
        let text = self.documents.get(&uri).cloned().filter(|_| is_toml(&uri));
        let answered = match (text, self.runtime()) {
            (None, _) => Err(Unanswered::Params(format!(
                "{uri} is not an open TOML document"
            ))),
            (Some(_), Err(reason)) => Err(Unanswered::Unavailable(reason)),
            (Some(text), Ok(_)) => self.read(&uri, &text, method, params).and_then(|()| {
                match (self.runtime.as_ref(), self.models.get(&uri)) {
                    (Some(runtime), Some(model)) => answer(runtime, model, method, params, &uri),
                    _ => Err(Unanswered::Unavailable(NOT_NAMED.into())),
                }
            }),
        };
        match (answered, quiet(method)) {
            (Ok((result, changed)), _) => {
                if let Some(changed) = changed {
                    self.asked += 1;
                    out.push(
                        json!({ "jsonrpc": "2.0", "id": format!("xmip-lsp-{}", self.asked),
                        "method": "workspace/applyEdit",
                        "params": { "label": changed.label, "edit": changed.edit } }),
                    );
                }
                out.push(response(id, &result));
            }
            (Err(_), Some(nothing)) => out.push(response(id, &nothing)),
            (Err(Unanswered::Refused(said)), None) if method == EXECUTE => {
                out.push(json!({ "jsonrpc": "2.0", "method": "window/showMessage",
                    "params": { "type": 1, "message": format!("Xmip: {said}") } }));
                out.push(response(id, &Value::Null));
            }
            (Err(Unanswered::Params(reason)), None) => out.push(error(id, INVALID_PARAMS, &reason)),
            (Err(Unanswered::Refused(reason)), None) => out.push(error(id, REFUSED, &reason)),
            (Err(Unanswered::Unavailable(reason)), None) => {
                out.push(error(id, RUNTIME_UNAVAILABLE, &reason));
            }
        }
    }

    /// The runtime's reading of the document at `uri`, kept until its text
    /// changes; a suggestion reads around the line being typed.
    fn read(
        &mut self,
        uri: &str,
        text: &str,
        method: &str,
        params: &Value,
    ) -> Result<(), Unanswered> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or(Unanswered::Unavailable(NOT_NAMED.into()))?;
        let current = self.models.get(uri).is_some_and(|model| model.text == text);
        if !current {
            let model = if typed(method) {
                let at = crate::text::offset(text, &params["position"]).unwrap_or_default();
                Model::read_around(runtime, text, at)?
            } else {
                Model::read(runtime, text)?
            };
            self.models.insert(uri.to_string(), model);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::list;
    use crate::text::position;

    const URI: &str = "file:///c:/cluster/xmip.toml";

    /// The test cluster, an Xmip Application added as its section and
    /// bound, its Receive Location taken by the cluster's first node; the
    /// runtime, and that node's name.
    fn cluster() -> Option<(Runtime, String, String)> {
        let path = crate::built::runtime_library()?;
        let runtime = Runtime::load(&path).expect("the built runtime loads");
        let base = crate::built::test_cluster();
        let read = Model::read(&runtime, &base).expect("the test cluster reads");
        let first = read
            .entries()
            .into_iter()
            .find(|entry| entry.kind == "node")
            .map(|entry| entry.name.to_string())
            .expect("a node");
        let text = format!(
            "{base}\n[[xmip_applications]]\nname = \"Orders\"\n\n\
             [[xmip_applications.receive_ports]]\nname = \"Orders\"\n\n\
             [[xmip_applications.receive_locations]]\nname = \"OrdersIn\"\n\
             receive_port = \"Orders\"\n\n\
             [[xmip_applications.send_ports]]\nname = \"Billing\"\n\n\
             [[xmip_applications.subscriptions]]\nid = \"billing\"\n\
             destination = {{ send-port = \"Billing\" }}\n\
             filter = \"MessageType = 'Order'\"\n\n\
             [[applications]]\nname = \"Orders\"\n\n\
             [[applications.receive_locations]]\nname = \"OrdersIn\"\nnode = \"{first}\"\n\
             start = true\ntransport = \"xmip-core-transport-file\"\naddress = \"/in\"\n"
        );
        Some((runtime, text, first))
    }

    /// `method` asked at the byte `at`, with `more` among its parameters.
    fn ask(runtime: &Runtime, text: &str, method: &str, at: usize, more: &Value) -> Value {
        let model = Model::read_around(runtime, text, at).expect("reads");
        let mut params = json!({ "textDocument": { "uri": URI }, "position": position(text, at) });
        if let (Some(params), Some(more)) = (params.as_object_mut(), more.as_object()) {
            params.extend(more.clone());
        }
        answer(runtime, &model, method, &params, URI)
            .expect("answered")
            .0
    }

    /// The byte `skip` bytes into the first `needle` of `text`.
    fn at(text: &str, needle: &str, skip: usize) -> usize {
        text.find(needle).unwrap_or_else(|| panic!("no {needle}")) + skip
    }

    fn labels(items: &Value, field: &str) -> Vec<String> {
        list(items)
            .iter()
            .filter_map(|item| item[field].as_str().map(str::to_string))
            .collect()
    }

    /// The one text edit of a workspace edit, applied.
    fn applied(text: &str, edit: &Value) -> String {
        let one = &edit["changes"][URI][0];
        let from = offset(text, &one["range"]["start"]).expect("a start");
        let to = offset(text, &one["range"]["end"]).expect("an end");
        let new = one["newText"].as_str().expect("text");
        format!("{}{new}{}", &text[..from], &text[to..])
    }

    fn command(
        runtime: &Runtime,
        text: &str,
        name: &str,
        arguments: &Value,
    ) -> Result<String, Unanswered> {
        let model = Model::read(runtime, text).expect("reads");
        let params = json!({ "command": name, "arguments": [arguments] });
        let (_, changed) = answer(runtime, &model, EXECUTE, &params, URI)?;
        Ok(applied(text, &changed.expect("a change").edit))
    }

    #[test]
    fn keys_values_names_cases_roles_and_headers_are_suggested() {
        let Some((runtime, text, _)) = cluster() else {
            return;
        };
        let suggested = |text: &str, at: usize| {
            labels(&ask(&runtime, text, COMPLETION, at, &json!({})), "label")
        };

        let blank = at(&text, "\n\n[[xmip_applications.send_ports]]", 1);
        let keys = suggested(&text, blank);
        assert!(keys.contains(&"interaction".to_string()), "{keys:?}");
        assert!(
            !keys.contains(&"receive_port".to_string()),
            "written: {keys:?}"
        );

        let open = text.replace("receive_port = \"Orders\"", "receive_port = \"\"");
        assert_eq!(
            suggested(&open, at(&open, "receive_port = \"", 16)),
            ["Orders"]
        );

        let case = text.replace("{ send-port = \"Billing\" }", "{ send-port = \"\" }");
        assert_eq!(
            suggested(&case, at(&case, "send-port = \"", 13)),
            ["Billing"]
        );

        let roles = at(&text, "roles = \"", 9);
        let written = text[roles..].split('"').next().unwrap_or_default();
        let words = suggested(&text, roles);
        assert!(
            words.iter().any(|word| written.contains(word.as_str())),
            "{words:?}"
        );

        let typed = format!("{text}[[xmip_app");
        let headers = suggested(&typed, typed.len());
        let wanted = "[[xmip_applications.send_ports]]".to_string();
        assert!(headers.contains(&wanted), "{headers:?}");
    }

    #[test]
    fn a_reference_goes_to_its_definition_and_a_declaration_finds_its_references() {
        let Some((runtime, text, first)) = cluster() else {
            return;
        };
        let on_destination = at(&text, "send-port = \"Billing", 15);

        let found = ask(&runtime, &text, DEFINITION, on_destination, &json!({}));
        let declared = position(&text, at(&text, "name = \"Billing", 8));
        assert_eq!(found["range"]["start"], declared);

        let more = json!({ "context": { "includeDeclaration": true } });
        let on_name = at(&text, "name = \"Billing", 9);
        let named = ask(&runtime, &text, REFERENCES, on_name, &more);
        assert_eq!(list(&named).len(), 2, "{named}");

        let on_node = at(&text, &format!("node = \"{first}"), 8);
        let node = ask(&runtime, &text, DEFINITION, on_node, &json!({}));
        assert_eq!(node["uri"], URI, "{node}");
    }

    #[test]
    fn a_hover_explains_a_key_a_reference_and_an_entry() {
        let Some((runtime, text, _)) = cluster() else {
            return;
        };
        let said = |at: usize| {
            let hover = ask(&runtime, &text, HOVER, at, &json!({}));
            hover["contents"]["value"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        };

        let key = said(at(&text, "receive_port = ", 2));
        assert!(key.contains("Names a Receive Port"), "{key}");
        let reference = said(at(&text, "send-port = \"Billing", 15));
        assert!(reference.contains("declared at"), "{reference}");
        let entry = said(at(&text, "name = \"Billing", 9));
        assert!(entry.contains("Send Port"), "{entry}");
    }

    #[test]
    fn the_outline_holds_each_entry_within_its_application() {
        let Some((runtime, text, _)) = cluster() else {
            return;
        };
        let symbols = ask(&runtime, &text, SYMBOLS, 0, &json!({}));

        let orders = list(&symbols)
            .iter()
            .find(|symbol| {
                symbol["name"] == "Orders"
                    && symbol["detail"]
                        .as_str()
                        .is_some_and(|d| d.contains("Route"))
            })
            .expect("the Application");
        let children = labels(&orders["children"], "name");
        assert!(children.contains(&"Billing".to_string()), "{orders}");
    }

    #[test]
    fn a_rename_from_a_reference_renames_the_declaration_and_every_name() {
        let Some((runtime, text, _)) = cluster() else {
            return;
        };
        let on = at(&text, "send-port = \"Billing", 15);

        let prepared = ask(&runtime, &text, PREPARE_RENAME, on, &json!({}));
        assert_eq!(prepared["placeholder"], "Billing");
        let renamed = ask(&runtime, &text, RENAME, on, &json!({ "newName": "Ledger" }));
        let expected = text.replace("\"Billing\"", "\"Ledger\"");
        assert_eq!(applied(&text, &renamed), expected);
    }

    #[test]
    fn delete_is_refused_while_named_or_takes_its_references_and_add_makes_entries() {
        let Some((runtime, text, _)) = cluster() else {
            return;
        };
        let context = json!({ "context": { "diagnostics": [] } });
        let on_port = at(&text, "name = \"Billing", 9);
        let titles = labels(
            &ask(&runtime, &text, CODE_ACTION, on_port, &context),
            "title",
        );
        let referenced = "Delete Send Port 'Billing' and";
        assert!(
            titles.iter().any(|t| t.starts_with(referenced)),
            "{titles:?}"
        );
        let subscription = "Add Subscription to Orders".to_string();
        assert!(titles.contains(&subscription), "{titles:?}");

        let port = json!(["xmip_applications", "Orders", "send_ports", "Billing"]);
        let alone = json!({ "uri": URI, "edit": { "remove-entry": { "section": port } } });
        let refused = command(&runtime, &text, action::EDIT, &alone).err();
        let said = match refused {
            Some(Unanswered::Refused(said)) => said,
            other => panic!("not refused: {other:?}"),
        };
        assert!(said.contains("subscriptions.billing"), "{said}");
        let with = json!({ "uri": URI,
            "edit": { "remove-entry": { "section": port, "references": true } } });
        let removed = command(&runtime, &text, action::EDIT, &with).expect("removed");
        assert!(!removed.contains("Billing"), "{removed}");
        assert!(!removed.contains("id = \"billing\""), "{removed}");

        let ports = json!(["xmip_applications", "Orders", "send_ports"]);
        let add = json!({ "uri": URI, "section": ports, "name": "Ledger", "answers": [] });
        let added = command(&runtime, &text, action::ADD, &add).expect("added");
        let declared = "[[xmip_applications.send_ports]]\nname = \"Ledger\"";
        assert!(added.contains(declared), "{added}");
        let routed = json!({ "uri": URI, "subscription": "Orders", "name": "ledger",
            "answers": [{ "key": "destination", "value": "send-port:Billing" }] });
        let subscribed = command(&runtime, &text, action::ADD, &routed).expect("subscribed");
        assert!(subscribed.contains("id = \"ledger\""), "{subscribed}");
    }

    #[test]
    fn a_misspelled_key_and_a_dangling_name_each_have_their_fix() {
        let Some((runtime, text, _)) = cluster() else {
            return;
        };
        // A key spelled wrong on purpose: receive with the i and e swapped.
        let wrong = "receive_port".replacen("ei", "ie", 1);
        let broken = text
            .replace("receive_port = ", &format!("{wrong} = "))
            .replace("send-port = \"Billing\"", "send-port = \"Biling\"");
        let unknown = format!("unknown field `{wrong}`, expected one of `name`, `receive_port`");
        let diagnostics = json!([
            { "message": unknown },
            { "message": "Subscription 'billing' routes to the Send Port 'Biling', \
                          which the Application does not declare" },
        ]);
        let context = json!({ "context": { "diagnostics": diagnostics } });
        let actions = ask(&runtime, &broken, CODE_ACTION, 0, &context);
        let fix = |words: &str| {
            list(&actions)
                .iter()
                .find(|a| {
                    a["kind"] == "quickfix"
                        && a["title"].as_str().is_some_and(|t| t.contains(words))
                })
                .map_or_else(
                    || panic!("no fix with {words}: {actions}"),
                    |a| a["edit"].clone(),
                )
        };
        let _offered = fix("Send Port Billing");
        let once = applied(&broken, &fix("to receive_port"));
        let model = Model::read(&runtime, &once).expect("reads");
        let again = answer(
            &runtime,
            &model,
            CODE_ACTION,
            &json!({
            "textDocument": { "uri": URI }, "range": { "start": position(&once, 0) },
            "context": { "diagnostics": [diagnostics[1]] } }),
            URI,
        )
        .expect("answered")
        .0;
        let name = list(&again)
            .iter()
            .find(|a| {
                a["title"]
                    .as_str()
                    .is_some_and(|t| t.contains("Send Port Billing"))
            })
            .map(|a| a["edit"].clone())
            .expect("the name's fix");
        assert_eq!(applied(&once, &name), text);
    }

    #[test]
    fn an_accepted_party_and_an_attached_call_are_suggested_by_name() {
        let Some((runtime, text, first)) = cluster() else {
            return;
        };
        let text = format!(
            "[[parties]]\nid = \"0192f000-0000-7000-8000-000000000001\"\nname = \"partner\"\n\
             identities = []\n\n{text}\n[[nodes.{first}.receive_locations]]\nname = \"in\"\n\
             transport = \"t\"\naddress = \"a\"\n\
             [nodes.{first}.receive_locations.accept]\nparty = [\"\"]\n"
        );
        let suggested = labels(
            &ask(
                &runtime,
                &text,
                COMPLETION,
                at(&text, "party = [\"", 10),
                &json!({}),
            ),
            "label",
        );
        assert_eq!(suggested, ["partner"]);
    }
}
