use crate::Fix;

/// Applies `fixes` to `source`. Fixes are applied in source order; one that
/// overlaps an already applied fix, or that does not fit the source, is
/// skipped. Returns the new text and the number of fixes applied.
pub fn apply_fixes<'a>(source: &str, fixes: impl IntoIterator<Item = &'a Fix>) -> (String, usize) {
    let mut fixes: Vec<_> = fixes.into_iter().collect();
    fixes.sort_by_key(|fix| fix.span);

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut applied = 0;
    for fix in fixes {
        let range = fix.span.range();
        if range.start < cursor
            || range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            continue;
        }
        out.push_str(&source[cursor..range.start]);
        out.push_str(&fix.replacement);
        cursor = range.end;
        applied += 1;
    }
    out.push_str(&source[cursor..]);
    (out, applied)
}

#[cfg(test)]
mod tests {
    use super::apply_fixes;
    use crate::{Fix, Span};

    fn fix(start: u32, end: u32, replacement: &str) -> Fix {
        Fix {
            span: Span::new(start, end),
            replacement: replacement.to_owned(),
        }
    }

    #[test]
    fn applies_fixes_in_source_order() {
        let fixes = [fix(4, 5, "X"), fix(0, 1, ""), fix(2, 2, "-")];
        assert_eq!(apply_fixes("abcdef", &fixes), ("b-cdXf".to_owned(), 3));
    }

    #[test]
    fn skips_overlapping_fixes() {
        let fixes = [fix(1, 4, "X"), fix(2, 3, "Y"), fix(4, 5, "Z")];
        assert_eq!(apply_fixes("abcdef", &fixes), ("aXZf".to_owned(), 2));
    }

    #[test]
    fn skips_fixes_outside_the_source() {
        assert_eq!(
            apply_fixes("é", &[fix(1, 2, "x"), fix(0, 9, "y")]),
            ("é".to_owned(), 0)
        );
    }

    #[test]
    fn no_fixes_is_identity() {
        assert_eq!(apply_fixes("abc", &[]), ("abc".to_owned(), 0));
    }
}
