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
        assert_eq!(range.extract("0123456789abcdefghij"), Some("abcdefghij"));
    }

    #[test]
    fn test_to_range() {
        let range = ByteRange::new(5, 15);
        let std_range: Range<usize> = range.into();
        assert_eq!(std_range, 5..15);
    }
}
