//! A document as the runtime reads it, for the editor's suggestions and
//! lookups (ADR-0064, amendment 2026-10-13).
//!
//! One call: `xmip_cluster_views_v1` (`xmip_operate.h` section 10), whose
//! answer is `xmip-core-configure`'s — the views, where each table and key is
//! written, what names what, and every key a table may hold — with the
//! runtime's `[tuning]` declaration beside it. Nothing here knows a key or a
//! kind: this finds, in that answer, what is at a byte of the text, and
//! which entry a table belongs to. The server keeps one per open document
//! and reads it again when the text changes.

use abi::operate::design::CLUSTER_VIEWS_ENTRYPOINT;
use serde_json::Value;

use crate::designer::{Unanswered, ask, parsed};
use crate::runtime::Runtime;
use crate::text::{holds, unquoted};

/// What an entry no view lists holds of its values: nothing the views say.
static NOTHING: Value = Value::Null;

/// The runtime's reading of one text.
pub struct Model {
    pub text: String,
    pub answer: Value,
}

/// An entry of the views: an Xmip Application, a Port, a Location, a node,
/// a cluster's table.
#[derive(Clone, Debug)]
pub struct Entry<'m> {
    /// Its view's kind, `send-port`, and title, `Send Port`.
    pub kind: &'m str,
    pub title: String,
    pub section: Vec<String>,
    pub name: &'m str,
    pub scope: &'m str,
    /// The entry as the views answer it, its fields among it.
    pub value: &'m Value,
}

impl Model {
    /// Ask the runtime to read `text`.
    ///
    /// # Errors
    /// The runtime could not be asked, or refused the text.
    pub fn read(runtime: &Runtime, text: &str) -> Result<Self, Unanswered> {
        let views = ask(runtime, CLUSTER_VIEWS_ENTRYPOINT, text, "")?;
        Ok(Self {
            text: text.to_string(),
            answer: parsed(&views)?,
        })
    }

    /// Read `text`; while it does not read, read it with the line holding
    /// `offset` blank — its bytes kept, so every place stays where it is —
    /// so a line half typed still has its table around it.
    ///
    /// # Errors
    /// As [`Model::read`], when neither reads.
    pub fn read_around(runtime: &Runtime, text: &str, offset: usize) -> Result<Self, Unanswered> {
        let refused = match Self::read(runtime, text) {
            Ok(model) => return Ok(model),
            Err(refused) => refused,
        };
        let offset = (0..=offset.min(text.len()))
            .rev()
            .find(|at| text.is_char_boundary(*at))
            .unwrap_or_default();
        let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
        let end = text[offset..]
            .find('\n')
            .map_or(text.len(), |at| offset + at);
        let blank = format!(
            "{}{}{}",
            &text[..start],
            " ".repeat(end - start),
            &text[end..]
        );
        let answer = Self::read(runtime, &blank).map_err(|_| refused)?.answer;
        Ok(Self {
            text: text.to_string(),
            answer,
        })
    }

    /// Every table as written.
    pub fn written(&self) -> &[Value] {
        list(&self.answer["written"])
    }

    /// What names what.
    pub fn references(&self) -> &[Value] {
        list(&self.answer["references"])
    }

    /// Every compiled unit the file's modules declare, each with what a
    /// call to it may give.
    pub fn units(&self) -> &[Value] {
        list(&self.answer["units"])
    }

    /// The runtime's declaration of `[tuning]`.
    pub fn tuning(&self) -> &Value {
        &self.answer["tuning"]
    }

    /// The table written at `section`.
    pub fn table(&self, section: &[String]) -> Option<&Value> {
        self.written()
            .iter()
            .find(|written| strings(&written["section"]) == section)
    }

    /// The table whose text holds `offset`: the one whose header opens it,
    /// or the top of the file before any header.
    pub fn table_at(&self, offset: usize) -> Option<&Value> {
        self.written()
            .iter()
            .filter(|written| !written["header"].is_null() || list(&written["section"]).is_empty())
            .filter(|written| {
                bytes(&written["extent"]).is_some_and(|at| at[0] <= offset && offset < at[1])
            })
            .max_by_key(|written| bytes(&written["extent"]).map_or(0, |at| at[0]))
            .or_else(|| self.table(&[]))
    }

    /// The keys a table written as `table` may hold, a node's name standing
    /// where the keys say `*`.
    pub fn keys_of(&self, table: &[String]) -> Vec<&Value> {
        list(&self.answer["keys"])
            .iter()
            .filter(|key| {
                let pattern = list(&key["table"]);
                pattern.len() == table.len()
                    && pattern
                        .iter()
                        .zip(table)
                        .all(|(said, step)| said == "*" || said.as_str() == Some(step))
            })
            .collect()
    }

    /// The entry of the views at `section`; or, for a list's entry no view
    /// lists (a Subscription), the entry the file writes, titled by its list.
    pub fn entry(&self, section: &[String]) -> Option<Entry<'_>> {
        self.viewed(section).or_else(|| {
            let written = self.table(section)?;
            let table = list(&written["table"]);
            let listed = table.last()?.as_str()?;
            let name = section.last()?;
            let is_entry = section.len() >= 2 && listed != name && !written["name"].is_null();
            is_entry.then(|| Entry {
                kind: listed,
                title: title_of(listed),
                section: section.to_vec(),
                name: bytes(&written["name"]).map_or("", |at| {
                    &self.text[unquoted(&self.text, at)[0]..unquoted(&self.text, at)[1]]
                }),
                scope: "",
                value: &NOTHING,
            })
        })
    }

    fn viewed(&self, section: &[String]) -> Option<Entry<'_>> {
        list(&self.answer["views"]).iter().find_map(|view| {
            list(&view["entries"])
                .iter()
                .find(|entry| strings(&entry["section"]) == section)
                .map(|entry| Entry {
                    kind: view["kind"].as_str().unwrap_or_default(),
                    title: view["title"].as_str().unwrap_or_default().to_string(),
                    section: section.to_vec(),
                    name: entry["name"].as_str().unwrap_or_default(),
                    scope: entry["scope"].as_str().unwrap_or_default(),
                    value: entry,
                })
        })
    }

    /// The entry a table at `section` belongs to: itself, or the nearest
    /// entry it is within.
    pub fn owner(&self, section: &[String]) -> Option<Entry<'_>> {
        (1..=section.len())
            .rev()
            .find_map(|length| self.entry(&section[..length]))
    }

    /// Every entry the file writes, in the order it writes them.
    pub fn entries(&self) -> Vec<Entry<'_>> {
        self.written()
            .iter()
            .filter_map(|written| self.entry(&strings(&written["section"])))
            .collect()
    }

    /// Where a reference's name is written.
    pub fn value_of(&self, reference: &Value) -> Option<[usize; 2]> {
        let table = self.table(&strings(&reference["holder"]))?;
        list(&table["keys"])
            .iter()
            .find(|key| key["key"] == reference["key"] && key["index"] == reference["index"])
            .and_then(|key| bytes(&key["value"]))
    }

    /// The written reference whose name holds `offset`, and where it is.
    pub fn reference_at(&self, offset: usize) -> Option<(&Value, [usize; 2])> {
        self.references()
            .iter()
            .filter(|reference| !reference["name"].is_null())
            .find_map(|reference| {
                let at = self.value_of(reference)?;
                holds(at, offset).then_some((reference, at))
            })
    }

    /// The entry whose name or header holds `offset`.
    pub fn declaration_at(&self, offset: usize) -> Option<Entry<'_>> {
        self.written().iter().find_map(|written| {
            let on = |at: &Value| bytes(at).is_some_and(|at| holds(at, offset));
            if on(&written["name"]) || on(&written["header"]) {
                self.entry(&strings(&written["section"]))
            } else {
                None
            }
        })
    }

    /// The references naming what is declared at `section`.
    pub fn referring(&self, section: &[String]) -> Vec<&Value> {
        self.references()
            .iter()
            .filter(|reference| strings(&reference["target"]) == section)
            .collect()
    }

    /// What names `entry` in the file: its name's bytes.
    pub fn name_of(&self, section: &[String]) -> Option<[usize; 2]> {
        let written = self.table(section)?;
        bytes(&written["name"]).or_else(|| bytes(&written["header"]))
    }
}

/// How a person reads a list's key as one of its entries:
/// `subscriptions`, a Subscription.
fn title_of(list: &str) -> String {
    let one = list.strip_suffix('s').unwrap_or(list);
    one.split('_')
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(letters).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A JSON array's items, or none.
pub fn list(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

/// A JSON array of strings.
pub fn strings(value: &Value) -> Vec<String> {
    list(value)
        .iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

/// A `[start, end]` of bytes.
pub fn bytes(value: &Value) -> Option<[usize; 2]> {
    let pair = value.as_array()?;
    let at = |index: usize| usize::try_from(pair.get(index)?.as_u64()?).ok();
    Some([at(0)?, at(1)?])
}
