//! Cargo version syntax: semantic versions and version requirements.

fn is_number(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'))
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// `MAJOR.MINOR.PATCH`, optionally followed by `-pre.release` and `+build`.
pub(crate) fn is_version(text: &str) -> bool {
    let (rest, build) = match text.split_once('+') {
        Some((rest, build)) => (rest, Some(build)),
        None => (text, None),
    };
    if build.is_some_and(|build| !build.split('.').all(is_identifier)) {
        return false;
    }
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let numbers: Vec<_> = core.split('.').collect();
    let pre_ok = pre.is_none_or(|pre| {
        pre.split('.').all(|part| {
            is_identifier(part) && (!part.bytes().all(|b| b.is_ascii_digit()) || is_number(part))
        })
    });
    numbers.len() == 3 && numbers.iter().all(|part| is_number(part)) && pre_ok
}

fn is_wildcard(text: &str) -> bool {
    matches!(text, "*" | "x" | "X")
}

/// One comparator: an optional operator and a full or partial version,
/// possibly with a wildcard, like `^1.2`, `>= 1.2.3`, `1.*`.
fn is_comparator(text: &str) -> bool {
    let text = text.trim();
    let version = ["<=", ">=", "^", "~", "=", "<", ">"]
        .iter()
        .find_map(|op| text.strip_prefix(op))
        .unwrap_or(text)
        .trim_start();
    if version.contains(['-', '+']) {
        return is_version(version);
    }
    let parts: Vec<_> = version.split('.').collect();
    match parts.as_slice() {
        [major] => is_wildcard(major) || is_number(major),
        [major, minor] => is_number(major) && (is_number(minor) || is_wildcard(minor)),
        [major, minor, patch] => {
            is_number(major)
                && match (is_number(minor), is_wildcard(minor)) {
                    (true, _) => is_wildcard(patch) || is_number(patch),
                    (_, true) => is_wildcard(patch),
                    _ => false,
                }
        }
        _ => false,
    }
}

/// Comma separated comparators, like `>=1.2, <2`.
pub(crate) fn is_version_req(text: &str) -> bool {
    text.split(',').all(is_comparator)
}

/// A requirement that matches every version: `*`, or `*` in every comparator.
pub(crate) fn is_wildcard_req(text: &str) -> bool {
    text.split(',').all(|part| is_wildcard(part.trim()))
}

#[cfg(test)]
mod tests {
    use super::{is_version, is_version_req, is_wildcard_req};

    #[test]
    fn versions() {
        for good in [
            "0.1.0",
            "1.2.3",
            "10.20.30",
            "1.0.0-alpha.1",
            "1.0.0+build.5",
            "1.0.0-rc-1+x",
        ] {
            assert!(is_version(good), "{good}");
        }
        for bad in [
            "", "1", "1.2", "1.2.3.4", "01.2.3", "1.2.x", "1.2.3-", "1.2.3-01", "v1.2.3", "1.2.3+",
        ] {
            assert!(!is_version(bad), "{bad}");
        }
    }

    #[test]
    fn requirements() {
        for good in [
            "1",
            "1.2",
            "1.2.3",
            "^1.2",
            "~1.2.3",
            "=1.0.0",
            ">=1.2, <2",
            ">= 1.2.3",
            "1.*",
            "1.2.*",
            "*",
            "1.0.0-beta.2",
            "<=0.4",
        ] {
            assert!(is_version_req(good), "{good}");
        }
        for bad in [
            "", "abc", "1..2", ">=", "1.2.3.4", "^", "1.x.3", ">=1.2,", "latest", "1.2.3.*",
        ] {
            assert!(!is_version_req(bad), "{bad}");
        }
    }

    #[test]
    fn wildcards() {
        assert!(is_wildcard_req("*"));
        assert!(is_wildcard_req(" * "));
        assert!(!is_wildcard_req("1.*"));
        assert!(!is_wildcard_req(">=1"));
    }
}
