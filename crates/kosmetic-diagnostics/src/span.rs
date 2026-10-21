use serde::Serialize;

/// A half-open byte range `start..end` in a source file.
///
/// Offsets are `u32` to keep tokens and tree nodes small: kosmetic does not
/// support files larger than 4 GiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
pub struct Span {
    /// First byte included in the range.
    pub start: u32,
    /// First byte after the range.
    pub end: u32,
}

impl Span {
    /// Creates a span. Panics in debug builds if `start > end`.
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "span start {start} is after end {end}");
        Self { start, end }
    }

    /// An empty span at `offset`, used for things like "missing value here".
    pub fn empty(offset: u32) -> Self {
        Self::new(offset, offset)
    }

    /// Length in bytes.
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    /// Whether the span covers zero bytes.
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Smallest span covering both `self` and `other`.
    #[must_use]
    pub fn cover(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }

    /// The span as a `usize` range, for slicing the source.
    pub fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

#[cfg(test)]
mod tests {
    use super::Span;

    #[test]
    fn cover_merges_ranges() {
        assert_eq!(Span::new(2, 5).cover(Span::new(4, 9)), Span::new(2, 9));
        assert_eq!(Span::new(4, 9).cover(Span::new(2, 5)), Span::new(2, 9));
    }

    #[test]
    fn empty_span() {
        let span = Span::empty(7);
        assert!(span.is_empty());
        assert_eq!(span.len(), 0);
    }
}
