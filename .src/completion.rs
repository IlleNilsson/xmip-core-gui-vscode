//! Suggestions while the cluster's `xmip.toml` is typed (ADR-0064,
//! amendment 2026-10-13): a table's header, a table's keys, and a key's
//! value — a choice's words, a role word, the names a reference may take,
//! the properties and operators a filter is written with.
//!
//! Every word offered is the runtime's ([`Model`]): the keys each table may
//! hold and their kinds are `xmip-core-configure`'s readers' own, the names
//! a reference may take are what the file declares where it looks, a role
//! word is `node::NodeRole`'s (`xmip_role_words_v1`), and `[tuning]`'s keys
//! the runtime's declaration. What is here is where the cursor is, read
//! from the line it is on, and the protocol's shapes. A Location's
//! technology and settings are `settings.rs`'s.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::model::{Model, list, strings};
use crate::text::{line_before, position};

/// The suggestions at `offset`; `roles` asks the runtime for the role words.
#[must_use]
pub fn completion(model: &Model, offset: usize, roles: &dyn Fn() -> Vec<String>) -> Vec<Value> {
    let before = line_before(&model.text, offset);
    if let Some(open) = before
        .find('[')
        .filter(|_| before.trim_start().starts_with('['))
    {
        let start = offset - (before.len() - open);
        return headers(model, start, offset);
    }
    let Some(table) = model.table_at(offset) else {
        return Vec::new();
    };
    let header = strings(&table["table"]);
    let section = strings(&table["section"]);
    let Some((outer, value)) = before.split_once('=') else {
        return keys(model, table, &header, &section, roles);
    };
    let outer = outer.trim().to_string();
    let inline = value.rfind(['{', ',']).map(|at| &value[at + 1..]);
    match inline {
        Some(inner) if !inner.contains('=') && value.contains('{') => {
            let cases = cases(model, &header, &section, &outer);
            if !cases.is_empty() {
                return cases;
            }
            let within = [header, vec![outer]].concat();
            let written = BTreeSet::new();
            key_items(model, &within, &section, &written, roles)
        }
        Some(inner) if value.contains('{') => {
            let key = inner
                .split('=')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            values(model, &header, &section, &[outer, key], inner, roles)
        }
        _ => values(model, &header, &section, &[outer], value, roles),
    }
}

/// Every header the file may open, a node's tables for each node it
/// declares.
fn headers(model: &Model, start: usize, offset: usize) -> Vec<Value> {
    let nodes: Vec<String> = model
        .entries()
        .into_iter()
        .filter(|entry| entry.kind == "node")
        .map(|entry| entry.name.to_string())
        .collect();
    let mut found = BTreeSet::new();
    for key in list(&model.answer["keys"]) {
        let kind = key["kind"].as_str().unwrap_or_default();
        if !matches!(kind, "table" | "list" | "free") || key["key"] == "nodes" {
            continue;
        }
        let path: Vec<String> = [strings(&key["table"]), vec![string(&key["key"])]].concat();
        let each: Vec<Vec<String>> = if path.iter().any(|step| step == "*") {
            nodes
                .iter()
                .map(|node| {
                    path.iter()
                        .map(|s| if s == "*" { node.clone() } else { s.clone() })
                        .collect()
                })
                .collect()
        } else {
            vec![path]
        };
        for path in each {
            let joined = path.join(".");
            found.insert(if kind == "list" {
                format!("[[{joined}]]")
            } else {
                format!("[{joined}]")
            });
        }
    }
    for node in &nodes {
        found.insert(format!("[nodes.{node}]"));
    }
    let range =
        json!({ "start": position(&model.text, start), "end": position(&model.text, offset) });
    found
        .into_iter()
        .map(|header| {
            let edit = json!({ "range": range, "newText": header });
            json!({ "label": header, "kind": 22, "textEdit": edit })
        })
        .collect()
}

/// The keys a table may hold that it does not write yet.
fn keys(
    model: &Model,
    table: &Value,
    header: &[String],
    section: &[String],
    roles: &dyn Fn() -> Vec<String>,
) -> Vec<Value> {
    let written: BTreeSet<String> = list(&table["keys"])
        .iter()
        .filter_map(|key| {
            list(&key["key"])
                .first()
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    if header.last().map(String::as_str) == Some("tuning") {
        return list(&model.tuning()["settings"])
            .iter()
            .filter(|setting| !written.contains(setting["name"].as_str().unwrap_or_default()))
            .map(|setting| {
                json!({
                    "label": setting["name"], "kind": 10,
                    "detail": format!("{} · {}", setting["kind"], setting["presence"]),
                    "documentation": setting["meaning"],
                    "insertText": format!("{} = ", string(&setting["name"])),
                })
            })
            .collect();
    }
    key_items(model, header, section, &written, roles)
}

fn key_items(
    model: &Model,
    header: &[String],
    section: &[String],
    written: &BTreeSet<String>,
    roles: &dyn Fn() -> Vec<String>,
) -> Vec<Value> {
    model
        .keys_of(header)
        .into_iter()
        .filter(|key| !written.contains(key["key"].as_str().unwrap_or_default()))
        .filter(|key| !matches!(key["kind"].as_str(), Some("list" | "free")))
        .map(|key| {
            let name = string(&key["key"]);
            let kind = key["kind"].as_str().unwrap_or_default();
            let choices = reference_choices(model, section, std::slice::from_ref(&name))
                .unwrap_or_else(|| match kind {
                    "roles" => roles(),
                    _ => strings(&key["choices"]),
                });
            let required = if key["required"] == true {
                " · required"
            } else {
                ""
            };
            let within = format!("In [{}]", header.join("."));
            let documentation = match key["meaning"].as_str() {
                Some(meaning) => format!("{meaning}\n\n{within}"),
                None => within,
            };
            json!({
                "label": name, "kind": 10,
                "detail": format!("{kind}{required}"),
                "documentation": documentation,
                "insertText": format!("{name} = {}", snippet(kind, &choices)),
                "insertTextFormat": 2,
            })
        })
        .collect()
}

/// The cases a one-key table such as a Subscription's `destination` is
/// keyed by, each with the names it may take.
fn cases(model: &Model, header: &[String], section: &[String], outer: &str) -> Vec<Value> {
    let Some(variant) = model
        .keys_of(header)
        .into_iter()
        .find(|key| key["key"] == outer && key["kind"] == "variant")
    else {
        return Vec::new();
    };
    strings(&variant["choices"])
        .into_iter()
        .map(|case| {
            let key = [outer.to_string(), case.clone()];
            let names = reference_choices(model, section, &key).unwrap_or_default();
            json!({
                "label": case, "kind": 20,
                "insertText": format!("{case} = {}", snippet("text", &names)),
                "insertTextFormat": 2,
            })
        })
        .collect()
}

/// What follows `key = ` for a value of `kind`.
fn snippet(kind: &str, choices: &[String]) -> String {
    let one_of = || {
        choices
            .iter()
            .map(|c| c.replace(',', "\\,").replace('|', "\\|"))
            .collect::<Vec<_>>()
            .join(",")
    };
    match kind {
        _ if !choices.is_empty() && kind == "variant" => {
            format!("{{ ${{1|{}|}} = \"$2\" }}", one_of())
        }
        _ if !choices.is_empty() => format!("\"${{1|{}|}}\"", one_of()),
        "boolean" => "${1|true,false|}".to_string(),
        "integer" | "float" => "${1:0}".to_string(),
        "array" => "[$1]".to_string(),
        "table" => "{ $1 }".to_string(),
        _ => "\"$1\"".to_string(),
    }
}

/// The names a reference at `key` of the table at `section` may take; in a
/// sub-table, such as a Location's `[accept]`, the reference its entry holds
/// under the sub-table's key.
fn reference_choices(model: &Model, section: &[String], key: &[String]) -> Option<Vec<String>> {
    (0..section.len()).rev().find_map(|keep| {
        let holder = &section[..=keep];
        let key = [&section[keep + 1..], key].concat();
        let found: BTreeSet<String> = model
            .references()
            .iter()
            .filter(|reference| {
                strings(&reference["holder"]) == holder && strings(&reference["key"]) == key
            })
            .flat_map(|reference| strings(&reference["choices"]))
            .collect();
        (!found.is_empty()).then(|| found.into_iter().collect())
    })
}

/// A key's value: the names a reference takes, a choice's words, a role
/// word, true or false, or a filter's properties and operators.
fn values(
    model: &Model,
    header: &[String],
    section: &[String],
    key: &[String],
    typed: &str,
    roles: &dyn Fn() -> Vec<String>,
) -> Vec<Value> {
    let quoted =
        typed.trim_start().starts_with(['"', '\'']) || typed.contains('[') && typed.contains('"');
    let word = |text: &str, kind: u8| {
        let insert = if quoted {
            text.to_string()
        } else {
            format!("\"{text}\"")
        };
        json!({ "label": text, "kind": kind, "insertText": insert })
    };
    if key == ["filter"] {
        return filter_words(model)
            .into_iter()
            .map(|text| json!({ "label": text, "kind": 14 }))
            .collect();
    }
    if let Some(names) = reference_choices(model, section, key) {
        return names.iter().map(|name| word(name, 18)).collect();
    }
    let (name, within) = key
        .split_last()
        .map_or((String::new(), Vec::new()), |(n, w)| {
            (n.clone(), w.to_vec())
        });
    let declared = model
        .keys_of(&[header.to_vec(), within].concat())
        .into_iter()
        .find(|known| known["key"] == name.as_str());
    let Some(declared) = declared else {
        return Vec::new();
    };
    match declared["kind"].as_str() {
        Some("boolean") => ["true", "false"]
            .iter()
            .map(|b| json!({ "label": b, "kind": 12 }))
            .collect(),
        Some("roles") => roles().iter().map(|role| word(role, 20)).collect(),
        _ => strings(&declared["choices"])
            .iter()
            .map(|choice| word(choice, 20))
            .collect(),
    }
}

/// The properties the file's filters compare and the operators a filter
/// row offers, from every Xmip Application's routes.
fn filter_words(model: &Model) -> Vec<String> {
    let mut words = BTreeSet::new();
    for view in list(&model.answer["views"]) {
        for entry in list(&view["entries"]) {
            let routes = &entry["routes"];
            words.extend(strings(&routes["operators"]));
            for node in list(&routes["nodes"]) {
                properties(&node["filter"], &mut words);
            }
        }
    }
    words.into_iter().collect()
}

fn properties(part: &Value, words: &mut BTreeSet<String>) {
    match part {
        Value::Object(fields) => {
            if let Some(Value::String(property)) = fields.get("property") {
                words.insert(property.clone());
            }
            fields.values().for_each(|inner| properties(inner, words));
        }
        Value::Array(items) => items.iter().for_each(|inner| properties(inner, words)),
        _ => {}
    }
}

fn string(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}
