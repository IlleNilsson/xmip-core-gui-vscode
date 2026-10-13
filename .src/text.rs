//! Bytes of a document and the protocol's positions, one turned into the
//! other: a line, and UTF-16 code units within it. The runtime answers in
//! bytes of the text it was given (`xmip-core-configure`'s `written`); the
//! editor speaks in positions; this is the one place either becomes the
//! other.

use serde_json::{Value, json};

/// The protocol's position of byte `offset` in `text`.
#[must_use]
pub fn position(text: &str, offset: usize) -> Value {
    let offset = floor(text, offset.min(text.len()));
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    let character = before[start..].encode_utf16().count();

    json!({ "line": line, "character": character })
}

/// The protocol's range of the bytes `[start, end]` in `text`.
#[must_use]
pub fn range(text: &str, bytes: [usize; 2]) -> Value {
    json!({ "start": position(text, bytes[0]), "end": position(text, bytes[1]) })
}

/// The byte a protocol position names in `text`, clamped to its line.
#[must_use]
pub fn offset(text: &str, at: &Value) -> Option<usize> {
    let line = usize::try_from(at["line"].as_u64()?).ok()?;
    let character = usize::try_from(at["character"].as_u64()?).ok()?;
    let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
    if line > 0 && start == 0 {
        return None;
    }
    let mut units = 0;
    let mut at = start;
    for c in text[start..].chars() {
        if c == '\n' || units >= character {
            break;
        }
        units += c.len_utf16();
        at += c.len_utf8();
    }
    Some(at)
}

/// The text of the line holding `offset`, up to `offset`.
#[must_use]
pub fn line_before(text: &str, offset: usize) -> &str {
    let offset = floor(text, offset.min(text.len()));
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    &text[start..offset]
}

/// The bytes of a written value without its quotes, where it has them.
#[must_use]
pub fn unquoted(text: &str, bytes: [usize; 2]) -> [usize; 2] {
    let written = text.get(bytes[0]..bytes[1]).unwrap_or_default();
    let quoted = written.len() >= 2
        && (written.starts_with('"') && written.ends_with('"')
            || written.starts_with('\'') && written.ends_with('\''));
    if quoted {
        [bytes[0] + 1, bytes[1] - 1]
    } else {
        bytes
    }
}

/// Whether `offset` is within `bytes`, its end included.
#[must_use]
pub fn holds(bytes: [usize; 2], offset: usize) -> bool {
    bytes[0] <= offset && offset <= bytes[1]
}

fn floor(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_and_its_byte_are_one_another() {
        let text = "a = 1\nname = \"\u{1F600}x\"\n";
        let x = text.find('x').expect("x");
        let at = position(text, x);
        assert_eq!(at, json!({ "line": 1, "character": 10 }), "UTF-16 units");
        assert_eq!(offset(text, &at), Some(x));
        assert_eq!(
            offset(text, &json!({ "line": 0, "character": 99 })),
            Some(5)
        );
        assert_eq!(line_before(text, x), "name = \"\u{1F600}");
        let quoted = [text.find('"').expect("a quote"), text.len() - 1];
        assert_eq!(
            &text[unquoted(text, quoted)[0]..unquoted(text, quoted)[1]],
            "\u{1F600}x"
        );
    }
}
