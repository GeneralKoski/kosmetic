//! Shape checks for bare values. They only look at the grammar: ranges
//! (integer overflow, February 30) are checked by the value model.

use crate::{NodeKind, TokenKind};

/// Classifies the text of a bare value, or returns [`NodeKind::Error`].
pub(crate) fn classify(text: &str) -> NodeKind {
    if text == "true" || text == "false" {
        NodeKind::Boolean
    } else if is_integer(text) {
        NodeKind::Integer
    } else if is_float(text) {
        NodeKind::Float
    } else if datetime_kind(text).is_some() {
        NodeKind::DateTime
    } else {
        NodeKind::Error
    }
}

/// Non-empty digits in `radix`, with each `_` between two digits.
fn digits(text: &str, radix: u32) -> bool {
    text.split('_')
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_digit(radix)))
}

fn decimal(text: &str) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    digits(unsigned, 10) && (unsigned == "0" || !unsigned.starts_with('0'))
}

fn is_integer(text: &str) -> bool {
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return digits(rest, radix);
        }
    }
    decimal(text)
}

fn is_float(text: &str) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned == "inf" || unsigned == "nan" {
        return true;
    }
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (text, None),
    };
    let (int, fraction) = match mantissa.split_once('.') {
        Some((int, fraction)) => (int, Some(fraction)),
        None => (mantissa, None),
    };
    (fraction.is_some() || exponent.is_some())
        && decimal(int)
        && fraction.is_none_or(|fraction| digits(fraction, 10))
        && exponent.is_none_or(|exponent| {
            digits(exponent.strip_prefix(['+', '-']).unwrap_or(exponent), 10)
        })
}

/// The four TOML date-time types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimeKind {
    /// `1979-05-27T07:32:00Z`, `1979-05-27T07:32:00-07:00`
    Offset,
    /// `1979-05-27T07:32:00`
    Local,
    /// `1979-05-27`
    Date,
    /// `07:32:00`
    Time,
}

/// `YYYY-MM-DD`, digits only, no range check.
pub(crate) fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, &b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
}

/// `HH:MM:SS` with optional `.fraction`.
fn is_time(text: &str) -> bool {
    let (hms, fraction) = match text.split_once('.') {
        Some((hms, fraction)) => (hms, Some(fraction)),
        None => (text, None),
    };
    let bytes = hms.as_bytes();
    bytes.len() == 8
        && bytes.iter().enumerate().all(|(i, &b)| match i {
            2 | 5 => b == b':',
            _ => b.is_ascii_digit(),
        })
        && fraction.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
}

/// Which date-time `text` is, if any. Accepts `T`, `t` or a space as separator.
pub(crate) fn datetime_kind(text: &str) -> Option<DateTimeKind> {
    if is_date(text) {
        return Some(DateTimeKind::Date);
    }
    if is_time(text) {
        return Some(DateTimeKind::Time);
    }
    let (date, rest) = (text.get(..10)?, text.get(10..)?);
    let time = rest.strip_prefix(['T', 't', ' '])?;
    if !is_date(date) {
        return None;
    }
    if is_time(time) {
        return Some(DateTimeKind::Local);
    }
    let local = if let Some(local) = time.strip_suffix(['Z', 'z']) {
        local
    } else {
        let split = time.len().checked_sub(6)?;
        let (local, offset) = (time.get(..split)?, time.get(split..)?);
        let offset = offset.as_bytes();
        let valid = matches!(offset[0], b'+' | b'-')
            && offset[3] == b':'
            && [1, 2, 4, 5].iter().all(|&i| offset[i].is_ascii_digit());
        if !valid {
            return None;
        }
        local
    };
    is_time(local).then_some(DateTimeKind::Offset)
}

/// Decoded content of a string token: quotes removed, escapes resolved, and
/// the newline right after a multi-line opener trimmed. Other tokens are
/// returned as they are, which is what bare keys need. Never fails: invalid
/// escapes were already reported by the lexer and are kept verbatim.
pub fn string_value(kind: TokenKind, text: &str) -> String {
    let (quote, multiline, basic) = match kind {
        TokenKind::BasicString => ("\"", false, true),
        TokenKind::MultilineBasicString => ("\"\"\"", true, true),
        TokenKind::LiteralString => ("'", false, false),
        TokenKind::MultilineLiteralString => ("'''", true, false),
        _ => return text.to_owned(),
    };
    let body = text.strip_prefix(quote).unwrap_or(text);
    let body = body.strip_suffix(quote).unwrap_or(body);
    let body = if multiline {
        body.strip_prefix("\r\n")
            .or_else(|| body.strip_prefix('\n'))
            .unwrap_or(body)
    } else {
        body
    };
    if basic {
        unescape(body)
    } else {
        body.to_owned()
    }
}

fn unescape(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('b') => out.push('\u{8}'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('f') => out.push('\u{c}'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') | None => out.push('\\'),
            Some(u @ ('u' | 'U')) => {
                let len = if u == 'u' { 4 } else { 8 };
                let hex: String = chars.by_ref().take(len).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            // Line ending backslash: drop the whitespace and newlines that follow.
            Some(' ' | '\t' | '\n' | '\r') => {
                while chars
                    .next_if(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
                    .is_some()
                {}
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{DateTimeKind, classify, datetime_kind, string_value};
    use crate::NodeKind::*;
    use crate::TokenKind;

    #[test]
    fn decodes_strings() {
        let cases = [
            (
                TokenKind::BasicString,
                r#""a\tb\u00e9\U0001F600\"""#,
                "a\tbé😀\"",
            ),
            (TokenKind::LiteralString, r"'C:\path'", r"C:\path"),
            (
                TokenKind::MultilineBasicString,
                "\"\"\"\nab \\\n   c\"\"\"\"\"",
                "ab c\"\"",
            ),
            (TokenKind::MultilineLiteralString, "'''\r\nx\n'''", "x\n"),
            (TokenKind::BasicString, "\"unterminated", "unterminated"),
            (TokenKind::BasicString, r#""\x""#, r"\x"),
            (TokenKind::Bare, "key", "key"),
        ];
        for (kind, text, expected) in cases {
            assert_eq!(string_value(kind, text), expected, "{text:?}");
        }
    }

    #[test]
    fn classifies_values() {
        for text in ["0", "+42", "-17", "1_000", "0xDEAD_beef", "0o755", "0b1101"] {
            assert_eq!(classify(text), Integer, "{text}");
        }
        for text in [
            "3.14",
            "-0.0",
            "1e6",
            "6.626e-34",
            "1E+0_1",
            "inf",
            "-nan",
            "1e007",
        ] {
            assert_eq!(classify(text), Float, "{text}");
        }
        for text in ["true", "false"] {
            assert_eq!(classify(text), Boolean);
        }
        for text in [
            "1979-05-27T07:32:00Z",
            "1979-05-27 07:32:00.999-07:00",
            "1979-05-27t07:32:00",
            "1979-05-27",
            "07:32:00.5",
        ] {
            assert_eq!(classify(text), DateTime, "{text}");
        }
    }

    #[test]
    fn rejects_malformed_values() {
        for text in [
            "03",
            "1__0",
            "_1",
            "1_",
            "+0x1",
            "0X1",
            "0x",
            "1.",
            ".5",
            "1.e5",
            "1e",
            "01.5",
            "True",
            "inf1",
            "1979-5-27",
            "07:32",
            "1979-05-27T07:32:00+0700",
            "hello",
            "",
        ] {
            assert_eq!(classify(text), Error, "{text}");
        }
    }

    #[test]
    fn datetime_kinds() {
        assert_eq!(
            datetime_kind("1979-05-27T07:32:00Z"),
            Some(DateTimeKind::Offset)
        );
        assert_eq!(
            datetime_kind("1979-05-27T07:32:00"),
            Some(DateTimeKind::Local)
        );
        assert_eq!(datetime_kind("1979-05-27"), Some(DateTimeKind::Date));
        assert_eq!(datetime_kind("07:32:00"), Some(DateTimeKind::Time));
    }
}
