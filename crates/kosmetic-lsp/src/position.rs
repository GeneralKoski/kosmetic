//! Conversion between byte offsets and LSP positions: a zero-based line and a
//! column counted in UTF-16 code units.

pub(crate) struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self { text, starts }
    }

    /// Line and column of a byte offset. Offsets past the end are clamped.
    pub(crate) fn position(&self, offset: usize) -> (u32, u32) {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let column = self.text[self.starts[line]..offset].encode_utf16().count();
        (
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(column).unwrap_or(u32::MAX),
        )
    }

    /// Byte offset of a position. A line or column past the end is clamped to
    /// the end of the line or of the text.
    pub(crate) fn offset(&self, line: u32, character: u32) -> usize {
        let Some(&start) = self.starts.get(line as usize) else {
            return self.text.len();
        };
        let end = self
            .starts
            .get(line as usize + 1)
            .map_or(self.text.len(), |&next| next);
        let mut units = 0;
        for (i, c) in self.text[start..end].char_indices() {
            if units >= character || c == '\n' {
                return start + i;
            }
            units += u32::from(c.len_utf16() > 1) + 1;
        }
        end
    }

    /// Position after the last character.
    pub(crate) fn end(&self) -> (u32, u32) {
        self.position(self.text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::Lines;

    #[test]
    fn positions_count_utf16_units() {
        let text = "ab\ncé😀x\n";
        let lines = Lines::new(text);
        assert_eq!(lines.position(0), (0, 0));
        assert_eq!(lines.position(2), (0, 2));
        assert_eq!(lines.position(3), (1, 0));
        assert_eq!(lines.position(text.find('x').unwrap()), (1, 4));
        assert_eq!(lines.position(text.len()), (2, 0));
        assert_eq!(lines.end(), (2, 0));
    }

    #[test]
    fn offsets_invert_positions() {
        let text = "ab\ncé😀x\n";
        let lines = Lines::new(text);
        for offset in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
            let (line, column) = lines.position(offset);
            assert_eq!(lines.offset(line, column), offset);
        }
    }

    #[test]
    fn out_of_range_is_clamped() {
        let lines = Lines::new("ab\ncd");
        assert_eq!(lines.offset(0, 99), 2);
        assert_eq!(lines.offset(1, 99), 5);
        assert_eq!(lines.offset(9, 0), 5);
        assert_eq!(lines.position(99), (1, 2));
    }

    #[test]
    fn a_position_inside_a_surrogate_pair_moves_to_the_next_character() {
        let lines = Lines::new("😀x");
        assert_eq!(lines.offset(0, 1), 4);
    }
}
