//! The routes designer's requests (ADR-0064): what the VS Code extension's
//! webview asks for, answered from the runtime's library.
//!
//! Four custom requests, each one of `xmip_operate.h` section 10's exports:
//!
//! - `xmip/routes` `{ text }` — an Xmip Application's routes as a graph,
//!   every node given its place on the canvas;
//! - `xmip/filterStructure` `{ filter }` — a filter's text as rows and groups;
//! - `xmip/filterText` `{ structure }` — rows and groups as the filter's text;
//! - `xmip/edit` `{ text, edit }` — an edit to the Application, answered as
//!   the one text edit that makes it, so the webview never writes TOML.
//!
//! What the design means is `xmip-core-configure`'s, behind the runtime's
//! library. What is here is the language server's part: the layout — where
//! each node stands, from its place along the route — and the text edit,
//! in the protocol's positions.

use std::collections::BTreeMap;

use abi::ffi::status;
use abi::operate::design::{
    APPLICATION_EDIT_ENTRYPOINT, APPLICATION_ROUTES_ENTRYPOINT, FILTER_STRUCTURE_ENTRYPOINT,
    FILTER_TEXT_ENTRYPOINT,
};
use serde_json::{Value, json};

use crate::runtime::Runtime;

/// The methods this file answers.
pub const METHODS: [&str; 4] = [ROUTES, FILTER_STRUCTURE, FILTER_TEXT, EDIT];

const ROUTES: &str = "xmip/routes";
const FILTER_STRUCTURE: &str = "xmip/filterStructure";
const FILTER_TEXT: &str = "xmip/filterText";
const EDIT: &str = "xmip/edit";

/// Where a column starts, how wide a node is, where a row starts and how
/// tall a node is, in the webview's pixels.
const COLUMN: i64 = 260;
const WIDTH: i64 = 210;
const ROW: i64 = 92;
const HEIGHT: i64 = 64;
const MARGIN: i64 = 24;

/// Why a request was not answered.
#[derive(Debug, PartialEq, Eq)]
pub enum Unanswered {
    /// The parameters are not what the method takes.
    Params(String),
    /// The runtime could not be asked.
    Unavailable(String),
    /// The runtime refused, in its own sentence.
    Refused(String),
}

/// Answer `method` over `params`. `document` is the Application's text, the
/// one the request carries or the one the server holds for its uri.
///
/// # Errors
/// [`Unanswered`], saying which of the three it was.
pub fn answer(
    runtime: &Runtime,
    method: &str,
    params: &Value,
    document: Option<&str>,
) -> Result<Value, Unanswered> {
    let needed = |key: &str| Unanswered::Params(format!("{method} needs {key}"));

    match method {
        ROUTES => {
            let text = document.ok_or_else(|| needed("a text or an open uri"))?;
            let routes = ask(runtime, APPLICATION_ROUTES_ENTRYPOINT, text, "")?;
            Ok(laid_out(parsed(&routes)?))
        }
        FILTER_STRUCTURE => {
            let filter = params["filter"]
                .as_str()
                .ok_or_else(|| needed("a filter"))?;
            parsed(&ask(runtime, FILTER_STRUCTURE_ENTRYPOINT, filter, "")?)
        }
        FILTER_TEXT => {
            let structure = params
                .get("structure")
                .ok_or_else(|| needed("a structure"))?;
            let text = ask(runtime, FILTER_TEXT_ENTRYPOINT, &structure.to_string(), "")?;
            Ok(json!({ "text": text }))
        }
        EDIT => {
            let text = document.ok_or_else(|| needed("a text or an open uri"))?;
            let edit = params.get("edit").ok_or_else(|| needed("an edit"))?;
            let edited = ask(
                runtime,
                APPLICATION_EDIT_ENTRYPOINT,
                text,
                &edit.to_string(),
            )?;
            Ok(text_edit(text, &edited))
        }
        _ => Err(Unanswered::Params(format!(
            "{method} is not a designer request"
        ))),
    }
}

fn ask(
    runtime: &Runtime,
    entrypoint: &str,
    input: &str,
    argument: &str,
) -> Result<String, Unanswered> {
    let answer = runtime
        .design(entrypoint, input, argument)
        .map_err(Unanswered::Unavailable)?;

    if answer.status == status::OK {
        Ok(answer.text)
    } else {
        Err(Unanswered::Refused(answer.text))
    }
}

fn parsed(text: &str) -> Result<Value, Unanswered> {
    serde_json::from_str(text)
        .map_err(|error| Unanswered::Unavailable(format!("the runtime answered no JSON: {error}")))
}

/// The graph with every node placed: its column from its place along the
/// route, its row from its order in that column, and the canvas it needs.
#[must_use]
pub fn laid_out(mut routes: Value) -> Value {
    let mut rows: BTreeMap<i64, i64> = BTreeMap::new();

    if let Some(nodes) = routes["nodes"].as_array_mut() {
        for node in nodes {
            let column = node["column"].as_i64().unwrap_or_default();
            let row = rows.entry(column).or_default();
            node["x"] = json!(MARGIN + column * COLUMN);
            node["y"] = json!(MARGIN + *row * ROW);
            node["width"] = json!(WIDTH);
            node["height"] = json!(HEIGHT);
            *row += 1;
        }
    }

    let columns = rows.keys().max().map_or(0, |last| last + 1);
    let deepest = rows.values().max().copied().unwrap_or_default();
    routes["width"] = json!(2 * MARGIN + (columns - 1).max(0) * COLUMN + WIDTH);
    routes["height"] = json!(2 * MARGIN + (deepest - 1).max(0) * ROW + HEIGHT);
    routes
}

/// The one text edit that turns `old` into `new`: what lies between the
/// text they share at the start and at the end, in the protocol's
/// positions — lines, and UTF-16 code units within a line.
#[must_use]
pub fn text_edit(old: &str, new: &str) -> Value {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }

    let most = old.len().min(new.len()) - prefix;
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(most)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }

    json!({
        "range": {
            "start": position(old, prefix),
            "end": position(old, old.len() - suffix),
        },
        "newText": &new[prefix..new.len() - suffix],
    })
}

fn position(text: &str, offset: usize) -> Value {
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    let character = before[start..].encode_utf16().count();

    json!({ "line": line, "character": character })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
    use std::path::Path;

    /// The estate's runtime as `cargo build` leaves it, beside this
    /// repository in the estate. Absent when nobody built it, which is said.
    fn built() -> Option<Runtime> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../platform/runtime/target/debug")
            .join(format!("{DLL_PREFIX}xmip_core_runtime{DLL_SUFFIX}"));

        if path.is_file() {
            Some(Runtime::load(&path).expect("the built runtime loads"))
        } else {
            println!("skipped: no runtime library at {}", path.display());
            None
        }
    }

    const ORDERS: &str = "[application]\nname = \"Orders\"\n\n[[receive_locations]]\n\
                          name = \"OrdersIn\"\n\n[[xmip_processes]]\nname = \"Approval\"\n\n\
                          [[send_ports]]\nname = \"Billing\"\n\n[[subscriptions]]\n\
                          id = \"billing\"\ndestination = { send-port = \"Billing\" }\n\
                          filter = \"MessageType = 'Order'\"\n";

    fn edited(runtime: &Runtime, edit: &Value) -> String {
        let answered =
            answer(runtime, EDIT, &json!({ "edit": edit }), Some(ORDERS)).expect("edits");
        applied(ORDERS, &answered)
    }

    /// The text edit applied the way an editor applies it.
    fn applied(text: &str, edit: &Value) -> String {
        let offset = |at: &Value| {
            let line = usize::try_from(at["line"].as_u64().expect("a line")).expect("fits");
            let character = at["character"].as_u64().expect("a character");
            let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
            let within = text[start..].chars().scan(0u64, |units, c| {
                let here = *units;
                *units += u64::try_from(c.len_utf16()).expect("fits");
                Some((here, c.len_utf8()))
            });
            start
                + within
                    .take_while(|(units, _)| *units < character)
                    .map(|(_, n)| n)
                    .sum::<usize>()
        };
        let (from, to) = (
            offset(&edit["range"]["start"]),
            offset(&edit["range"]["end"]),
        );
        format!(
            "{}{}{}",
            &text[..from],
            edit["newText"].as_str().expect("text"),
            &text[to..]
        )
    }

    #[test]
    fn a_text_edit_is_what_lies_between_the_shared_start_and_end() {
        let edit = text_edit("a = 1\nb = 2\n", "a = 1\nb = 3\n");

        assert_eq!(edit["range"]["start"], json!({ "line": 1, "character": 4 }));
        assert_eq!(edit["range"]["end"], json!({ "line": 1, "character": 5 }));
        assert_eq!(edit["newText"], "3");

        let wide = "name = \"\u{1F600}x\"\n";
        let edit = text_edit(wide, "name = \"\u{1F600}y\"\n");
        assert_eq!(edit["range"]["start"]["character"], 10, "UTF-16 units");
        assert_eq!(applied(wide, &edit), "name = \"\u{1F600}y\"\n");
        assert_eq!(applied("abc", &text_edit("abc", "abc")), "abc");
    }

    #[test]
    fn the_layout_puts_each_column_side_by_side_and_each_row_below() {
        let routes = laid_out(json!({ "nodes": [
            { "id": "a", "column": 0 }, { "id": "b", "column": 1 }, { "id": "c", "column": 1 },
        ] }));

        assert_eq!(routes["nodes"][0]["x"], MARGIN);
        assert_eq!(routes["nodes"][1]["x"], MARGIN + COLUMN);
        assert_eq!(routes["nodes"][2]["y"], MARGIN + ROW);
        assert_eq!(routes["width"], 2 * MARGIN + COLUMN + WIDTH);
        assert_eq!(routes["height"], 2 * MARGIN + ROW + HEIGHT);
    }

    #[test]
    fn the_graph_model_comes_from_the_runtime_laid_out() {
        let Some(runtime) = built() else { return };
        let routes = answer(&runtime, ROUTES, &json!({}), Some(ORDERS)).expect("routes");

        let ids = routes["nodes"].as_array().expect("nodes").iter();
        let ids = ids
            .map(|node| node["id"].as_str().expect("id"))
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "receive-location:OrdersIn",
                "subscription:billing",
                "xmip-process:Approval",
                "send-port:Billing"
            ]
        );
        assert_eq!(routes["nodes"][1]["summary"], "MessageType = 'Order'");
        assert_eq!(routes["nodes"][3]["x"], MARGIN + 3 * COLUMN);
        assert_eq!(routes["edges"].as_array().expect("edges").len(), 2);
        assert_eq!(routes["operators"][0], "=");
        assert_eq!(routes["kinds"][3], "expression");
    }

    #[test]
    fn a_filter_goes_to_its_structure_and_back_byte_for_byte() {
        let Some(runtime) = built() else { return };
        let filter = "exists Urgent or not Amount < 10";

        let structure = answer(
            &runtime,
            FILTER_STRUCTURE,
            &json!({ "filter": filter }),
            None,
        )
        .expect("structure");
        assert_eq!(structure["shape"], "group");
        assert_eq!(structure["join"], "or");

        let text = answer(
            &runtime,
            FILTER_TEXT,
            &json!({ "structure": structure }),
            None,
        )
        .expect("text");
        assert_eq!(text["text"], filter);

        let refused = answer(
            &runtime,
            FILTER_STRUCTURE,
            &json!({ "filter": "{ x = 1 }" }),
            None,
        );
        assert!(
            matches!(refused, Err(Unanswered::Refused(_))),
            "{refused:?}"
        );
    }

    #[test]
    fn each_edit_comes_back_as_the_text_edit_that_makes_it() {
        let Some(runtime) = built() else { return };

        let added = edited(
            &runtime,
            &json!({ "add-subscription": {
            "id": "approval", "target": "xmip-process:Approval" } }),
        );
        assert!(
            added.ends_with(
                "\n[[subscriptions]]\nid = \"approval\"\n\
            destination = { process = \"Approval\" }\nfilter = \"true\"\n"
            ),
            "{added}"
        );

        let filtered = edited(
            &runtime,
            &json!({ "set-filter": { "subscription": "billing",
            "filter": { "shape": "condition", "property": "Amount", "operator": ">",
                        "value": "1000", "kind": "integer" } } }),
        );
        assert!(
            filtered.contains("filter = \"Amount > 1000\"\n"),
            "{filtered}"
        );

        let connected = edited(
            &runtime,
            &json!({ "connect": {
            "subscription": "billing", "target": "xmip-process:Approval" } }),
        );
        assert_eq!(
            connected,
            ORDERS.replace("{ send-port = \"Billing\" }", "{ process = \"Approval\" }")
        );

        for (edit, name) in [
            ("add-receive-location", "Drop"),
            ("add-xmip-process", "Audit"),
            ("add-send-port", "Ledger"),
        ] {
            let added = edited(&runtime, &json!({ edit: { "name": name } }));
            assert!(added.contains(&format!("name = \"{name}\"\n")), "{added}");
        }

        let refused = answer(
            &runtime,
            EDIT,
            &json!({ "edit": { "connect": {
            "subscription": "billing", "target": "send-port:Audit" } } }),
            Some(ORDERS),
        );
        assert!(
            matches!(refused, Err(Unanswered::Refused(_))),
            "{refused:?}"
        );
    }
}
