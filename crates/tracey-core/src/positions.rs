#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LineNumber(usize);

impl LineNumber {
    pub(crate) fn from_zero_based(index: usize) -> Self {
        Self(index + 1)
    }

    #[cfg(not(feature = "reverse"))]
    pub(crate) fn from_one_based(index: usize) -> Self {
        debug_assert!(index > 0, "line numbers are 1-based");
        Self(index)
    }

    pub(crate) fn as_usize(self) -> usize {
        self.0
    }

    pub(crate) fn is_immediately_after(self, previous: Self) -> bool {
        self.0 == previous.0.saturating_add(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ByteOffset(usize);

impl ByteOffset {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) fn from_usize(value: usize) -> Self {
        Self(value)
    }

    pub(crate) fn as_usize(self) -> usize {
        self.0
    }

    pub(crate) fn add(self, amount: usize) -> Self {
        Self(
            self.0
                .checked_add(amount)
                .expect("byte offset arithmetic overflowed"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteLength(usize);

impl ByteLength {
    pub(crate) fn as_usize(self) -> usize {
        self.0
    }

    fn from_inclusive_bounds(start: usize, end_inclusive: usize) -> Self {
        let length = end_inclusive
            .checked_sub(start)
            .and_then(|delta| delta.checked_add(1))
            .expect("inclusive byte bounds must be ordered");
        Self(length)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteSpan {
    offset: ByteOffset,
    length: ByteLength,
}

impl ByteSpan {
    pub(crate) fn from_relative_indices(
        base_offset: ByteOffset,
        start_idx: usize,
        end_idx_inclusive: usize,
    ) -> Self {
        Self {
            offset: base_offset.add(start_idx),
            length: ByteLength::from_inclusive_bounds(start_idx, end_idx_inclusive),
        }
    }

    pub(crate) fn offset(self) -> ByteOffset {
        self.offset
    }

    pub(crate) fn length(self) -> ByteLength {
        self.length
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RefLocation {
    line: LineNumber,
    span: ByteSpan,
}

impl RefLocation {
    pub(crate) fn from_relative_indices(
        line: LineNumber,
        base_offset: ByteOffset,
        start_idx: usize,
        end_idx_inclusive: usize,
    ) -> Self {
        Self {
            line,
            span: ByteSpan::from_relative_indices(base_offset, start_idx, end_idx_inclusive),
        }
    }

    pub(crate) fn line(self) -> LineNumber {
        self.line
    }

    pub(crate) fn span(self) -> ByteSpan {
        self.span
    }
}

#[cfg(not(feature = "reverse"))]
#[derive(Debug, Clone)]
pub(crate) struct LineStarts(Vec<ByteOffset>);

#[cfg(not(feature = "reverse"))]
impl LineStarts {
    pub(crate) fn from_content(content: &str) -> Self {
        let starts = std::iter::once(ByteOffset::ZERO)
            .chain(
                content
                    .match_indices('\n')
                    .map(|(idx, _)| ByteOffset::from_usize(idx + 1)),
            )
            .collect();
        Self(starts)
    }

    pub(crate) fn line_number_for_offset(&self, offset: ByteOffset) -> LineNumber {
        match self.0.binary_search(&offset) {
            Ok(line_index) => LineNumber::from_zero_based(line_index),
            Err(line_index) => LineNumber::from_one_based(line_index),
        }
    }

    pub(crate) fn line_start_for_index(&self, line_index: usize) -> ByteOffset {
        self.0.get(line_index).copied().unwrap_or(ByteOffset::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_span_uses_inclusive_end_indices() {
        let span = ByteSpan::from_relative_indices(ByteOffset::from_usize(10), 5, 9);
        assert_eq!(span.offset().as_usize(), 15);
        assert_eq!(span.length().as_usize(), 5);
    }

    #[cfg(not(feature = "reverse"))]
    #[test]
    fn line_starts_maps_offsets_to_one_based_lines() {
        let starts = LineStarts::from_content("a\nbc\ndef");
        assert_eq!(
            starts
                .line_number_for_offset(ByteOffset::from_usize(0))
                .as_usize(),
            1
        );
        assert_eq!(
            starts
                .line_number_for_offset(ByteOffset::from_usize(2))
                .as_usize(),
            2
        );
        assert_eq!(
            starts
                .line_number_for_offset(ByteOffset::from_usize(5))
                .as_usize(),
            3
        );
    }

    #[cfg(not(feature = "reverse"))]
    #[test]
    fn line_number_after_check_is_one_step() {
        let previous = LineNumber::from_one_based(7);
        let next = LineNumber::from_one_based(8);
        let skipped = LineNumber::from_one_based(9);
        assert!(next.is_immediately_after(previous));
        assert!(!skipped.is_immediately_after(previous));
    }
}
