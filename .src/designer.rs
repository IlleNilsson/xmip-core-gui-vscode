//! The designer's requests (ADR-0064, amendment 2026-10-03): what the VS
//! Code extension's webview asks of the cluster's one `xmip.toml`, answered
//! from the runtime's library.
//!
//! Four custom requests, each one of `xmip_operate.h` section 10's exports:
//!
//! - `xmip/views` `{ text }` — the cluster's file as one view per artifact
//!   kind, each Xmip Application's routes given their places on the canvas;
//! - `xmip/filterStructure` `{ filter }` — a filter's text as rows and groups;
//! - `xmip/filterText` `{ structure }` — rows and groups as the filter's text;
//! - `xmip/edit` `{ text, edit }` — an edit to the file, answered as the one
//!   text edit that makes it, so the webview never writes TOML.
//!
//! What the file and its design mean is `xmip-core-configure`'s, behind the
//! runtime's library. What is here is the language server's part: the
//! layout — where each node of a route stands, from its place along the
//! route — and the text edit, in the protocol's positions.

use std::collections::BTreeMap;

use abi::ffi::status;
use abi::operate::design::{
    CLUSTER_EDIT_ENTRYPOINT, CLUSTER_VIEWS_ENTRYPOINT, FILTER_STRUCTURE_ENTRYPOINT,
    FILTER_TEXT_ENTRYPOINT,
};
use serde_json::{Value, json};

use crate::runtime::Runtime;

/// The methods this file answers.
pub const METHODS: [&str; 4] = [VIEWS, FILTER_STRUCTURE, FILTER_TEXT, EDIT];

const VIEWS: &str = "xmip/views";
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

/// Answer `method` over `params`. `document` is the cluster's file, the
/// text the request carries or the one the server holds for its uri.
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
        VIEWS => {
            let text = document.ok_or_else(|| needed("a text or an open uri"))?;
            let views = ask(runtime, CLUSTER_VIEWS_ENTRYPOINT, text, "")?;
            Ok(every_route_laid_out(parsed(&views)?))
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
            let edited = ask(runtime, CLUSTER_EDIT_ENTRYPOINT, text, &edit.to_string())?;
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

/// The views with every Xmip Application's routes laid out.
#[must_use]
pub fn every_route_laid_out(mut views: Value) -> Value {
    for view in views["views"].as_array_mut().into_iter().flatten() {
        for entry in view["entries"].as_array_mut().into_iter().flatten() {
            if let Some(routes) = entry.get_mut("routes") {
                *routes = laid_out(routes.take());
            }
        }
    }
    views
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

    /// The runtime the estate built, when it built one.
    fn built() -> Option<Runtime> {
        crate::built::runtime_library()
            .map(|path| Runtime::load(&path).expect("the built runtime loads"))
    }

    /// A cluster's file holding one Xmip Application as a section. It
    /// declares no node, so it names none.
    const ORDERS: &str = "# The cluster's one file.\n[service]\nname = \"xmip\"\n\n\
                          [[receive_locations]]\nname = \"drop\"\nstart = true\n\
                          transport = \"xmip-core-transport-file\"\naddress = \"/in\"\n\n\
                          [[xmip_applications]]\nname = \"Orders\"\n\n\
                          [[xmip_applications.receive_ports]]\nname = \"Orders\"\n\n\
                          [[xmip_applications.receive_locations]]\nname = \"OrdersIn\"\n\
                          receive_port = \"Orders\"\ninteraction = \"data-transfer\"\n\
                          depth = \"light\"\n\n\
                          [[xmip_applications.work_processes]]\nname = \"Approval\"\n\n\
                          [[xmip_applications.send_ports]]\nname = \"Billing\"\n\n\
                          [[xmip_applications.subscriptions]]\nid = \"billing\"\n\
                          destination = { send-port = \"Billing\" }\n\
                          filter = \"MessageType = 'Order'\"\n";

    fn edited(runtime: &Runtime, edit: &Value) -> String {
        let answered =
            answer(runtime, EDIT, &json!({ "edit": edit }), Some(ORDERS)).expect("edits");
        applied(ORDERS, &answered)
    }

    /// One of the Application's own edits, made to its section.
    fn of_orders(edit: &Value) -> Value {
        json!({ "application": { "application": "Orders", "edit": edit } })
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
    fn the_views_come_from_the_runtime_with_every_route_laid_out() {
        let Some(runtime) = built() else { return };
        let views = answer(&runtime, VIEWS, &json!({}), Some(ORDERS)).expect("views");

        let kinds = views["views"].as_array().expect("views").iter();
        let kinds = kinds
            .map(|view| view["kind"].as_str().expect("kind"))
            .collect::<Vec<_>>();
        assert_eq!(kinds.len(), 13);
        assert_eq!(kinds[0], "cluster");
        let route = &views["views"][10];
        assert_eq!(route["kind"], "route");
        let routes = &route["entries"][0]["routes"];

        let ids = routes["nodes"].as_array().expect("nodes").iter();
        let ids = ids
            .map(|node| node["id"].as_str().expect("id"))
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "receive-location:OrdersIn",
                "subscription:billing",
                "work-process:Approval",
                "send-port:Billing"
            ]
        );
        assert_eq!(routes["nodes"][1]["summary"], "MessageType = 'Order'");
        assert_eq!(routes["nodes"][3]["x"], MARGIN + 3 * COLUMN);
        assert_eq!(routes["edges"].as_array().expect("edges").len(), 2);
        assert_eq!(routes["operators"][0], "=");
        assert_eq!(routes["kinds"][3], "expression");
        assert_eq!(
            views["views"][3]["entries"][0]["section"],
            json!(["receive_locations", "drop"])
        );
        assert_eq!(views["views"][2]["defined"], true);
        assert_eq!(
            views["views"][2]["entries"][0]["section"],
            json!(["xmip_applications", "Orders", "receive_ports", "Orders"])
        );
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
            &of_orders(&json!({ "add-subscription": {
            "id": "approval", "target": "work-process:Approval" } })),
        );
        assert!(
            added.ends_with(
                "\n[[xmip_applications.subscriptions]]\nid = \"approval\"\n\
            destination = { work-process = \"Approval\" }\nfilter = \"true\"\n"
            ),
            "{added}"
        );

        let filtered = edited(
            &runtime,
            &of_orders(&json!({ "set-filter": { "subscription": "billing",
            "filter": { "shape": "condition", "property": "Amount", "operator": ">",
                        "value": "1000", "kind": "integer" } } })),
        );
        assert!(
            filtered.contains("filter = \"Amount > 1000\"\n"),
            "{filtered}"
        );

        let connected = edited(
            &runtime,
            &of_orders(&json!({ "connect": {
            "subscription": "billing", "target": "work-process:Approval" } })),
        );
        assert_eq!(
            connected,
            ORDERS.replace(
                "{ send-port = \"Billing\" }",
                "{ work-process = \"Approval\" }"
            )
        );

        for (edit, name) in [
            ("add-receive-location", "Drop"),
            ("add-work-process", "Audit"),
            ("add-send-port", "Ledger"),
        ] {
            let added = edited(&runtime, &of_orders(&json!({ edit: { "name": name } })));
            assert!(added.contains(&format!("name = \"{name}\"\n")), "{added}");
        }

        let moved = edited(
            &runtime,
            &json!({ "set": { "section": ["receive_locations", "drop"],
                              "key": ["address"], "value": "\"/srv/in\"" } }),
        );
        assert_eq!(moved, ORDERS.replace("\"/in\"", "\"/srv/in\""));

        let refused = answer(
            &runtime,
            EDIT,
            &json!({ "edit": of_orders(&json!({ "connect": {
            "subscription": "billing", "target": "send-port:Audit" } })) }),
            Some(ORDERS),
        );
        assert!(
            matches!(refused, Err(Unanswered::Refused(_))),
            "{refused:?}"
        );
    }
}
