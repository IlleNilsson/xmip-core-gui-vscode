//! A Location's settings in the text editor: completion and hover from what
//! each technology declares (ADR-0064, amendment 2026-09-26), and the
//! declarations themselves for the extension's Location form.
//!
//! Every word offered here — a transport's or a contract's module name, a
//! setting's name, its kind, whether it is required, its default, what it
//! means — is the technology's own, read from the runtime's library through
//! `xmip_technology_catalogue_v1` (`xmip_operate.h` section 12). Nothing is
//! known here about any technology. What is here is the editor's part: where
//! the cursor is — which Location's table, on which side, naming which
//! technology — read from the lines around it, and the protocol's shapes.
//!
//! One custom request, for the form the next slice draws:
//! `xmip/technologies` `{ technology? }` — the catalogue, or one
//! technology's declaration, as the runtime answers it.

use abi::ffi::status;
use serde_json::{Value, json};

use crate::designer::Unanswered;
use crate::runtime::Runtime;

/// The custom request answering the catalogue.
pub const TECHNOLOGIES: &str = "xmip/technologies";
const COMPLETION: &str = "textDocument/completion";
const HOVER: &str = "textDocument/hover";

/// The methods this file answers.
pub const METHODS: [&str; 3] = [COMPLETION, HOVER, TECHNOLOGIES];

/// What `method` answers when the runtime cannot: nothing offered, for a
/// completion or a hover; `None` for the request, which says why.
#[must_use]
pub fn quiet(method: &str) -> Option<Value> {
    match method {
        COMPLETION => Some(json!([])),
        HOVER => Some(Value::Null),
        _ => None,
    }
}

/// Answer `method` over `params`, `document` the text the server holds for
/// the params' uri.
///
/// # Errors
/// The runtime could not be asked, or refused a technology it does not
/// carry.
pub fn answer(
    runtime: &Runtime,
    method: &str,
    params: &Value,
    document: Option<&str>,
) -> Result<Value, Unanswered> {
    if method == TECHNOLOGIES {
        return catalogue(runtime, params["technology"].as_str().unwrap_or_default());
    }

    let position = &params["position"];
    let at = document
        .zip(position["line"].as_u64())
        .zip(position["character"].as_u64());
    let Some(((text, line), character)) = at else {
        return Ok(quiet(method).unwrap_or_default());
    };
    let line = usize::try_from(line).unwrap_or(usize::MAX);
    let character = usize::try_from(character).unwrap_or(usize::MAX);
    let Some(place) = place(text, line, character) else {
        return Ok(quiet(method).unwrap_or_default());
    };
    let catalogue = catalogue(runtime, "")?;

    Ok(if method == COMPLETION {
        json!(completion(&catalogue, &place))
    } else {
        let written = text.lines().nth(line).unwrap_or_default();
        hover(&catalogue, &place, written).unwrap_or(Value::Null)
    })
}

/// The runtime's catalogue answer, parsed.
fn catalogue(runtime: &Runtime, technology: &str) -> Result<Value, Unanswered> {
    let answer = runtime
        .catalogue(technology)
        .map_err(Unanswered::Unavailable)?;
    if answer.status != status::OK {
        return Err(Unanswered::Refused(answer.text));
    }
    serde_json::from_str(&answer.text)
        .map_err(|error| Unanswered::Unavailable(format!("the catalogue is not JSON: {error}")))
}

/// Which table the cursor is in, as far as settings go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// On the value of a Location's `transport` or `contract` key: the
    /// technologies of that capability are offered.
    Naming { capability: &'static str },
    /// In a Location's `settings` or `contract_settings` table.
    Settings {
        /// `receive` or `send`, as a declaration's `applies` says it.
        side: &'static str,
        /// The module the Location names for this table, where it names one.
        technology: Option<String>,
        /// The keys already written in the table.
        written: Vec<String>,
    },
}

/// The four arrays a Location is written in, and the side each is on.
const LOCATIONS: [(&str, &str); 4] = [
    ("receive_locations", "receive"),
    ("send_locations", "send"),
    ("applications.receive_locations", "receive"),
    ("applications.send_ports", "send"),
];

/// Where `line`, at `character`, is in `text`.
#[must_use]
pub fn place(text: &str, line: usize, character: usize) -> Option<Place> {
    let lines: Vec<&str> = text.lines().collect();
    let current = lines.get(line).copied().unwrap_or_default();
    let before: String = current.chars().take(character).collect();

    for (key, capability) in [("transport", "transport"), ("contract", "contract")] {
        if let Some((name, value)) = before.split_once('=')
            && name.trim() == key
            && value.trim_start().starts_with('"')
        {
            return Some(Place::Naming { capability });
        }
    }

    let (header_line, header) = lines[..=line.min(lines.len().saturating_sub(1))]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, text)| text.trim_start().starts_with('['))?;
    let table = header
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();

    let (array, key) = if let Some(array) = table.strip_suffix(".contract_settings") {
        (array, "contract")
    } else {
        (table.strip_suffix(".settings")?, "transport")
    };
    let side = LOCATIONS
        .iter()
        .find(|(name, _)| *name == array)
        .map(|(_, side)| *side)?;

    let opening = format!("[[{array}]]");
    let technology = lines[..header_line]
        .iter()
        .rev()
        .take_while(|text| text.trim() != opening)
        .find_map(|text| value_of(text, key));
    let written = lines[header_line + 1..]
        .iter()
        .take_while(|text| !text.trim_start().starts_with('['))
        .filter_map(|text| {
            text.split_once('=')
                .map(|(name, _)| name.trim().to_string())
        })
        .collect();

    Some(Place::Settings {
        side,
        technology,
        written,
    })
}

/// The string `key = "…"` holds on `line`, where it is that key's line.
fn value_of(line: &str, key: &str) -> Option<String> {
    let (name, value) = line.split_once('=')?;
    if name.trim() != key {
        return None;
    }
    Some(value.trim().trim_matches('"').to_string())
}

/// The technologies of a catalogue answer.
fn technologies(catalogue: &Value) -> &[Value] {
    catalogue["technologies"]
        .as_array()
        .map_or(&[], Vec::as_slice)
}

/// The declaration of `technology` in the catalogue.
fn declaration<'a>(catalogue: &'a Value, technology: &str) -> Option<&'a Value> {
    technologies(catalogue)
        .iter()
        .find(|entry| entry["technology"] == technology)
}

/// Whether a setting applies on `side`.
fn applies(setting: &Value, side: &str) -> bool {
    matches!(setting["applies"].as_str(), Some("both")) || setting["applies"] == side
}

/// A setting's kind, presence and side in one line, as a form's hint and a
/// completion's detail say it.
fn summary(setting: &Value) -> String {
    let kind = setting["kind"].as_str().unwrap_or("text");
    let presence = match setting["presence"].as_str() {
        Some("default") => format!("default {}", setting["default"]),
        Some(presence) => presence.to_string(),
        None => String::new(),
    };
    let side = setting["applies"].as_str().unwrap_or("both");
    format!("{kind} · {presence} · {side}")
}

/// What is written after `name = ` for a setting of this kind: a snippet
/// whose placeholder is the default, where there is one.
fn snippet(setting: &Value) -> String {
    let default = &setting["default"];
    let placeholder = match default {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    match setting["kind"].as_str() {
        Some("integer") => format!(
            "${{1:{}}}",
            if placeholder.is_empty() {
                "0"
            } else {
                &placeholder
            }
        ),
        Some("boolean") => "${1|true,false|}".to_string(),
        Some("choice") => {
            let choices: Vec<&str> = setting["choices"]
                .as_array()
                .map(|all| all.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            format!("\"${{1|{}|}}\"", choices.join(","))
        }
        _ => format!("\"${{1:{placeholder}}}\""),
    }
}

/// The completion items at `place`, from `catalogue`.
#[must_use]
pub fn completion(catalogue: &Value, place: &Place) -> Vec<Value> {
    match place {
        Place::Naming { capability } => technologies(catalogue)
            .iter()
            .filter(|entry| entry["capability"] == *capability)
            .map(|entry| {
                let count = entry["settings"].as_array().map_or(0, Vec::len);
                json!({
                    "label": entry["technology"],
                    "kind": 9,
                    "detail": format!("{capability} · {count} settings"),
                })
            })
            .collect(),
        Place::Settings {
            side,
            technology: Some(technology),
            written,
        } => declaration(catalogue, technology)
            .and_then(|entry| entry["settings"].as_array())
            .map(|settings| {
                settings
                    .iter()
                    .filter(|setting| applies(setting, side))
                    .filter(|setting| !written.iter().any(|name| setting["name"] == name.as_str()))
                    .map(|setting| {
                        json!({
                            "label": setting["name"],
                            "kind": 10,
                            "detail": summary(setting),
                            "documentation": setting["meaning"],
                            "insertText": format!("{} = {}", setting["name"].as_str()
                                .unwrap_or_default(), snippet(setting)),
                            "insertTextFormat": 2,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Place::Settings { .. } => Vec::new(),
    }
}

/// The hover for the key written on `line` at `place`, from `catalogue`:
/// what the technology declares of that setting.
#[must_use]
pub fn hover(catalogue: &Value, place: &Place, line: &str) -> Option<Value> {
    let Place::Settings {
        technology: Some(technology),
        ..
    } = place
    else {
        return None;
    };
    let (name, _) = line.split_once('=')?;
    let setting = declaration(catalogue, technology)?["settings"]
        .as_array()?
        .iter()
        .find(|setting| setting["name"] == name.trim())?;
    let meaning = setting["meaning"].as_str().unwrap_or_default();
    Some(json!({
        "contents": {
            "kind": "markdown",
            "value": format!(
                "**{}** — {}\n\n{meaning}\n\n*Declared by {technology}.*",
                name.trim(),
                summary(setting)
            ),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue() -> Value {
        json!({"technologies": [
            {"capability": "transport", "technology": "xmip-core-transport-kafka",
             "settings": [
                {"name": "topic", "kind": "text", "presence": "required",
                 "meaning": "The topic a Location reads or writes.", "applies": "both"},
                {"name": "group", "kind": "text", "presence": "optional",
                 "meaning": "The consumer group.", "applies": "receive"},
                {"name": "acks", "kind": "choice", "choices": ["none", "all"],
                 "presence": "default", "default": "all",
                 "meaning": "Who acknowledges a send.", "applies": "send"}]},
            {"capability": "contract", "technology": "xmip-core-contract-json-schema",
             "settings": []}
        ]})
    }

    const NODE: &str = "[[send_locations]]\nname = \"out\"\n\
        transport = \"xmip-core-transport-kafka\"\naddress = \"b:9092\"\n\
        [send_locations.settings]\ntopic = \"orders\"\n\n";

    #[test]
    fn a_settings_table_knows_its_side_technology_and_written_keys() {
        let place = place(NODE, 6, 0).expect("in the table");
        assert_eq!(
            place,
            Place::Settings {
                side: "send",
                technology: Some("xmip-core-transport-kafka".to_string()),
                written: vec!["topic".to_string()],
            }
        );
        assert_eq!(place_outside(), None);
    }

    fn place_outside() -> Option<Place> {
        place(NODE, 1, 3)
    }

    #[test]
    fn completion_offers_what_applies_and_is_not_yet_written() {
        let items = completion(&catalogue(), &place(NODE, 6, 0).expect("in the table"));
        let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
        assert_eq!(
            labels,
            ["acks"],
            "group is a receive setting, topic is written"
        );
        assert_eq!(items[0]["insertText"], "acks = \"${1|none,all|}\"");
        assert!(
            items[0]["detail"]
                .as_str()
                .expect("detail")
                .contains("default")
        );
    }

    #[test]
    fn naming_a_transport_offers_the_transports_carried() {
        let place = place("transport = \"", 0, 13).expect("naming");
        assert_eq!(
            place,
            Place::Naming {
                capability: "transport"
            }
        );
        let items = completion(&catalogue(), &place);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["label"], "xmip-core-transport-kafka");
    }

    #[test]
    fn a_completion_or_hover_without_a_runtime_offers_nothing_and_the_request_says_why() {
        assert_eq!(quiet(COMPLETION), Some(json!([])));
        assert_eq!(quiet(HOVER), Some(Value::Null));
        assert_eq!(quiet(TECHNOLOGIES), None);
    }

    #[test]
    fn hover_says_what_the_technology_declares() {
        let place = place(NODE, 5, 2).expect("in the table");
        let shown = hover(&catalogue(), &place, "topic = \"orders\"").expect("a hover");
        let value = shown["contents"]["value"].as_str().expect("markdown");
        assert!(value.contains("**topic**"), "{value}");
        assert!(value.contains("The topic a Location reads or writes."));
        assert!(value.contains("xmip-core-transport-kafka"));
    }
}
