//! Create, rename and delete in the cluster's `xmip.toml` (ADR-0064,
//! amendment 2026-10-13): the code actions where the cursor is, the
//! commands they run, and the lists the extension's commands choose from.
//!
//! Every change is one of `xmip_cluster_edit_v1`'s (`edit.rs`), made by the
//! runtime: an entry added to a list the views say may hold one, a node
//! declared, a Subscription added to an Application by its own edit, an
//! entry renamed with every name that stands for it, an entry removed —
//! refused while anything names it, saying what does, or removed with what
//! names it. The extension asks the developer the questions this sends and
//! sends the answers back; it writes nothing itself.

use serde_json::{Value, json};

use crate::designer::Unanswered;
use crate::edit;
use crate::model::{Entry, Model, list, strings};
use crate::runtime::Runtime;

/// The server's commands, run through `workspace/executeCommand`.
pub const EDIT: &str = "xmip.server.edit";
pub const ADD: &str = "xmip.server.add";
pub const RENAME: &str = "xmip.server.rename";
pub const COMMANDS: [&str; 3] = [EDIT, ADD, RENAME];

/// The extension's commands, which ask the developer and then run one of
/// the server's.
const ASK_ADD: &str = "xmip.add";
const ASK_RENAME: &str = "xmip.rename";

/// The actions on the entry at `offset`: add beside it or within it,
/// rename it, delete it alone or with what names it.
#[must_use]
pub fn at(model: &Model, uri: &str, offset: usize) -> Vec<Value> {
    let section = model
        .table_at(offset)
        .map(|table| strings(&table["section"]))
        .unwrap_or_default();
    let mut actions = Vec::new();
    if let Some(entry) = model
        .owner(&section)
        .filter(|entry| entry.section.len() > 1)
    {
        let what = format!("{} '{}'", entry.title, entry.name);
        actions.push(action(
            &format!("Rename {what}"),
            ASK_RENAME,
            &rename_arguments(uri, &entry),
        ));
        actions.extend(
            deletions(model, uri, &entry)
                .into_iter()
                .map(|(title, arguments)| action(&title, EDIT, &arguments)),
        );
    }
    let holder = places(model, uri)
        .into_iter()
        .filter(|place| section.starts_with(&strings(&place["holder"])))
        .max_by_key(|place| list(&place["holder"]).len())
        .map(|place| strings(&place["holder"]));
    for place in places(model, uri) {
        if holder.as_ref() == Some(&strings(&place["holder"])) {
            let title = place["label"].as_str().unwrap_or_default();
            actions.push(action(title, ASK_ADD, &place["arguments"]));
        }
    }
    actions
}

fn action(title: &str, command: &str, arguments: &Value) -> Value {
    json!({
        "title": title,
        "kind": "refactor",
        "command": { "title": title, "command": command, "arguments": [arguments] },
    })
}

fn rename_arguments(uri: &str, entry: &Entry<'_>) -> Value {
    json!({ "uri": uri, "section": entry.section, "name": entry.name, "title": entry.title })
}

/// Delete `entry`; and, where anything names it, delete it with those.
fn deletions(model: &Model, uri: &str, entry: &Entry<'_>) -> Vec<(String, Value)> {
    let what = format!("{} '{}'", entry.title, entry.name);
    let mut found = vec![(
        format!("Delete {what}"),
        json!({ "uri": uri, "edit": edit::remove(&entry.section, false) }),
    )];
    let named = model.referring(&entry.section).len();
    if named > 0 {
        found.push((
            format!("Delete {what} and the {named} name(s) that stand for it"),
            json!({ "uri": uri, "edit": edit::remove(&entry.section, true) }),
        ));
    }
    found
}

/// Every place a new entry may be added: each list the views say may hold
/// one, each Application's Subscriptions, and a new node. Each with what
/// the extension asks before it adds one.
#[must_use]
pub fn places(model: &Model, uri: &str) -> Vec<Value> {
    let mut found = vec![json!({
        "label": "Declare a node", "holder": [],
        "arguments": { "uri": uri, "node": true, "title": "node", "questions": [] },
    })];
    for view in list(&model.answer["views"]) {
        let title = view["title"].as_str().unwrap_or_default();
        for place in list(&view["places"]) {
            let section = strings(&place["section"]);
            let holder = section[..section.len().saturating_sub(1)].to_vec();
            let label = format!(
                "Add {title} to {}",
                place["scope"].as_str().unwrap_or_default()
            );
            let questions = questions(model, &section);
            let arguments =
                json!({ "uri": uri, "section": section, "title": title, "questions": questions });
            found.push(json!({ "label": label, "holder": holder, "arguments": arguments }));
        }
        for entry in list(&view["entries"])
            .iter()
            .filter(|entry| !entry["routes"].is_null())
        {
            let application = entry["name"].as_str().unwrap_or_default();
            let options: Vec<Value> = list(&entry["routes"]["nodes"])
                .iter()
                .filter(|node| node["target"] == true)
                .map(|node| {
                    let kind = node["kind"].as_str().unwrap_or_default();
                    let name = node["name"].as_str().unwrap_or_default();
                    json!({ "label": format!("{kind} {name}"), "value": node["id"] })
                })
                .collect();
            let question = json!({ "key": "destination", "kind": "target", "options": options });
            let arguments = json!({
                "uri": uri, "subscription": application, "title": "Subscription",
                "questions": [question],
            });
            found.push(json!({
                "label": format!("Add Subscription to {application}"),
                "holder": strings(&entry["section"]),
                "arguments": arguments,
            }));
        }
    }
    found
}

/// What a new entry of the list at `section` cannot be without, beyond its
/// name: each required key, its kind and the words it reads.
fn questions(model: &Model, section: &[String]) -> Vec<Value> {
    let Some((list_key, holder)) = section.split_last() else {
        return Vec::new();
    };
    let held = model
        .table(holder)
        .map(|table| strings(&table["table"]))
        .unwrap_or_default();
    let table = [held, vec![list_key.clone()]].concat();
    model
        .keys_of(&table)
        .into_iter()
        .filter(|key| {
            key["required"] == true && !matches!(key["key"].as_str(), Some("name" | "id"))
        })
        .map(|key| {
            let mut options = strings(&key["choices"]);
            if key["kind"] == "boolean" {
                options = vec!["true".to_string(), "false".to_string()];
            }
            let options: Vec<Value> = options
                .iter()
                .map(|o| json!({ "label": o, "value": o }))
                .collect();
            json!({ "key": key["key"], "kind": key["kind"], "options": options })
        })
        .collect()
}

/// Every entry that may be deleted, with the choices deleting it offers.
#[must_use]
pub fn entries(model: &Model, uri: &str) -> Vec<Value> {
    model
        .entries()
        .into_iter()
        .filter(|entry| entry.section.len() > 1)
        .map(|entry| {
            let choices: Vec<Value> = deletions(model, uri, &entry)
                .into_iter()
                .map(|(title, arguments)| {
                    json!({ "title": title, "command": EDIT, "arguments": arguments })
                })
                .collect();
            json!({
                "label": format!("{} '{}'", entry.title, entry.name),
                "description": entry.section.join("."),
                "rename": rename_arguments(uri, &entry),
                "choices": choices,
            })
        })
        .collect()
}

/// The text a server command leaves the document in.
///
/// # Errors
/// Arguments the command does not take, or the runtime's refusal.
pub fn run(
    runtime: &Runtime,
    text: &str,
    command: &str,
    arguments: &Value,
) -> Result<String, Unanswered> {
    let wanted = |key: &str| Unanswered::Params(format!("{command} needs {key}"));
    let change = match command {
        EDIT => arguments
            .get("edit")
            .cloned()
            .ok_or_else(|| wanted("an edit"))?,
        RENAME => {
            let name = arguments["name"].as_str().ok_or_else(|| wanted("a name"))?;
            edit::rename(&strings(&arguments["section"]), name)
        }
        ADD => added(arguments).map_err(Unanswered::Refused)?,
        _ => {
            return Err(Unanswered::Params(format!(
                "{command} is not a command xmip-lsp runs"
            )));
        }
    };
    edit::made(runtime, text, &change)
}

/// The edit adding what `arguments` say: a node, a Subscription, or an
/// entry with the developer's answers.
fn added(arguments: &Value) -> Result<Value, String> {
    let name = arguments["name"].as_str().unwrap_or_default();
    let answers = list(&arguments["answers"]);
    if arguments["node"] == true {
        return Ok(edit::add_node(name));
    }
    if let Some(application) = arguments["subscription"].as_str() {
        let target = answers
            .first()
            .and_then(|answer| answer["value"].as_str())
            .unwrap_or_default();
        return Ok(json!({ "application": { "application": application,
            "edit": { "add-subscription": { "id": name, "target": target } } } }));
    }
    let values = answers
        .iter()
        .map(|answer| {
            let kind = answer["kind"].as_str().unwrap_or_default();
            let literal = edit::literal(kind, answer["value"].as_str().unwrap_or_default())
                .map_err(|refused| match refused {
                    Unanswered::Refused(said)
                    | Unanswered::Params(said)
                    | Unanswered::Unavailable(said) => said,
                })?;
            Ok(json!({ "key": [answer["key"]], "value": literal }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(edit::add_entry(
        &strings(&arguments["section"]),
        name,
        &values,
    ))
}
