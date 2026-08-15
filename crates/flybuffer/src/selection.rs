use crate::{TextBuffer, WordDelim};

impl TextBuffer {
    /// Anchor a new selection at the current cursor position if one is not
    /// already active. Call this before performing a movement that should
    /// extend the selection.
    pub fn start_selection_if_none(&mut self) {
        if self.selection_byte.is_none() {
            self.selection_byte = Some(self.cursor_byte);
        }
    }

    /// Clear any active selection.
    pub fn clear_selection(&mut self) {
        self.selection_byte = None;
    }

    /// Returns the current selection anchor byte position, or `None` if no
    /// selection is active.
    pub fn selection_byte(&self) -> Option<usize> {
        self.selection_byte
    }

    /// Set the selection anchor byte position (bounded to UTF-8 character boundary).
    pub fn set_selection_anchor(&mut self, pos: usize) {
        let clamped = pos.min(self.buf.len());
        let mut valid_pos = clamped;
        while valid_pos > 0 && !self.buf.is_char_boundary(valid_pos) {
            valid_pos -= 1;
        }
        self.selection_byte = Some(valid_pos);
    }

    /// Returns the byte range of the current selection, sorted so that
    /// `start <= end`. Returns `None` when no selection is active or when the
    /// selection is empty (anchor equal to cursor).
    pub fn selection_range(&self) -> Option<std::ops::Range<usize>> {
        let anchor = self.selection_byte?;
        if anchor == self.cursor_byte {
            return None;
        }
        let start = anchor.min(self.cursor_byte);
        let end = anchor.max(self.cursor_byte);
        if end > self.buf.len()
            || !self.buf.is_char_boundary(start)
            || !self.buf.is_char_boundary(end)
        {
            return None;
        }
        Some(start..end)
    }

    pub fn set_selection_range(&mut self, range: std::ops::Range<usize>, cursor_is_left: bool) {
        if cursor_is_left {
            self.selection_byte = Some(range.end);
            self.cursor_byte = range.start;
        } else {
            self.selection_byte = Some(range.start);
            self.cursor_byte = range.end;
        }
    }

    /// Returns the currently selected text, or `None` if no selection is
    /// active or it is empty.
    pub fn selected_text(&self) -> Option<String> {
        self.selection_range().map(|r| self.buf[r].to_string())
    }

    /// If a non-empty selection is active, delete the selected text, move the
    /// cursor to the start of the selection, and clear the selection. Returns
    /// `true` if a deletion was performed. A snapshot is pushed so the
    /// deletion can be undone.
    pub fn delete_selection(&mut self) -> bool {
        let Some(range) = self.selection_range() else {
            self.selection_byte = None;
            return false;
        };
        self.push_snapshot(false);
        self.buf.drain(range.clone());
        self.cursor_byte = range.start;
        self.selection_byte = None;
        true
    }

    /// Surround the current selection with `open` inserted before it and
    /// `close` inserted after it.  The cursor is placed immediately after
    /// `close` and the selection is cleared.  Returns `true` when the surround
    /// was performed (a non-empty selection was active), `false` otherwise.
    /// A snapshot is pushed so the operation can be undone.
    pub fn surround_selection(&mut self, open: char, close: char) -> bool {
        let Some(range) = self.selection_range() else {
            return false;
        };
        self.push_snapshot(false);
        // Insert the closing char first so that `range.start` stays valid.
        self.buf.insert(range.end, close);
        self.buf.insert(range.start, open);
        self.cursor_byte = range.end + open.len_utf8();
        self.selection_byte = Some(range.start + open.len_utf8());
        true
    }

    pub fn select_entire_buffer(&mut self) {
        self.cursor_byte = self.buf.len();
        self.selection_byte = Some(0);
    }

    pub fn select_word_using_mouse(&mut self) -> std::ops::Range<usize> {
        let start = self.move_one_word_left_pos(WordDelim::FineGrained);
        let end = self.move_one_word_right_pos(WordDelim::FineGrained);
        self.selection_byte = Some(start);
        self.cursor_byte = end;
        self.selection_range().unwrap_or(start..end)
    }

    pub fn select_line_using_mouse(&mut self) -> std::ops::Range<usize> {
        let line_start = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte)
            .find_map(|(i, c)| if c == '\n' { Some(i + 1) } else { None })
            .unwrap_or(0);

        let line_end = self
            .buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_byte)
            .find_map(|(i, c)| if c == '\n' { Some(i) } else { None })
            .unwrap_or(self.buf.len());

        self.selection_byte = Some(line_start);
        self.cursor_byte = line_end;
        self.selection_range().unwrap_or(line_start..line_end)
    }
}

#[cfg(test)]
mod test_selection {
    use super::*;

    #[test]
    fn test_select_line_using_mouse() {
        let mut tb = TextBuffer::new("first line\nsecond line\nthird line");
        tb.try_move_cursor_to_byte_pos(15, false); // in "second line"
        let range = tb.select_line_using_mouse();
        assert_eq!(range, 11..22);
        assert_eq!(tb.selected_text().unwrap(), "second line");
    }

    #[test]
    fn test_select_empty_line_using_mouse() {
        let mut tb = TextBuffer::new("first line\n\nthird line");
        tb.try_move_cursor_to_byte_pos(11, false); // on empty middle line
        let range = tb.select_line_using_mouse();
        assert_eq!(range, 11..11);
    }

    #[test]
    fn no_selection_by_default() {
        let tb = TextBuffer::new("hello");
        assert!(tb.selection_byte().is_none());
        assert!(tb.selection_range().is_none());
        assert!(tb.selected_text().is_none());
    }

    #[test]
    fn start_selection_anchors_at_cursor() {
        let mut tb = TextBuffer::new("hello");
        tb.move_to_start();
        tb.start_selection_if_none();
        assert_eq!(tb.selection_byte(), Some(0));
        // Empty selection — anchor equals cursor — yields no range.
        assert!(tb.selection_range().is_none());
        tb.move_right_selection();
        tb.move_right_selection();
        assert_eq!(tb.selection_range(), Some(0..2));
        assert_eq!(tb.selected_text().as_deref(), Some("he"));
    }

    #[test]
    fn start_selection_is_idempotent() {
        let mut tb = TextBuffer::new("hello");
        tb.move_to_start();
        tb.start_selection_if_none();
        tb.move_right_selection();
        tb.start_selection_if_none(); // should not move the anchor
        assert_eq!(tb.selection_byte(), Some(0));
        assert_eq!(tb.selection_range(), Some(0..1));
    }

    #[test]
    fn selection_range_is_normalised_when_cursor_left_of_anchor() {
        let mut tb = TextBuffer::new("hello");
        // Cursor is at end (5).
        tb.move_left_selection();
        tb.move_left_selection();
        assert_eq!(tb.selection_byte(), Some(5));
        assert_eq!(tb.selection_range(), Some(3..5));
        assert_eq!(tb.selected_text().as_deref(), Some("lo"));
    }

    #[test]
    fn clear_selection_removes_anchor() {
        let mut tb = TextBuffer::new("hello");
        tb.move_to_start();
        tb.move_right_selection();
        assert!(tb.selection_range().is_some());
        tb.clear_selection();
        assert!(tb.selection_byte().is_none());
        assert!(tb.selection_range().is_none());
    }

    #[test]
    fn delete_selection_removes_selected_text() {
        let mut tb = TextBuffer::new("hello world");
        tb.move_to_start();
        tb.move_right_selection();
        tb.move_right_selection();
        tb.move_right_selection();
        tb.move_right_selection();
        tb.move_right_selection();
        assert_eq!(tb.selected_text().as_deref(), Some("hello"));
        assert!(tb.delete_selection());
        assert_eq!(tb.buffer(), " world");
        assert_eq!(tb.cursor_byte, 0);
        assert!(tb.selection_byte().is_none());
    }

    #[test]
    fn delete_selection_with_cursor_left_of_anchor() {
        let mut tb = TextBuffer::new("hello");
        // Cursor at end (5), select backwards.
        tb.move_left_selection();
        tb.move_left_selection();
        assert_eq!(tb.selection_range(), Some(3..5));
        assert!(tb.delete_selection());
        assert_eq!(tb.buffer(), "hel");
        assert_eq!(tb.cursor_byte, 3);
    }

    #[test]
    fn delete_selection_with_no_selection_is_noop() {
        let mut tb = TextBuffer::new("hello");
        assert!(!tb.delete_selection());
        assert_eq!(tb.buffer(), "hello");
    }

    #[test]
    fn delete_selection_can_be_undone() {
        let mut tb = TextBuffer::new("hello");
        tb.move_to_start();
        tb.move_right_selection();
        tb.move_right_selection();
        assert!(tb.delete_selection());
        assert_eq!(tb.buffer(), "llo");
        tb.undo();
        assert_eq!(tb.buffer(), "hello");
    }

    #[test]
    fn surround_selection_wraps_text() {
        let mut tb = TextBuffer::new("hello world");
        tb.move_to_start();
        tb.move_right_selection();
        tb.move_right_selection();
        tb.move_right_selection();
        tb.move_right_selection();
        tb.move_right_selection();
        assert_eq!(tb.selected_text().as_deref(), Some("hello"));
        assert!(tb.surround_selection('(', ')'));
        assert_eq!(tb.buffer(), "(hello) world");
        assert_eq!(tb.cursor_byte, 6); // after ')'
        assert_eq!(tb.selection_byte(), Some(1));
    }

    #[test]
    fn surround_selection_with_cursor_left_of_anchor() {
        let mut tb = TextBuffer::new("hello");
        // Cursor at end, select backwards.
        tb.move_left_selection();
        tb.move_left_selection();
        assert_eq!(tb.selection_range(), Some(3..5));
        assert!(tb.surround_selection('"', '"'));
        assert_eq!(tb.buffer(), "hel\"lo\"");
        assert_eq!(tb.cursor_byte, 6);
        assert_eq!(tb.selection_byte(), Some(4));
    }

    #[test]
    fn surround_selection_with_no_selection_is_noop() {
        let mut tb = TextBuffer::new("hello");
        assert!(!tb.surround_selection('(', ')'));
        assert_eq!(tb.buffer(), "hello");
    }

    #[test]
    fn surround_selection_can_be_undone() {
        let mut tb = TextBuffer::new("hello");
        tb.move_to_start();
        tb.move_right_selection();
        tb.move_right_selection();
        assert!(tb.surround_selection('[', ']'));
        assert_eq!(tb.buffer(), "[he]llo");
        tb.undo();
        assert_eq!(tb.buffer(), "hello");
    }
}
