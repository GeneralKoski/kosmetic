//! Python packaging syntax: project names, PEP 440 versions and specifiers,
//! and PEP 508 requirements. Only the grammar is checked, not whether a
//! package or a marker exists.

/// A project name: letters, digits, `.`, `_` and `-`, starting and ending
/// with a letter or a digit.
pub(crate) fn is_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    let edge = |b: &u8| b.is_ascii_alphanumeric();
    bytes.first().is_some_and(edge)
        && bytes.last().is_some_and(edge)
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn digits(text: &str) -> &str {
    let end = text
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(text.len());
    &text[..end]
}

fn skip_separator(text: &str) -> &str {
    text.strip_prefix(['.', '-', '_']).unwrap_or(text)
}

/// Consumes one of `words` (the first that matches) and the number after it.
fn keyword<'a>(text: &'a str, words: &[&str]) -> Option<&'a str> {
    let text = skip_separator(text);
    let rest = words.iter().find_map(|word| text.strip_prefix(word))?;
    let rest = skip_separator(rest);
    Some(&rest[digits(rest).len()..])
}

/// A PEP 440 version, in any of its accepted spellings:
/// `1.0`, `2!1.0.post1`, `1.0rc1`, `1.0-1`, `1.0.dev3+local.7`.
pub(crate) fn is_version(text: &str) -> bool {
    let text = text.trim().to_ascii_lowercase();
    let mut rest = text.strip_prefix('v').unwrap_or(&text);

    if let Some((epoch, after)) = rest.split_once('!') {
        if epoch.is_empty() || !epoch.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        rest = after;
    }

    let first = digits(rest);
    if first.is_empty() {
        return false;
    }
    rest = &rest[first.len()..];
    while let Some(after) = rest.strip_prefix('.') {
        let number = digits(after);
        if number.is_empty() {
            break;
        }
        rest = &after[number.len()..];
    }

    if let Some(after) = keyword(
        rest,
        &["alpha", "beta", "preview", "pre", "rc", "a", "b", "c"],
    ) {
        rest = after;
    }
    if let Some(after) = rest.strip_prefix('-').filter(|a| !digits(a).is_empty()) {
        rest = &after[digits(after).len()..];
    } else if let Some(after) = keyword(rest, &["post", "rev", "r"]) {
        rest = after;
    }
    if let Some(after) = keyword(rest, &["dev"]) {
        rest = after;
    }

    match rest.strip_prefix('+') {
        None => rest.is_empty(),
        Some(local) => {
            !local.is_empty()
                && local
                    .split(['.', '-', '_'])
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
        }
    }
}

/// One clause of a version specifier: an operator and a version, like
/// `>=3.8` or `==1.2.*`.
fn is_clause(text: &str) -> bool {
    let text = text.trim();
    let Some((op, rest)) = ["===", "~=", "==", "!=", "<=", ">=", "<", ">"]
        .iter()
        .find_map(|op| text.strip_prefix(op).map(|rest| (*op, rest)))
    else {
        return false;
    };
    let rest = rest.trim();
    if op == "===" {
        return !rest.is_empty() && !rest.contains(char::is_whitespace);
    }
    let rest = if matches!(op, "==" | "!=") {
        rest.strip_suffix(".*").unwrap_or(rest)
    } else {
        rest
    };
    is_version(rest)
}

/// Comma separated clauses, like `>=3.8,<4`.
pub(crate) fn is_specifier_set(text: &str) -> bool {
    text.split(',').all(is_clause)
}

/// Checks a PEP 508 requirement like `requests[security]>=2.0; python_version<"3.12"`
/// or `pkg @ https://example.com/pkg.whl`. Returns what is wrong, if anything.
pub(crate) fn check_requirement(text: &str) -> Result<(), &'static str> {
    let text = text.trim();
    let name_end = text
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')))
        .unwrap_or(text.len());
    if !is_name(&text[..name_end]) {
        return Err("expected a package name");
    }
    let mut rest = text[name_end..].trim_start();

    if let Some(after) = rest.strip_prefix('[') {
        let Some((extras, after)) = after.split_once(']') else {
            return Err("unclosed `[` in the extras");
        };
        if !extras
            .split(',')
            .map(str::trim)
            .all(|extra| extra.is_empty() || is_name(extra))
        {
            return Err("invalid extra name");
        }
        rest = after.trim_start();
    }

    if let Some(url) = rest.strip_prefix('@') {
        let url = url.split(" ;").next().unwrap_or(url).trim();
        return if url.is_empty() || url.contains(char::is_whitespace) {
            Err("expected a URL after `@`")
        } else {
            Ok(())
        };
    }

    let (specifier, marker) = match rest.split_once(';') {
        Some((specifier, marker)) => (specifier.trim(), Some(marker.trim())),
        None => (rest.trim(), None),
    };
    if marker.is_some_and(str::is_empty) {
        return Err("expected an environment marker after `;`");
    }
    let specifier = specifier
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(specifier);
    if !specifier.is_empty() && !is_specifier_set(specifier) {
        return Err("invalid version specifier");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_requirement, is_name, is_specifier_set, is_version};

    #[test]
    fn names() {
        for good in ["a", "my-package", "My_Package.2", "x1"] {
            assert!(is_name(good), "{good}");
        }
        for bad in ["", "-a", "a-", "a b", "a/b", ".a", "é"] {
            assert!(!is_name(bad), "{bad}");
        }
    }

    #[test]
    fn versions() {
        for good in [
            "1",
            "1.0",
            "1.2.3",
            "2!1.0",
            "1.0a1",
            "1.0.alpha.1",
            "1.0rc2",
            "1.0b",
            "1.0.post1",
            "1.0-1",
            "1.0.dev3",
            "1.0rc1.post2.dev3",
            "1.0+ubuntu.1",
            "1.0.0-beta",
            "V1.0",
        ] {
            assert!(is_version(good), "{good}");
        }
        for bad in [
            "", "a", "1.", "1..0", "1.0x", "1.0+", "1.0+!", "!1", "1.0 beta", "latest",
        ] {
            assert!(!is_version(bad), "{bad}");
        }
    }

    #[test]
    fn specifiers() {
        for good in [
            ">=3.8",
            ">=3.8,<4",
            "~=1.4.2",
            "==1.*",
            "!=2.0",
            "===foo",
            ">= 3.8 , < 4",
            "==3.*",
        ] {
            assert!(is_specifier_set(good), "{good}");
        }
        for bad in ["", "3.8", ">=", ">=x", "=>3", ">=3.8,", "~=1.*"] {
            assert!(!is_specifier_set(bad), "{bad}");
        }
    }

    #[test]
    fn requirements() {
        for good in [
            "requests",
            "requests>=2.0",
            "requests >= 2.0, < 3",
            "requests[security,socks]>=2",
            "requests (>=2.0)",
            "pkg @ https://example.com/pkg-1.0.whl",
            "pywin32; sys_platform == 'win32'",
            "a>=1; python_version < '3.12'",
            "pkg[]",
        ] {
            assert_eq!(check_requirement(good), Ok(()), "{good}");
        }
        for (bad, why) in [
            ("", "expected a package name"),
            (">=1", "expected a package name"),
            ("pkg[a", "unclosed `[` in the extras"),
            ("pkg[!]", "invalid extra name"),
            ("pkg @", "expected a URL after `@`"),
            ("pkg>=", "invalid version specifier"),
            ("pkg 1.0", "invalid version specifier"),
            ("pkg;", "expected an environment marker after `;`"),
        ] {
            assert_eq!(check_requirement(bad), Err(why), "{bad}");
        }
    }
}
