//! Utility Functions
//!
//! Shared helpers used by reduction passes.

use slicer_parser::ByteRange;

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
}
