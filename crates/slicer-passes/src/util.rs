use slicer_parser::ByteRange;

pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    pub fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        }
    }
}

pub fn extend_to_line(source: &str, range: ByteRange) -> ByteRange {
    let start = source[..range.start]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = source[range.end..]
        .find('\n')
        .map(|i| range.end + i + 1)
        .unwrap_or(range.end);

    ByteRange::new(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extend_to_line() {
        let source = "line1\nline2\nline3";
        let range = ByteRange::new(7, 10); // "ne2"
        let extended = extend_to_line(source, range);
        assert_eq!(extended.start, 6); // start of "line2"
        assert_eq!(extended.end, 12); // after newline
    }

    #[test]
    fn test_line_index_single_line() {
        let idx = LineIndex::new("hello");
        assert_eq!(idx.line_of(0), 1);
        assert_eq!(idx.line_of(3), 1);
    }

    #[test]
    fn test_line_index_multi_line() {
        let source = "line1\nline2\nline3\n";
        let idx = LineIndex::new(source);
        assert_eq!(idx.line_of(0), 1); // 'l' of line1
        assert_eq!(idx.line_of(5), 1); // '\n' after line1
        assert_eq!(idx.line_of(6), 2); // 'l' of line2
        assert_eq!(idx.line_of(12), 3); // 'l' of line3
    }

    #[test]
    fn test_line_index_empty() {
        let idx = LineIndex::new("");
        assert_eq!(idx.line_of(0), 1);
    }
}
