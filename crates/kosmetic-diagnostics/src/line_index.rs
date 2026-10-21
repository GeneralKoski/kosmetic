/// A 1-based line and column, where the column counts characters, not bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column, in Unicode scalar values.
    pub col: u32,
}

/// Maps byte offsets to line and column numbers.
///
/// Built once per file in O(n), then each lookup is a binary search.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    source: &'a str,
    /// Byte offset where each line starts. The first entry is always 0.
    line_starts: Vec<u32>,
}

impl<'a> LineIndex<'a> {
    /// Indexes `source`. Both `\n` and `\r\n` end a line.
    ///
    /// # Panics
    ///
    /// Panics if `source` is larger than `u32::MAX` bytes.
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                let next = u32::try_from(offset + 1).expect("source larger than 4 GiB");
                line_starts.push(next);
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    /// Line and column of a byte offset. Offsets past the end clamp to the end.
    ///
    /// # Panics
    ///
    /// Panics if a line has more than `u32::MAX` characters, which `new` already rules out.
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(u32::try_from(self.source.len()).unwrap_or(u32::MAX));
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        };
        let line_start = self.line_starts[line] as usize;
        let col = self.source[line_start..offset as usize].chars().count();
        LineCol {
            line: u32::try_from(line + 1).expect("checked in new"),
            col: u32::try_from(col + 1).expect("checked in new"),
        }
    }

    /// Text of a 1-based line, without its line ending.
    pub fn line_text(&self, line: u32) -> Option<&'a str> {
        let index = usize::try_from(line).ok()?.checked_sub(1)?;
        let start = *self.line_starts.get(index)? as usize;
        let end = self
            .line_starts
            .get(index + 1)
            .map_or(self.source.len(), |&next| next as usize);
        Some(self.source[start..end].trim_end_matches(['\n', '\r']))
    }
}

#[cfg(test)]
mod tests {
    use super::{LineCol, LineIndex};

    #[test]
    fn maps_offsets_across_lines() {
        let index = LineIndex::new("a = 1\nb = 2\n");
        assert_eq!(index.line_col(0), LineCol { line: 1, col: 1 });
        assert_eq!(index.line_col(4), LineCol { line: 1, col: 5 });
        assert_eq!(index.line_col(6), LineCol { line: 2, col: 1 });
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let index = LineIndex::new("città = \"Zürich\"");
        // "città " is 7 bytes but 6 characters, so the `=` is column 7.
        assert_eq!(index.line_col(7), LineCol { line: 1, col: 7 });
    }

    #[test]
    fn line_text_strips_crlf() {
        let index = LineIndex::new("a = 1\r\nb = 2");
        assert_eq!(index.line_text(1), Some("a = 1"));
        assert_eq!(index.line_text(2), Some("b = 2"));
        assert_eq!(index.line_text(3), None);
    }
}
