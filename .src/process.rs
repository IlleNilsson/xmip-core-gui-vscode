//! The Work Process designer's requests (ADR-0066, amendment 2026-10-22):
//! what the VS Code extension's webview asks of a `.bpmn` design, answered
//! by the language server itself, through the design formats it carries.
//!
//! - `xmip/process` `{ text }` — the design as it is drawn: every step with
//!   its shape, every flow with its waypoints, what keeps it from compiling
//!   and the kinds a palette offers;
//! - `xmip/processEdit` `{ text, edit }` — an edit made in place, answered
//!   as the one text edit that makes it, so the webview never writes XML;
//! - `xmip/processCompile` `{ text, design, module, format?, version? }` —
//!   the design compiled into its Work Process unit's crate and module
//!   entry.
//!
//! What a design means, how it is drawn and edited and what it compiles to
//! are the design format's and `xmip-core-process`'s (`process_design`);
//! nothing here is a rule.

use serde_json::{Value, json};

use crate::designer::{Unanswered, parsed, text_edit};
use crate::process_design;

/// The methods this file answers.
pub const METHODS: [&str; 3] = [VIEW, EDIT, COMPILE];

const VIEW: &str = "xmip/process";
const EDIT: &str = "xmip/processEdit";
const COMPILE: &str = "xmip/processCompile";

/// Answer `method` over `params`; `document` is the design's text.
///
/// # Errors
/// [`Unanswered`], saying which of the three it was.
pub fn answer(method: &str, params: &Value, document: Option<&str>) -> Result<Value, Unanswered> {
    let needed = |key: &str| Unanswered::Params(format!("{method} needs {key}"));
    let text = document.ok_or_else(|| needed("a text or an open uri"))?;
    match method {
        VIEW => parsed(&process_design::view(text).map_err(Unanswered::Refused)?),
        EDIT => {
            let edit = params.get("edit").ok_or_else(|| needed("an edit"))?;
            let edited =
                process_design::edit(text, &edit.to_string()).map_err(Unanswered::Refused)?;
            Ok(text_edit(text, &edited))
        }
        COMPILE => {
            let asked = compile_argument(params).ok_or_else(|| needed("a design and a module"))?;
            parsed(&process_design::compile(text, &asked.to_string()).map_err(Unanswered::Refused)?)
        }
        _ => Err(Unanswered::Params(format!(
            "{method} is not a Work Process designer request"
        ))),
    }
}

/// What a compile is asked with, from the request's parameters.
fn compile_argument(params: &Value) -> Option<Value> {
    let design = params["design"].as_str()?;
    let module = params["module"].as_str()?;
    let mut asked = json!({ "design": design, "module": module });
    if let Some(format) = params["format"].as_str() {
        asked["format"] = format.into();
    }
    if let Some(version) = params.get("version").filter(|v| v.is_array()) {
        asked["version"] = version.clone();
    }
    Some(asked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compile_is_asked_with_what_the_request_gives_and_nothing_else() {
        let asked = compile_argument(&json!({
            "text": "<x/>",
            "design": "orders.bpmn",
            "module": "orders-flow",
            "version": [1, 2, 3],
        }))
        .expect("asked");
        assert_eq!(
            asked,
            json!({ "design": "orders.bpmn", "module": "orders-flow", "version": [1, 2, 3] })
        );
        assert_eq!(compile_argument(&json!({ "design": "a.bpmn" })), None);
    }

    #[test]
    fn a_request_with_no_design_text_is_refused_naming_what_it_needs() {
        let refused = answer(VIEW, &json!({}), None);
        assert_eq!(
            refused,
            Err(Unanswered::Params(
                "xmip/process needs a text or an open uri".into()
            ))
        );
    }
}
