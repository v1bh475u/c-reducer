//! Source location and byte range types.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "start must be <= end");
        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    pub fn overlaps(&self, other: &ByteRange) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn extract<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }

    pub fn to_range(&self) -> Range<usize> {
        self.start..self.end
    }
}

impl From<Range<usize>> for ByteRange {
    fn from(range: Range<usize>) -> Self {
        Self::new(range.start, range.end)
    }
}

impl From<ByteRange> for Range<usize> {
    fn from(range: ByteRange) -> Self {
        range.start..range.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_range() {
        let range = ByteRange::new(10, 20);
        assert_eq!(range.len(), 10);
        assert!(!range.is_empty());
        assert!(range.contains(15));
    }

    #[test]
    fn test_range_overlap() {
        let a = ByteRange::new(10, 20);
        let b = ByteRange::new(15, 25);
        let c = ByteRange::new(25, 30);
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }
}
