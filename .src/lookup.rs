//! Lookups in the cluster's `xmip.toml` (ADR-0064, amendment 2026-10-13):
//! go to a definition, find every reference, hover on a key, a name or a
//! reference, the outline, and a rename across the file.
//!
//! What names what, where each part is written and what each key is are the
//! runtime's answer ([`Model`]); a rename is `xmip_cluster_edit_v1`'s
//! `rename`, which renames the entry and every name that stands for it. What
//! is here is the protocol's shapes around them.

use serde_json::{Value, json};

use crate::designer::{Unanswered, text_edit};
use crate::edit;
use crate::model::{Entry, Model, bytes, list, strings};
use crate::runtime::Runtime;
use crate::text::{holds, range, unquoted};

/// Where the definition of what is at `offset` is: the entry a reference
/// names, or the entry itself on its name.
#[must_use]
pub fn definition(model: &Model, uri: &str, offset: usize) -> Value {
    let section = target_at(model, offset);
    section
        .and_then(|section| model.name_of(&section))
        .map_or(Value::Null, |at| location(model, uri, at))
}

/// Every place that names what is at `offset`, and its declaration when
/// `declaration`.
#[must_use]
pub fn references(model: &Model, uri: &str, offset: usize, declaration: bool) -> Value {
    let Some(section) = target_at(model, offset) else {
        return json!([]);
    };
    let mut found: Vec<Value> = model
        .referring(&section)
        .into_iter()
        .filter_map(|reference| model.value_of(reference))
        .map(|at| location(model, uri, at))
        .collect();
    if declaration && let Some(at) = model.name_of(&section) {
        found.insert(0, location(model, uri, at));
    }
    json!(found)
}

/// The section of what is at `offset`: what a reference there names, or
/// the entry whose name or header is there.
fn target_at(model: &Model, offset: usize) -> Option<Vec<String>> {
    if let Some((reference, _)) = model.reference_at(offset) {
        let target = strings(&reference["target"]);
        return (!target.is_empty()).then_some(target);
    }
    model.declaration_at(offset).map(|entry| entry.section)
}

/// What the hover at `offset` says, where it says anything.
#[must_use]
pub fn hover(model: &Model, offset: usize) -> Option<Value> {
    let said = if let Some((reference, _)) = model.reference_at(offset) {
        referenced(model, reference)
    } else if let Some(entry) = model.declaration_at(offset) {
        declared(model, &entry)
    } else {
        key_at(model, offset)?
    };
    Some(json!({ "contents": { "kind": "markdown", "value": said } }))
}

fn referenced(model: &Model, reference: &Value) -> String {
    let names = reference["names"].as_str().unwrap_or_default();
    let name = reference["name"].as_str().unwrap_or_default();
    let target = strings(&reference["target"]);
    if let Some(entry) = model.entry(&target) {
        let at = target.join(".");
        format!(
            "**{names}** `{name}`, declared at `{at}`\n\n{}",
            fields(&entry)
        )
    } else {
        let choices = strings(&reference["choices"]).join("`, `");
        format!("No {names} named `{name}` is declared here.\n\nDeclared: `{choices}`")
    }
}

fn declared(model: &Model, entry: &Entry<'_>) -> String {
    let named = model.referring(&entry.section).len();
    format!(
        "**{}** `{}` — {}\n\nNamed {named} time(s) elsewhere in the file.\n\n{}",
        entry.title,
        entry.name,
        entry.scope,
        fields(entry)
    )
}

fn fields(entry: &Entry<'_>) -> String {
    list(&entry.value["fields"])
        .iter()
        .map(|field| {
            let key = strings(&field["key"]).join(".");
            format!(
                "- `{key} = {}`",
                field["value"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the key under `offset` is, from the keys its table may hold.
fn key_at(model: &Model, offset: usize) -> Option<String> {
    let table = model.table_at(offset)?;
    let written = list(&table["keys"]).iter().find(|key| {
        bytes(&key["at"]).is_some_and(|at| holds(at, offset)) && key["index"].is_null()
    })?;
    let path = strings(&written["key"]);
    let (name, within) = path.split_last()?;
    let header = [strings(&table["table"]), within.to_vec()].concat();
    if header.last().map(String::as_str) == Some("tuning") {
        let setting = list(&model.tuning()["settings"])
            .iter()
            .find(|s| s["name"] == name.as_str())?;
        let meaning = setting["meaning"].as_str().unwrap_or_default();
        return Some(format!("**{name}** — {}\n\n{meaning}", setting["kind"]));
    }
    let key = model
        .keys_of(&header)
        .into_iter()
        .find(|key| key["key"] == name.as_str())?;
    let kind = key["kind"].as_str().unwrap_or_default();
    let required = if key["required"] == true {
        ", required"
    } else {
        ""
    };
    let choices = strings(&key["choices"]);
    let choices = if choices.is_empty() {
        String::new()
    } else {
        format!("\n\nOne of `{}`", choices.join("`, `"))
    };
    let section = strings(&table["section"]);
    let names = model
        .references()
        .iter()
        .find(|r| strings(&r["holder"]) == section && strings(&r["key"]) == path)
        .map(|r| format!("\n\nNames a {}.", r["names"].as_str().unwrap_or_default()))
        .unwrap_or_default();
    let header = header.join(".");
    Some(format!(
        "**{name}** — {kind}{required}{choices}{names}\n\nIn `[{header}]`."
    ))
}

/// The outline: every entry, each within the entry it belongs to.
#[must_use]
pub fn symbols(model: &Model) -> Value {
    let entries = model.entries();
    let roots: Vec<&Entry<'_>> = entries
        .iter()
        .filter(|entry| parent(&entries, entry).is_none())
        .collect();
    json!(
        roots
            .into_iter()
            .map(|entry| symbol(model, &entries, entry))
            .collect::<Vec<_>>()
    )
}

fn parent<'e>(entries: &'e [Entry<'e>], entry: &Entry<'_>) -> Option<&'e Entry<'e>> {
    entries
        .iter()
        .filter(|other| other.section.len() < entry.section.len())
        .filter(|other| entry.section.starts_with(&other.section))
        .max_by_key(|other| other.section.len())
}

fn symbol(model: &Model, entries: &[Entry<'_>], entry: &Entry<'_>) -> Value {
    let children: Vec<Value> = entries
        .iter()
        .filter(|child| parent(entries, child).is_some_and(|p| p.section == entry.section))
        .map(|child| symbol(model, entries, child))
        .collect();
    let extent = |section: &[String]| model.table(section).and_then(|w| bytes(&w["extent"]));
    let mut whole = extent(&entry.section).unwrap_or([0, 0]);
    for inner in entries
        .iter()
        .filter(|e| e.section.starts_with(&entry.section))
    {
        if let Some([start, end]) = extent(&inner.section).filter(|at| at[0] < at[1]) {
            whole = [whole[0].min(start), whole[1].max(end)];
        }
    }
    let chosen = model
        .name_of(&entry.section)
        .filter(|at| whole[0] <= at[0] && at[1] <= whole[1])
        .unwrap_or([whole[0], whole[0]]);
    let kind = match entry.kind {
        "route" => 2,
        "node" => 5,
        "cluster" => 19,
        "subscription" => 24,
        _ => 23,
    };
    json!({
        "name": if entry.name.is_empty() { "(unnamed)" } else { entry.name },
        "detail": format!("{} · {}", entry.title, entry.scope),
        "kind": kind,
        "range": range(&model.text, whole),
        "selectionRange": range(&model.text, chosen),
        "children": children,
    })
}

/// What a rename at `offset` renames: its section and the name's bytes.
fn renamed_at(model: &Model, offset: usize) -> Option<(Vec<String>, [usize; 2])> {
    if let Some((reference, at)) = model.reference_at(offset) {
        let target = strings(&reference["target"]);
        return (!target.is_empty()).then(|| (target, unquoted(&model.text, at)));
    }
    let entry = model.declaration_at(offset)?;
    let at = bytes(&model.table(&entry.section)?["name"])?;
    (entry.section.len() > 1 && holds(at, offset))
        .then(|| (entry.section, unquoted(&model.text, at)))
}

/// `textDocument/prepareRename`: the name under `offset`, if it renames.
#[must_use]
pub fn prepare_rename(model: &Model, offset: usize) -> Value {
    renamed_at(model, offset).map_or(Value::Null, |(_, at)| {
        json!({ "range": range(&model.text, at), "placeholder": &model.text[at[0]..at[1]] })
    })
}

/// `textDocument/rename`: the runtime's rename, as the one text edit.
///
/// # Errors
/// Nothing renames at `offset`, or the runtime refused the name.
pub fn rename(
    runtime: &Runtime,
    model: &Model,
    uri: &str,
    offset: usize,
    name: &str,
) -> Result<Value, Unanswered> {
    let (section, _) = renamed_at(model, offset)
        .ok_or_else(|| Unanswered::Refused("nothing to rename here".to_string()))?;
    let edited = edit::made(runtime, &model.text, &edit::rename(&section, name))?;
    Ok(json!({ "changes": { uri: [text_edit(&model.text, &edited)] } }))
}

fn location(model: &Model, uri: &str, at: [usize; 2]) -> Value {
    json!({ "uri": uri, "range": range(&model.text, unquoted(&model.text, at)) })
}
