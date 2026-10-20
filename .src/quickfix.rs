//! Quick fixes for what the runtime reports (ADR-0064, amendment
//! 2026-10-13): the runtime's validation is the only judge, and where a
//! problem it reports names a key or a name the file writes, and the fix is
//! determinable, the editor offers it:
//!
//! - a key no reader of its table takes: the nearest key it does take;
//! - a key its table cannot be without: that key, inserted with its kind's
//!   empty value for the developer to fill;
//! - a name nothing of its kind is declared under: each name that is.
//!
//! Which keys a table takes, which it requires, and what a reference may
//! name are the runtime's answer ([`Model`]); each fix is one of
//! `xmip_cluster_edit_v1`'s, made by the runtime as the action is offered.

use serde_json::{Value, json};

use crate::designer::text_edit;
use crate::edit;
use crate::model::{Model, list, strings};
use crate::runtime::Runtime;

/// The fixes for `diagnostics`, each with the edit that makes it.
#[must_use]
pub fn fixes(runtime: &Runtime, model: &Model, uri: &str, diagnostics: &[Value]) -> Vec<Value> {
    let mut found = Vec::new();
    for diagnostic in diagnostics {
        let message = diagnostic["message"].as_str().unwrap_or_default();
        let said = quoted(message);
        for (title, change, preferred) in proposed(model, &said) {
            if let Ok(edited) = edit::made(runtime, &model.text, &change) {
                found.push(json!({
                    "title": title,
                    "kind": "quickfix",
                    "diagnostics": [diagnostic],
                    "isPreferred": preferred,
                    "edit": { "changes": { uri: [text_edit(&model.text, &edited)] } },
                }));
            }
        }
    }
    found
}

/// What a problem quotes: the words between backticks or quotes.
fn quoted(message: &str) -> Vec<String> {
    let mut words = Vec::new();
    for mark in ['`', '\'', '"'] {
        let parts: Vec<&str> = message.split(mark).collect();
        words.extend(
            parts
                .iter()
                .skip(1)
                .step_by(2)
                .map(|word| (*word).to_string()),
        );
    }
    words
}

/// Each fix the quoted words make determinable: its title, its edit, and
/// whether it is the one to prefer.
fn proposed(model: &Model, said: &[String]) -> Vec<(String, Value, bool)> {
    let mut found = Vec::new();
    for table in model.written() {
        let section = strings(&table["section"]);
        let header = strings(&table["table"]);
        let known = known(model, &header);
        if known.is_empty() {
            continue;
        }
        let written: Vec<Vec<String>> = list(&table["keys"])
            .iter()
            .filter(|key| key["index"].is_null())
            .map(|key| strings(&key["key"]))
            .collect();
        for key in written.iter().filter(|key| key.len() == 1) {
            let name = &key[0];
            let unknown = said.contains(name) && !known.iter().any(|(k, _, _, _)| k == name);
            if unknown
                && let Some(nearest) = nearest(name, known.iter().map(|(k, _, _, _)| k.as_str()))
            {
                let title = format!("Change {name} to {nearest} in [{}]", section.join("."));
                found.push((title, edit::rename_key(&section, key, &nearest), true));
            }
        }
        for (name, kind, required, choices) in &known {
            let missing = !written.iter().any(|key| key.first() == Some(name));
            if *required && missing && said.contains(name) {
                let title = format!("Add {name} to [{}]", section.join("."));
                let value = edit::placeholder(kind, choices);
                found.push((
                    title,
                    edit::set(&section, std::slice::from_ref(name), &value),
                    false,
                ));
            }
        }
    }
    for reference in model.references() {
        let name = reference["name"].as_str().unwrap_or_default();
        if name.is_empty() || !reference["target"].is_null() || !said.iter().any(|w| w == name) {
            continue;
        }
        let mut choices = strings(&reference["choices"]);
        choices.sort_by_key(|choice| distance(name, choice));
        for (place, choice) in choices.iter().take(5).enumerate() {
            let what = reference["names"].as_str().unwrap_or_default();
            let title = format!("Change {name} to the {what} {choice}");
            let change = edit::refer(
                &reference["holder"],
                &reference["key"],
                &reference["index"],
                choice,
            );
            found.push((title, change, place == 0));
        }
    }
    found
}

/// The keys a table written as `header` takes: name, kind, whether it is
/// required, and the words it reads — its readers' keys, or the runtime's
/// `[tuning]` declaration.
fn known(model: &Model, header: &[String]) -> Vec<(String, String, bool, Vec<String>)> {
    if header.last().map(String::as_str) == Some("tuning") {
        return list(&model.tuning()["settings"])
            .iter()
            .map(|setting| {
                let name = setting["name"].as_str().unwrap_or_default().to_string();
                (name, String::new(), false, Vec::new())
            })
            .collect();
    }
    let free = header.last().is_some_and(|last| last.ends_with("settings"));
    if free {
        return Vec::new();
    }
    model
        .keys_of(header)
        .into_iter()
        .map(|key| {
            let name = key["key"].as_str().unwrap_or_default().to_string();
            let kind = key["kind"].as_str().unwrap_or_default().to_string();
            (
                name,
                kind,
                key["required"] == true,
                strings(&key["choices"]),
            )
        })
        .collect()
}

/// The known key nearest `written`, where one is near enough to be meant.
fn nearest<'k>(written: &str, known: impl Iterator<Item = &'k str>) -> Option<String> {
    known
        .map(|key| (distance(written, key), key))
        .filter(|(far, key)| *far <= 2.max(key.len() / 3))
        .min()
        .map(|(_, key)| key.to_string())
}

/// How many letters must change to turn one word into the other.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (previous + usize::from(x != *y))
                .min(row[j] + 1)
                .min(here + 1);
            previous = here;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_key_is_offered_only_where_it_is_near() {
        let known = ["address", "transport", "start"];
        // A key spelled wrong on purpose: address with one d.
        let wrong = "address".replacen("dd", "d", 1);
        assert_eq!(
            nearest(&wrong, known.into_iter()).as_deref(),
            Some("address")
        );
        assert_eq!(nearest("color", known.into_iter()), None);
        assert_eq!(
            quoted(&format!("unknown field `{wrong}`, expected one of `a`")),
            [wrong.as_str(), "a"]
        );
    }
}
