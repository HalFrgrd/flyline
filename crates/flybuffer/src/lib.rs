mod accessors;
mod editing_advanced;
mod editing_primitives;
mod movement;
mod selection;
mod substring;
mod undo_redo;

pub use substring::SubString;
pub use undo_redo::Snapshot;
pub(crate) use undo_redo::SnapshotManager;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WordDelim {
    WhiteSpace,
    FineGrained,
}

impl WordDelim {
    pub(crate) fn is_word_boundary(&self, c: char) -> bool {
        match self {
            WordDelim::WhiteSpace => c.is_whitespace(),
            WordDelim::FineGrained => c.is_whitespace() || c.is_ascii_punctuation(),
        }
    }
}

pub struct TextBuffer {
    pub(crate) buf: String,
    // Byte index of the cursor position in the buffer
    // Need to ensure it lines up with grapheme boundaries.
    // The cursor is on the left of the grapheme at this index.
    pub(crate) cursor_byte: usize,
    /// The anchor byte position for an active text selection. The selection
    /// spans from `selection_byte` to `cursor_byte` (in either order). When
    /// `None`, no selection is active.
    pub(crate) selection_byte: Option<usize>,
    pub(crate) undo_redo: SnapshotManager,
}

impl TextBuffer {
    pub fn new(starting_str: &str) -> Self {
        TextBuffer {
            buf: starting_str.to_string(),
            cursor_byte: starting_str.len(),
            selection_byte: None,
            undo_redo: SnapshotManager::new(),
        }
    }

    pub fn new_with_cursor(starting_str: &str) -> Self {
        let cursor_byte_pos = starting_str.find('█').expect("Cursor marker █ not found");
        let input_without_cursor = starting_str.replace('█', "");

        TextBuffer {
            buf: input_without_cursor,
            cursor_byte: cursor_byte_pos,
            selection_byte: None,
            undo_redo: SnapshotManager::new(),
        }
    }
}

#[cfg(test)]
mod test_misc {
    use super::*;

    #[test]
    fn text_buffer_creation() {
        let tb = TextBuffer::new("test");
        assert_eq!(tb.buffer(), "test");
        assert_eq!(tb.cursor_byte, 4);

        let tb2 = TextBuffer::new_with_cursor("te█st");
        assert_eq!(tb2.buffer(), "test");
        assert_eq!(tb2.cursor_byte, 2);
    }
}
