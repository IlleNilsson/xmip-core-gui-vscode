//! JSON-RPC framing as the Language Server Protocol specifies it: a block of
//! `Name: value` header lines ended by a blank line, then exactly
//! `Content-Length` bytes of JSON. Written by hand over `std` because this is
//! all the protocol needs at the transport level, and a server that owns its
//! twenty lines of framing has nothing to wait on.

use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Read one message. `Ok(None)` when the input has ended cleanly before a
/// message began; an error when a message began and could not be finished.
///
/// # Errors
/// Returns the underlying read error, or `InvalidData` when the header block
/// has no `Content-Length`, the length is not a number, or the body is not
/// JSON.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();

        if reader.read_line(&mut line)? == 0 {
            return if length.is_none() {
                Ok(None)
            } else {
                Err(invalid("input ended inside a message header"))
            };
        }

        let header = line.trim_end_matches(['\r', '\n']);

        if header.is_empty() {
            break;
        }

        if let Some(value) = header.strip_prefix("Content-Length:") {
            let parsed = value.trim().parse::<usize>();
            length = Some(parsed.map_err(|_| invalid("Content-Length is not a number"))?);
        }
    }

    let length = length.ok_or_else(|| invalid("a message without Content-Length"))?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;

    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| invalid(&format!("the message body is not JSON: {error}")))
}

/// Write one message, header and body, and flush it so the client sees it now.
///
/// # Errors
/// Returns the underlying write error.
pub fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = message.to_string();

    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    writer.flush()
}

fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn a_message_written_is_the_message_read() {
        let message = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}});
        let mut wire = Vec::new();

        write_message(&mut wire, &message).expect("writes");

        assert!(wire.starts_with(b"Content-Length: "));

        let mut reader = Cursor::new(wire);
        let read = read_message(&mut reader)
            .expect("reads")
            .expect("a message");

        assert_eq!(read, message);
        assert!(
            read_message(&mut reader).expect("reads").is_none(),
            "then the end"
        );
    }

    #[test]
    fn two_messages_back_to_back_are_read_in_order() {
        let mut wire = Vec::new();

        write_message(&mut wire, &json!({"id": 1})).expect("writes");
        write_message(&mut wire, &json!({"id": 2})).expect("writes");

        let mut reader = Cursor::new(wire);

        assert_eq!(
            read_message(&mut reader).expect("reads"),
            Some(json!({"id": 1}))
        );
        assert_eq!(
            read_message(&mut reader).expect("reads"),
            Some(json!({"id": 2}))
        );
    }

    #[test]
    fn other_headers_are_tolerated_and_the_length_is_what_counts() {
        let body = r#"{"id":3}"#;
        let text = format!(
            "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut reader = Cursor::new(text.into_bytes());

        assert_eq!(
            read_message(&mut reader).expect("reads"),
            Some(json!({"id": 3}))
        );
    }

    #[test]
    fn a_header_block_without_a_length_is_refused() {
        let mut reader = Cursor::new(b"Content-Type: text\r\n\r\n{}".to_vec());
        let error = read_message(&mut reader).expect_err("refused");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_body_cut_short_is_an_error_rather_than_a_message() {
        let mut reader = Cursor::new(b"Content-Length: 20\r\n\r\n{}".to_vec());

        assert!(read_message(&mut reader).is_err());
    }
}
