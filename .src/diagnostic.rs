//! The runtime's validation report as Language Server Protocol diagnostics.
//!
//! `xmip_validate_v1` answers with one problem per line, in the words
//! `xmip-core-configure` and the execution-tree validator use. This turns each
//! problem into a diagnostic the editor can show: at the line the problem
//! names when it names one, otherwise at the top of the document.

use serde_json::{Value, json};

/// The diagnostics for a report over the document `text` it came from. Empty
/// for an empty report: a clean document publishes an empty list, which is
/// how the editor clears what it showed before.
#[must_use]
pub fn diagnostics(report: &str, text: &str) -> Vec<Value> {
    problems(report)
        .iter()
        .map(|problem| {
            let (line, character) = position(problem);
            diagnostic(problem, &range(line, character, text))
        })
        .collect()
}

/// One diagnostic with a message and a range, as the protocol shapes it.
/// Severity 1 is Error: the runtime reports nothing it would start with.
#[must_use]
pub fn diagnostic(message: &str, range: &Value) -> Value {
    json!({
        "range": range,
        "severity": 1,
        "source": "xmip",
        "message": message,
    })
}

/// A range covering one whole line of `text`, or an empty range at the line's
/// start when the document is shorter than the report claims.
#[must_use]
pub fn range(line: u32, character: u32, text: &str) -> Value {
    let end = text
        .lines()
        .nth(line as usize)
        .map_or(character, |content| {
            u32::try_from(content.chars().count()).unwrap_or(u32::MAX)
        })
        .max(character);

    json!({
        "start": { "line": line, "character": character },
        "end": { "line": line, "character": end },
    })
}

/// The problems in a report, one per line — except a parse failure, which the
/// runtime reports alone because nothing is built from text that does not
/// parse, and whose message spans several lines with a source snippet in the
/// middle. That prefix is the runtime's own wording, in `service.rs` of
/// `xmip-core-runtime`.
fn problems(report: &str) -> Vec<String> {
    let trimmed = report.trim();

    if trimmed.is_empty() {
        return Vec::new();
    }

    if trimmed.starts_with("configuration parse failed") {
        return vec![trimmed.to_string()];
    }

    trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Zero-based line and character from a problem that says `line N` and
/// perhaps `column M`, the way a TOML parse error does. `(0, 0)` otherwise.
fn position(problem: &str) -> (u32, u32) {
    let line = number_after(problem, "line ").map_or(0, |n| n.saturating_sub(1));
    let character = number_after(problem, "column ").map_or(0, |n| n.saturating_sub(1));

    (line, character)
}

fn number_after(text: &str, marker: &str) -> Option<u32> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();

    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: &str = "[service]\nname = \"edge\"\ncluster_name = \"lab\"\n";

    #[test]
    fn a_report_with_three_lines_is_three_diagnostics() {
        let report = "first problem\nsecond problem at line 2\nthird problem\n";
        let found = diagnostics(report, DOCUMENT);

        assert_eq!(found.len(), 3);
        assert_eq!(found[0]["message"], "first problem");
        assert_eq!(found[0]["range"]["start"]["line"], 0);
        assert_eq!(
            found[0]["range"]["end"]["character"], 9,
            "the whole first line"
        );
        assert_eq!(
            found[1]["range"]["start"]["line"], 1,
            "line 2 named, zero-based"
        );
        assert_eq!(found[1]["range"]["end"]["character"], 13);
        assert_eq!(found[2]["range"]["start"]["line"], 0, "no line named");
        assert_eq!(found[2]["severity"], 1);
        assert_eq!(found[2]["source"], "xmip");
    }

    #[test]
    fn an_empty_report_is_an_empty_list() {
        assert!(diagnostics("", DOCUMENT).is_empty());
        assert!(diagnostics("\n  \n", DOCUMENT).is_empty());
    }

    #[test]
    fn a_parse_failure_is_one_diagnostic_at_the_line_and_column_it_names() {
        let report = "configuration parse failed: TOML parse error at line 3, column 16\n  |\n\
                      3 | cluster_name = \"lab\n  |                ^\ninvalid basic string\n";
        let found = diagnostics(report, DOCUMENT);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["range"]["start"]["line"], 2);
        assert_eq!(found[0]["range"]["start"]["character"], 15);
        assert_eq!(found[0]["range"]["end"]["character"], 20);
        assert!(
            found[0]["message"]
                .as_str()
                .expect("text")
                .contains("invalid basic string")
        );
    }

    #[test]
    fn a_line_beyond_the_document_gets_an_empty_range_there() {
        let found = diagnostics("missing at line 40, column 3", DOCUMENT);

        assert_eq!(found[0]["range"]["start"]["line"], 39);
        assert_eq!(found[0]["range"]["start"]["character"], 2);
        assert_eq!(found[0]["range"]["end"]["character"], 2);
    }

    #[test]
    fn a_line_with_no_number_after_it_is_line_zero() {
        assert_eq!(position("the line is wrong"), (0, 0));
        assert_eq!(position("at line 0"), (0, 0));
    }
}
