//! LSP message framing: `Content-Length: N`, a blank line, then N bytes of JSON.

use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Messages larger than this are refused instead of allocated.
const MAX_MESSAGE: usize = 128 * 1024 * 1024;

/// Reads one message. Returns `None` at the end of the input. A body that is
/// not valid JSON comes back as `Value::Null`, which the server ignores, so
/// one bad message does not end the session.
///
/// # Errors
///
/// Fails on I/O errors, on a body shorter than announced and on a message
/// larger than 128 MiB.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let length = length.unwrap_or(0);
    if length > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message of {length} bytes is too large"),
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body).unwrap_or_else(|err| {
        eprintln!("kosmetic-lsp: ignoring a message that is not JSON: {err}");
        Value::Null
    })))
}

/// Writes one message and flushes.
///
/// # Errors
///
/// Fails on I/O errors.
pub fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = message.to_string();
    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::{read_message, write_message};

    #[test]
    fn round_trip() {
        let mut out = Vec::new();
        write_message(&mut out, &json!({"a": "é"})).unwrap();
        write_message(&mut out, &json!([1])).unwrap();
        let mut reader = Cursor::new(out);
        assert_eq!(read_message(&mut reader).unwrap(), Some(json!({"a": "é"})));
        assert_eq!(read_message(&mut reader).unwrap(), Some(json!([1])));
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn length_counts_bytes() {
        let mut out = Vec::new();
        write_message(&mut out, &json!("é")).unwrap();
        assert!(String::from_utf8_lossy(&out).starts_with("Content-Length: 4\r\n"));
    }

    #[test]
    fn other_headers_and_case_are_accepted() {
        let raw = b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}";
        assert_eq!(
            read_message(&mut Cursor::new(&raw[..])).unwrap(),
            Some(json!({}))
        );
    }

    #[test]
    fn invalid_json_is_null() {
        let raw = b"Content-Length: 3\r\n\r\n{{{";
        assert_eq!(
            read_message(&mut Cursor::new(&raw[..])).unwrap(),
            Some(json!(null))
        );
    }

    #[test]
    fn truncated_body_is_an_error() {
        let raw = b"Content-Length: 10\r\n\r\n{}";
        assert!(read_message(&mut Cursor::new(&raw[..])).is_err());
    }

    #[test]
    fn huge_length_is_refused() {
        let raw = b"Content-Length: 99999999999\r\n\r\n";
        assert!(read_message(&mut Cursor::new(&raw[..])).is_err());
    }
}
