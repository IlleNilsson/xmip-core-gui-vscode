//! A Work Process design read, edited and compiled for the Work Process
//! designer (ADR-0066, amendment 2026-10-22). Here and not in the
//! runtime's library: a platform service names no technology
//! (`architecture.toml`), and the design formats are technologies, which
//! the developer's tooling carries.
//!
//! Nothing here is a rule. How a design is drawn and edited is its
//! format's (`xmip-core-process-bpmn`'s diagram and edits); what it
//! compiles to is `xmip-core-process`'s program and the unit generated
//! from it. This picks the format a design names, BPMN 2.0 where it names
//! none, and writes the answers as JSON, in memory only.

use process::design::{DEFAULT_FORMAT, DesignFormat, NoResources, chosen};
use process::generate::{Origin, unit};
use process_bpmn::Bpmn;
use process_bpmn::edit::Edit;
use serde::Deserialize;

/// The design formats this runtime carries: BPMN 2.0, the default.
pub const FORMATS: [&dyn DesignFormat; 1] = [&Bpmn];

/// A BPMN design as the designer draws it ([`process_bpmn::diagram`]).
///
/// # Errors
/// The design does not read as one.
pub fn view(design: &str) -> Result<String, String> {
    let drawn = process_bpmn::diagram::diagram(design)?;
    serde_json::to_string(&drawn).map_err(|error| error.to_string())
}

/// A BPMN design with an edit made to it ([`process_bpmn::edit`]).
///
/// # Errors
/// The JSON is not an edit, or the design refuses it.
pub fn edit(design: &str, edit: &str) -> Result<String, String> {
    let edit: Edit =
        serde_json::from_str(edit).map_err(|error| format!("not a design edit: {error}"))?;
    process_bpmn::edit::apply(design, &edit)
}

/// What a compile is asked with: the design's file, its format where it
/// names one, the module's name and version.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asked {
    design: String,
    #[serde(default)]
    format: Option<String>,
    module: String,
    #[serde(default = "first_version")]
    version: (u32, u32, u32),
}

const fn first_version() -> (u32, u32, u32) {
    (0, 1, 0)
}

/// A design compiled into its Work Process unit: `{cargo, library,
/// module}`, the crate's two files and its `[[modules]]` entry
/// ([`process::generate`]).
///
/// # Errors
/// The JSON is not a compile's, no format of its name is carried, or the
/// design does not compile — every refusal said, each by its element.
pub fn compile(design: &str, asked: &str) -> Result<String, String> {
    let asked: Asked =
        serde_json::from_str(asked).map_err(|error| format!("not a compile: {error}"))?;
    if asked.module.trim().is_empty() {
        return Err("a Work Process unit needs a module name".to_string());
    }
    let format = chosen(&FORMATS, asked.format.as_deref())?;
    let program = format.compile(design, &NoResources).map_err(|refusals| {
        let said: Vec<String> = refusals.iter().map(ToString::to_string).collect();
        said.join("\n")
    })?;
    let generated = unit(
        &program,
        &Origin {
            design: &asked.design,
            format: asked.format.as_deref().unwrap_or(DEFAULT_FORMAT),
            module: &asked.module,
            version: asked.version,
        },
    );
    serde_json::to_string(&serde_json::json!({
        "cargo": generated.cargo,
        "library": generated.library,
        "module": generated.module,
        "crate": process::generate::crate_name(&asked.module),
    }))
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESIGN: &str = "<definitions xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\" \
                          id=\"D\" targetNamespace=\"urn:example\"><process id=\"Orders\" \
                          isExecutable=\"true\"><startEvent id=\"S\"/><endEvent id=\"E\"/>\
                          <sequenceFlow id=\"F\" sourceRef=\"S\" targetRef=\"E\"/></process>\
                          </definitions>";

    #[test]
    fn a_design_is_drawn_edited_and_compiled_to_its_unit() {
        let drawn: serde_json::Value =
            serde_json::from_str(&view(DESIGN).expect("drawn")).expect("JSON");
        assert_eq!(drawn["process"], "Orders");
        assert_eq!(drawn["nodes"].as_array().map(Vec::len), Some(2));
        let renamed =
            edit(DESIGN, r#"{"edit":"rename","id":"S","name":"Order in"}"#).expect("edited");
        assert!(renamed.contains("name=\"Order in\""), "{renamed}");
        let compiled: serde_json::Value = serde_json::from_str(
            &compile(DESIGN, r#"{"design":"orders.bpmn","module":"orders-flow"}"#)
                .expect("compiled"),
        )
        .expect("JSON");
        assert!(
            compiled["library"]
                .as_str()
                .is_some_and(|l| l.contains("export_work_process!"))
        );
        assert!(
            compiled["module"]
                .as_str()
                .is_some_and(|m| m.contains("offline = true"))
        );
        assert_eq!(compiled["crate"], "orders-flow");
    }

    #[test]
    fn a_format_not_carried_and_a_design_that_does_not_compile_are_refused() {
        let asked = r#"{"design":"a.sw","format":"open-workflow","module":"m"}"#;
        let refused = compile(DESIGN, asked).expect_err("refused");
        assert!(
            refused.contains("no Work Process design format 'open-workflow'"),
            "{refused}"
        );
        let broken = DESIGN.replace(
            "<sequenceFlow id=\"F\" sourceRef=\"S\" targetRef=\"E\"/>",
            "",
        );
        let refused = compile(&broken, r#"{"design":"a.bpmn","module":"m"}"#)
            .expect_err("no");
        assert!(refused.contains("S: leads nowhere"), "{refused}");
    }
}
