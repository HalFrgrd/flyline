use std::fmt::Debug;

use unicode_segmentation::UnicodeSegmentation;
// use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use itertools::Itertools;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Eq, PartialEq)]
struct Snapshot {
    buf: String,
    // Cursor byte represents the next insertion position
    // It should always be on a grapheme boundary, but I don't enforce that here. I just need to make sure to update it correctly whenever I change the buffer.
    // It might be greater than the length of the buffer if the cursor is at the end, but it should never be greater than that.
    cursor_byte: usize,
    // Anchor byte for an active selection at the time the snapshot was taken,
    // or `None` if no selection was active. Saved alongside the buffer so that
    // an operation that consumed a selection (e.g. delete-selection) can have
    // its selection restored on undo.
    selection_byte: Option<usize>,
}

impl Snapshot {
    pub fn new(buf: &str, cursor_byte: usize, selection_byte: Option<usize>) -> Self {
        Snapshot {
            buf: buf.to_string(),
            cursor_byte,
            selection_byte,
        }
    }
}

impl Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Snap({:?})", self.buf)
    }
}

#[derive(Debug)]
struct SnapshotManager {
    undos: Vec<Snapshot>,
    redos: Vec<Snapshot>,
    last_snapshot_time: std::time::Instant,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WordDelim {
    WhiteSpace,
    FineGrained,
}

impl WordDelim {
    fn is_word_boundary(&self, c: char) -> bool {
        match self {
            WordDelim::WhiteSpace => c.is_whitespace(),
            WordDelim::FineGrained => c.is_whitespace() || c.is_ascii_punctuation(),
        }
    }
}

pub struct TextBuffer {
    buf: String,
    // Byte index of the cursor position in the buffer
    // Need to ensure it lines up with grapheme boundaries.
    // The cursor is on the left of the grapheme at this index.
    cursor_byte: usize,
    /// The anchor byte position for an active text selection. The selection
    /// spans from `selection_byte` to `cursor_byte` (in either order). When
    /// `None`, no selection is active.
    selection_byte: Option<usize>,
    undo_redo: SnapshotManager,
}

///////////////////////////////////////////////////////// misc
impl TextBuffer {
    pub fn new(starting_str: &str) -> Self {
        TextBuffer {
            buf: starting_str.to_string(),
            cursor_byte: starting_str.len(),
            selection_byte: None,
            undo_redo: SnapshotManager::new(),
        }
    }
}

///////////////////////////////////////////////////////// text selection
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

    pub fn select_word(&mut self) -> std::ops::Range<usize> {
        self.selection_byte = Some(self.move_one_word_left_pos(WordDelim::WhiteSpace));
        self.cursor_byte = self.move_one_word_right_pos(WordDelim::WhiteSpace);
        self.selection_range().unwrap() // should always be Some since we just set the anchor and moved the cursor
    }
}

#[cfg(test)]
mod test_selection {
    use super::*;

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

#[cfg(test)]
mod test_misc {
    use super::*;

    #[test]
    fn text_buffer_creation() {
        let tb = TextBuffer::new("abc");
        assert_eq!(tb.buffer(), "abc");
        assert_eq!(tb.cursor_byte, 3);
    }
}

///////////////////////////////////////////////////////// movement
impl TextBuffer {
    pub fn move_left(&mut self) {
        if let Some(selection_range) = self.selection_range() {
            // When moving left with an active selection, move to the start of the selection and clear it.
            self.cursor_byte = selection_range.start;
            self.clear_selection();
            return;
        }
        self.clear_selection();

        self.cursor_byte = self.left_move_pos();
    }

    fn left_move_pos(&self) -> usize {
        // the previous grapheme boundary before the cursor
        self.buf
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < self.cursor_byte)
            .last()
            .map_or(0, |(i, _)| i)
    }

    pub fn move_left_selection(&mut self) {
        self.start_selection_if_none();
        self.cursor_byte = self.left_move_pos();
    }

    pub fn move_right(&mut self) {
        if let Some(selection_range) = self.selection_range() {
            self.cursor_byte = selection_range.end;
            self.clear_selection();
            return;
        }
        self.clear_selection();
        self.cursor_byte = self.right_move_pos();
    }

    fn right_move_pos(&self) -> usize {
        // the next grapheme boundary after the cursor
        self.buf
            .grapheme_indices(true)
            .skip_while(|(i, _)| *i <= self.cursor_byte)
            .next()
            .map_or(self.buf.len(), |(i, _)| i)
    }

    pub fn move_right_selection(&mut self) {
        self.start_selection_if_none();
        self.cursor_byte = self.right_move_pos();
    }

    fn move_one_word_left_pos(&self, delim: WordDelim) -> usize {
        if let Some("\n\n") = self
            .buf
            .get(self.cursor_byte.saturating_sub(2)..self.cursor_byte)
        {
            return self.cursor_byte - 1;
        }
        self.buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte)
            .skip_while(|(_, c)| delim.is_word_boundary(*c))
            .tuple_windows()
            .find_map(|((i, c), (_, next_c))| {
                if !delim.is_word_boundary(c) && delim.is_word_boundary(next_c) {
                    Some(i)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    pub fn move_one_word_left(&mut self, delim: WordDelim) {
        self.cursor_byte = self.move_one_word_left_pos(delim);
    }

    fn move_one_word_right_pos(&self, delim: WordDelim) -> usize {
        if let Some("\n\n") = self.buf.get(self.cursor_byte..self.cursor_byte + 2) {
            return self.cursor_byte + 1;
        }

        self.buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_byte)
            .skip_while(|(_, c)| delim.is_word_boundary(*c))
            .skip_while(|(_, c)| !delim.is_word_boundary(*c))
            .next()
            .map_or(self.buf.len(), |(i, _)| i)
    }

    pub fn move_one_word_right(&mut self, delim: WordDelim) {
        self.cursor_byte = self.move_one_word_right_pos(delim);
    }

    /// Extend the selection one whitespace-delimited word to the right with
    /// "smart" anchor adjustment: when the selection anchor sits in the middle
    /// of a word (i.e. the characters immediately on either side of the anchor
    /// are both non-whitespace) and the cursor is to the right of the anchor,
    /// move the anchor leftward to the start of that word instead of moving
    /// the cursor further right. This makes a sequence of Ctrl+Shift+Right
    /// presses from the middle of a word naturally select first the right
    /// half of the word, then the entire word, then continue extending word
    /// by word — without maintaining any extra state.
    pub fn move_right_one_word_whitespace_extend_selection(&mut self) {
        if let Some(anchor) = self.selection_byte
            && self.cursor_byte > anchor
            && Self::is_inside_word(&self.buf, anchor)
        {
            // Extend the anchor leftward to the start of the word it sits in.
            let mut new_anchor = anchor;
            for (i, c) in self.buf[..anchor].char_indices().rev() {
                if c.is_whitespace() {
                    break;
                }
                new_anchor = i;
            }
            self.selection_byte = Some(new_anchor);
        } else {
            self.start_selection_if_none();
            self.move_one_word_right(WordDelim::WhiteSpace);
        }
    }

    /// Extend the selection one whitespace-delimited word to the left with
    /// "smart" anchor adjustment: when the selection anchor sits in the middle
    /// of a word (i.e. the characters immediately on either side of the anchor
    /// are both non-whitespace) and the cursor is to the left of the anchor,
    /// move the anchor rightward to the end of that word instead of moving
    /// the cursor further left. This makes a sequence of Ctrl+Shift+Left
    /// presses from the middle of a word naturally select first the left
    /// half of the word, then the entire word, then continue extending word
    /// by word — without maintaining any extra state.
    pub fn move_left_one_word_whitespace_extend_selection(&mut self) {
        if let Some(anchor) = self.selection_byte
            && self.cursor_byte < anchor
            && Self::is_inside_word(&self.buf, anchor)
        {
            // Extend the anchor rightward to the end of the word it sits in.
            let new_anchor = self.buf[anchor..]
                .char_indices()
                .find(|(_, c)| c.is_whitespace())
                .map_or(self.buf.len(), |(i, _)| anchor + i);
            self.selection_byte = Some(new_anchor);
        } else {
            self.start_selection_if_none();
            self.move_one_word_left(WordDelim::WhiteSpace);
        }
    }

    /// Returns `true` when `pos` is strictly inside a word — that is, both the
    /// character immediately before `pos` and the character at `pos` exist and
    /// are non-whitespace.
    fn is_inside_word(buf: &str, pos: usize) -> bool {
        let prev_is_word = buf[..pos]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_whitespace());
        let next_is_word = buf[pos..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace());
        prev_is_word && next_is_word
    }

    pub fn move_one_word_left_fine_grained(&mut self) {
        self.cursor_byte = self.fine_grained_word_left_pos();
    }

    pub fn move_one_word_right_fine_grained(&mut self) {
        self.cursor_byte = self.fine_grained_word_right_pos();
    }

    pub fn move_to_start(&mut self) {
        self.cursor_byte = 0;
    }

    #[allow(dead_code)]
    pub fn move_to_end(&mut self) {
        self.cursor_byte = self.buf.len();
    }

    pub fn move_end_of_line(&mut self) {
        self.cursor_byte = self
            .buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_byte)
            .find_map(|(i, c)| if c == '\n' { Some(i) } else { None })
            .unwrap_or(self.buf.len());
    }

    pub fn move_start_of_line(&mut self) {
        self.cursor_byte = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte)
            .find_map(|(i, c)| if c == '\n' { Some(i + 1) } else { None })
            .unwrap_or(0);
    }

    pub fn move_line_up(&mut self) {
        let (row, col) = self.cursor_2d_position();
        let target_row = row.max(1) - 1;

        self.move_to_cursor_pos(target_row, col);
    }

    pub fn move_line_down(&mut self) {
        let (row, col) = self.cursor_2d_position();
        let target_row = row + 1;

        self.move_to_cursor_pos(target_row, col);
    }

    fn move_to_cursor_pos(&mut self, target_row: usize, target_col: usize) {
        // Not a great implementation, but it works well for small buffers
        // tries to first go to target_row
        // then tries to get close to target_col
        let mut cur_row = 0;
        let mut cur_col = 0;
        // self.debug_buffer();
        for (i, grapheme) in self.buf.grapheme_indices(true) {
            self.cursor_byte = i;
            if cur_row == target_row && cur_col >= target_col {
                return;
            }
            if grapheme.contains('\n') {
                if cur_row == target_row {
                    return;
                }
                cur_row += 1;
                cur_col = 0;
            } else {
                cur_col += grapheme.width();
            }
        }
        self.cursor_byte = self.buf.len();
    }

    pub fn try_move_cursor_to_byte_pos(&mut self, byte_pos: usize, move_past_final_cell: bool) {
        if byte_pos >= self.buf.len().saturating_sub(1) && move_past_final_cell {
            self.cursor_byte = self.buf.len();
            return;
        }

        if byte_pos <= self.buf.len() {
            // Round up to next char boundary if not already on one
            let mut pos = byte_pos;
            while pos <= self.buf.len() && !self.buf.is_char_boundary(pos) {
                pos += 1;
            }
            self.cursor_byte = pos;
        }
    }
}

#[cfg(test)]
mod test_movement {
    use super::*;

    #[test]
    fn move_cursor_left() {
        let mut tb = TextBuffer::new("test 👩‍💻");
        assert_eq!(tb.cursor_byte, 16);
        tb.move_left();
        assert_eq!(tb.cursor_byte, 5);
        tb.move_left();
        tb.move_left();
        tb.move_left();
        tb.move_left();
        assert_eq!(tb.cursor_byte, 1);
        tb.move_left();
        assert_eq!(tb.cursor_byte, 0);
        tb.move_left();
        assert_eq!(tb.cursor_byte, 0);
    }

    #[test]
    fn move_cursor_right() {
        let mut tb = TextBuffer::new("test 👩‍💻");
        tb.move_left();
        tb.move_left();
        tb.move_left();
        assert_eq!(tb.cursor_byte, 3);
        tb.move_right();
        assert_eq!(tb.cursor_byte, 4);
        tb.move_right();
        assert_eq!(tb.cursor_byte, 5);
        tb.move_right();
        assert_eq!(tb.cursor_byte, 16);
        tb.move_right();
        assert_eq!(tb.cursor_byte, 16);
    }

    #[test]
    fn move_one_word_left() {
        let mut tb = TextBuffer::new("abc    def   asdfasdf");
        tb.move_end_of_line();
        tb.move_left();
        tb.move_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.cursor_byte, "abc    def   ".len());
        tb.move_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.cursor_byte, "abc    ".len());
        tb.move_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.cursor_byte, "".len());
    }

    #[test]
    fn move_one_word_right() {
        let mut tb = TextBuffer::new("  abc def");
        tb.move_to_start();
        tb.move_one_word_right(WordDelim::WhiteSpace);
        assert_eq!(tb.cursor_byte, "  abc".len());
        tb.move_one_word_right(WordDelim::WhiteSpace);
        assert_eq!(tb.cursor_byte, "  abc def".len());
    }

    #[test]
    fn move_right_one_word_extend_selection_smart_from_middle_of_word() {
        // Cursor in the middle of "abc": first press selects "bc", second press
        // grows the selection backward to include the whole word "abc",
        // subsequent presses continue extending word by word to the right.
        let mut tb = TextBuffer::new("abc def ghi");
        tb.cursor_byte = 1; // between 'a' and 'b'

        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(1));
        assert_eq!(tb.cursor_byte, 3);
        assert_eq!(tb.selected_text().as_deref(), Some("bc"));

        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(0));
        assert_eq!(tb.cursor_byte, 3);
        assert_eq!(tb.selected_text().as_deref(), Some("abc"));

        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(0));
        assert_eq!(tb.cursor_byte, "abc def".len());
        assert_eq!(tb.selected_text().as_deref(), Some("abc def"));

        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(0));
        assert_eq!(tb.cursor_byte, "abc def ghi".len());
        assert_eq!(tb.selected_text().as_deref(), Some("abc def ghi"));
    }

    #[test]
    fn move_right_one_word_extend_selection_from_start_of_word() {
        // Cursor at the start of "abc" (anchor would not be inside a word) —
        // behaves as a plain word-extending selection.
        let mut tb = TextBuffer::new("abc def");
        tb.move_to_start();
        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(0));
        assert_eq!(tb.cursor_byte, 3);
        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(0));
        assert_eq!(tb.cursor_byte, "abc def".len());
    }

    #[test]
    fn move_right_one_word_extend_selection_anchor_at_end_of_word() {
        // Anchor immediately after a word ('c' before, ' ' after) is not
        // "inside a word", so the cursor advances normally.
        let mut tb = TextBuffer::new("abc def");
        tb.cursor_byte = 3; // right after 'c'
        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(3));
        assert_eq!(tb.cursor_byte, "abc def".len());
        tb.move_right_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some(3));
        assert_eq!(tb.cursor_byte, "abc def".len());
    }

    #[test]
    fn move_left_one_word_extend_selection_smart_from_middle_of_word() {
        // Cursor in the middle of "ghi": first press selects "gh", second press
        // grows the selection forward to include the whole word "ghi",
        // subsequent presses continue extending word by word to the left.
        let mut tb = TextBuffer::new("abc def ghi");
        tb.cursor_byte = "abc def gh".len(); // between 'h' and 'i'

        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc def gh".len()));
        assert_eq!(tb.cursor_byte, "abc def ".len());
        assert_eq!(tb.selected_text().as_deref(), Some("gh"));

        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc def ghi".len()));
        assert_eq!(tb.cursor_byte, "abc def ".len());
        assert_eq!(tb.selected_text().as_deref(), Some("ghi"));

        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc def ghi".len()));
        assert_eq!(tb.cursor_byte, "abc ".len());
        assert_eq!(tb.selected_text().as_deref(), Some("def ghi"));

        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc def ghi".len()));
        assert_eq!(tb.cursor_byte, 0);
        assert_eq!(tb.selected_text().as_deref(), Some("abc def ghi"));
    }

    #[test]
    fn move_left_one_word_extend_selection_from_end_of_word() {
        // Cursor at the end of "ghi" (anchor would not be inside a word) —
        // behaves as a plain word-extending selection.
        let mut tb = TextBuffer::new("abc def");
        tb.move_end_of_line();
        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc def".len()));
        assert_eq!(tb.cursor_byte, "abc ".len());
        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc def".len()));
        assert_eq!(tb.cursor_byte, 0);
    }

    #[test]
    fn move_left_one_word_extend_selection_anchor_at_start_of_word() {
        // Anchor immediately before a word (' ' before, 'd' after) is not
        // "inside a word", so the cursor moves normally.
        let mut tb = TextBuffer::new("abc def");
        tb.cursor_byte = "abc ".len(); // right before 'd'
        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc ".len()));
        assert_eq!(tb.cursor_byte, 0);
        tb.move_left_one_word_whitespace_extend_selection();
        assert_eq!(tb.selection_byte(), Some("abc ".len()));
        assert_eq!(tb.cursor_byte, 0);
    }

    #[test]
    fn move_one_word_left_fine_grained_basic() {
        // Stops at punctuation boundaries (no slashes → full punctuation mode).
        let mut tb = TextBuffer::new("abc::def::ghi");
        tb.move_end_of_line();
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "abc::def::".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "abc::def".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "abc::".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "abc".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, 0);
    }

    #[test]
    fn move_one_word_right_fine_grained_basic() {
        // Stops at punctuation boundaries (no slashes → full punctuation mode).
        let mut tb = TextBuffer::new("abc::def::ghi");
        tb.move_to_start();
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "abc".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "abc::".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "abc::def".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "abc::def::".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "abc::def::ghi".len());
    }

    #[test]
    fn move_one_word_left_fine_grained_path() {
        // When the word contains slashes, only '/' and '\' are boundaries.
        let mut tb = TextBuffer::new("echo ./foo_bar/baz.jeb");
        tb.move_end_of_line();
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ./foo_bar/".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ./foo_bar".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ./".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "echo .".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ".len());
        // "echo" now has a slash to the right, so slash-only mode keeps it whole.
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, "echo".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, 0);
    }

    #[test]
    fn move_one_word_right_fine_grained_path() {
        // When the word contains slashes, only '/' and '\' are boundaries.
        let mut tb = TextBuffer::new("echo ./foo_bar/baz.jeb");
        tb.move_to_start();
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "echo".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ".len());
        // "./" contains a slash → slash-only mode; '.' and 'f' have different classes
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "echo .".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ./".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ./foo_bar".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ./foo_bar/".len());
        // Slash still present to the left → slash-only mode; whole "baz.jeb" as one segment.
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "echo ./foo_bar/baz.jeb".len());
    }

    #[test]
    fn move_one_word_fine_grained_edge_cases() {
        // Empty buffer: both directions stay at 0.
        let mut tb = TextBuffer::new("");
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, 0);
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, 0);

        // Whitespace-only: left from end stops at start; right from start goes to end.
        let mut tb = TextBuffer::new("   ");
        tb.move_end_of_line();
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, 0);
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "   ".len());

        // Starts/ends with punctuation.
        let mut tb = TextBuffer::new("::abc::");
        tb.move_to_start();
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "::".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "::abc".len());
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "::abc::".len());

        let mut tb2 = TextBuffer::new("::abc::");
        tb2.move_end_of_line();
        tb2.move_one_word_left_fine_grained();
        assert_eq!(tb2.cursor_byte, "::abc".len());
        tb2.move_one_word_left_fine_grained();
        assert_eq!(tb2.cursor_byte, "::".len());
        tb2.move_one_word_left_fine_grained();
        assert_eq!(tb2.cursor_byte, 0);
    }

    #[test]
    fn move_line_up() {
        let mut tb = TextBuffer::new("Line 1\nLine 2\nLine 3");
        tb.move_end_of_line();
        tb.move_line_up();
        assert_eq!(tb.cursor_byte, "Line 1\nLine 2".len());
        tb.move_line_up();
        assert_eq!(tb.cursor_byte, "Line 1".len());
    }

    #[test]
    fn move_line_down() {
        let mut tb = TextBuffer::new("Line 1\nLine 2\nLine 3");
        tb.move_to_start();
        tb.move_line_down();
        assert_eq!(tb.cursor_2d_position(), (1, 0));
        tb.move_right();
        tb.move_right();
        tb.move_right();
        tb.move_right();
        assert_eq!(tb.cursor_byte, "Line 1\nLine".len());
        tb.move_line_down();
        assert_eq!(tb.cursor_byte, "Line 1\nLine 2\nLine".len());
    }

    #[test]
    fn move_line_to_down_onto_empty_final_line() {
        let mut tb = TextBuffer::new("Line 1\nLine 2\n");
        tb.move_to_start();
        tb.move_line_down();
        assert_eq!(tb.cursor_2d_position(), (1, 0));
        tb.move_line_down();
        assert_eq!(tb.cursor_2d_position(), (2, 0));
        assert_eq!(tb.cursor_byte, "Line 1\nLine 2\n".len());
    }
}
///////////////////////////////////////////////////////// editing primitives without snapshots
impl TextBuffer {
    fn insert_char_no_snapshot(&mut self, c: char) {
        self.buf.insert(self.cursor_byte, c);
        self.cursor_byte += c.len_utf8();
    }

    fn insert_str_no_snapshot(&mut self, s: &str) {
        let sanitized_str = s.replace("\r\n", "\n").replace('\r', "\n"); // remove carriage returns, which can mess up the display
        self.buf.insert_str(self.cursor_byte, &sanitized_str);
        self.cursor_byte += sanitized_str.len();
    }
}

///////////////////////////////////////////////////////// editing primitives with snapshots
impl TextBuffer {
    pub fn insert_char(&mut self, c: char) {
        self.push_snapshot(true);
        self.insert_char_no_snapshot(c);
    }

    pub fn insert_str(&mut self, s: &str) {
        self.push_snapshot(true);
        self.insert_str_no_snapshot(s);
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }
}

#[cfg(test)]
mod test_editing_primitives {
    use super::*;

    #[test]
    fn zwj_emoji_insertion() {
        let mut tb = TextBuffer::new("test ");
        assert_eq!(tb.cursor_byte, 5);
        tb.insert_char('👩');
        assert_eq!(tb.cursor_byte, 5 + 4);
        tb.insert_char('\u{200d}'); // ZWJ
        assert_eq!(tb.cursor_byte, 5 + 4 + 3);
        tb.insert_char('💻');
        assert_eq!(tb.buffer(), "test 👩‍💻");
        assert_eq!(tb.cursor_byte, 5 + 4 + 3 + 4);
    }

    #[test]
    fn insert_char_emoji_with_modifier() {
        // Emoji with skin tone modifier (should be treated as single grapheme)
        let mut tb = TextBuffer::new("wave ");
        tb.insert_char('👋');
        tb.insert_char('\u{1F3FB}'); // Light skin tone modifier
        assert_eq!(tb.buffer(), "wave 👋🏻");
        assert_eq!(tb.cursor_byte, 13); // Base emoji (4 bytes) + modifier (4 bytes) + "wave " (5 bytes)
    }

    #[test]
    fn insert_char_combining_diacritics() {
        // Character with combining diacritical marks (NFD form)
        let mut tb = TextBuffer::new("caf");
        tb.insert_char('e');
        tb.insert_char('\u{0301}'); // Combining acute accent
        assert_eq!(tb.buffer(), "cafe\u{0301}"); // NFD (decomposed) form
        assert_eq!(tb.cursor_byte, 6); // 'e' (1 byte) + combining accent (2 bytes) + "caf" (3 bytes)
    }

    #[test]
    fn insert_char_regional_indicator() {
        // Regional indicator symbols (flag emojis are pairs of these)
        let mut tb = TextBuffer::new("Flag: ");
        tb.insert_char('🇺'); // Regional indicator U
        tb.insert_char('🇸'); // Regional indicator S
        assert_eq!(tb.buffer(), "Flag: 🇺🇸");
        assert_eq!(tb.cursor_byte, 14); // Each regional indicator is 4 bytes
    }

    #[test]
    fn insert_str_mixed_width_characters() {
        // Mix of ASCII, wide characters (CJK), and emoji
        let mut tb = TextBuffer::new("Start: ");
        tb.insert_str("Hello 世界 🌍");
        assert_eq!(tb.buffer(), "Start: Hello 世界 🌍");
        // "Start: " = 7, "Hello " = 6, "世界" = 6, " " = 1, "🌍" = 4 = 24 bytes total
        assert_eq!(tb.cursor_byte, 24);
    }

    #[test]
    fn insert_str_family_emoji_sequence() {
        // Family emoji is a ZWJ sequence of multiple emojis
        let mut tb = TextBuffer::new("Family: ");
        tb.insert_str("👨‍👩‍👧‍👦"); // Man, woman, girl, boy with ZWJ
        assert_eq!(tb.buffer(), "Family: 👨‍👩‍👧‍👦");
        // This is: 👨 (4) + ZWJ (3) + 👩 (4) + ZWJ (3) + 👧 (4) + ZWJ (3) + 👦 (4) = 25 bytes
        assert_eq!(tb.cursor_byte, 33); // "Family: " (8) + emoji sequence (25)
    }

    #[test]
    fn insert_str_right_to_left_text() {
        // Arabic and Hebrew text (right-to-left scripts)
        let mut tb = TextBuffer::new("Text: ");
        tb.insert_str("مرحبا שלום"); // Arabic "hello" + space + Hebrew "hello"
        assert_eq!(tb.buffer(), "Text: مرحبا שלום");
        // "Text: " = 6, "مرحبا" = 10 bytes, " " = 1, "שלום" = 8 bytes
        assert_eq!(tb.cursor_byte, 25);
    }

    #[test]
    fn insert_str_zero_width_joiner_sequences() {
        // Multiple ZWJ sequences in one string
        let mut tb = TextBuffer::new("");
        tb.insert_str("👨‍💻 and 👩‍🔬"); // Programmer and scientist
        assert_eq!(tb.buffer(), "👨‍💻 and 👩‍🔬");
        // 👨‍💻 = 11 bytes, " and " = 5 bytes, 👩‍🔬 = 11 bytes
        assert_eq!(tb.cursor_byte, 27);
    }
}

///////////////////////////////////////////////////////// editing advanced
impl TextBuffer {
    fn less_strict_class(c: char) -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_ascii_punctuation() {
            1
        } else {
            2
        }
    }

    fn less_strict_class_slash_only(c: char) -> u8 {
        if c.is_whitespace() {
            0
        } else if c == '/' || c == '\\' {
            1
        } else {
            2
        }
    }

    fn has_slash_in_word(buf: &str, cursor_byte: usize) -> bool {
        let left = buf[..cursor_byte]
            .chars()
            .rev()
            .take_while(|c| !c.is_whitespace())
            .any(|c| c == '/' || c == '\\');
        let right = buf[cursor_byte..]
            .chars()
            .take_while(|c| !c.is_whitespace())
            .any(|c| c == '/' || c == '\\');
        left || right
    }

    pub fn delete_left(&mut self) {
        // delete one grapheme to the left
        self.push_snapshot(true);
        let old_cursor_col = self.cursor_byte;
        self.move_left();
        assert!(self.cursor_byte <= old_cursor_col);
        self.buf.drain(self.cursor_byte..old_cursor_col);
    }

    pub fn delete_right(&mut self) {
        // delete one grapheme to the right
        self.push_snapshot(true);
        let cursor_pos_right = self.right_move_pos();
        assert!(self.cursor_byte <= cursor_pos_right);
        self.buf.drain(self.cursor_byte..cursor_pos_right);
    }

    /// Computes the target cursor byte position when moving/deleting one
    /// fine-grained word to the left (stopping at punctuation or path-segment
    /// boundaries, with slash-only mode when the word under the cursor
    /// contains `/` or `\`).
    fn fine_grained_word_left_pos(&self) -> usize {
        let class_fn: fn(char) -> u8 = if Self::has_slash_in_word(&self.buf, self.cursor_byte) {
            Self::less_strict_class_slash_only
        } else {
            Self::less_strict_class
        };
        let mut iter = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte);
        match iter.next() {
            Some((first_i, first_c)) => {
                let class = class_fn(first_c);
                iter.scan((first_i, first_c), |prev, (i, c)| {
                    let (prev_i, prev_c) = *prev;
                    let boundary = if class_fn(prev_c) == class && class_fn(c) != class {
                        Some(prev_i)
                    } else {
                        None
                    };
                    *prev = (i, c);
                    Some(boundary)
                })
                .find_map(|x| x)
                .unwrap_or(0)
            }
            None => 0,
        }
    }

    /// Computes the target cursor byte position when moving/deleting one
    /// fine-grained word to the right (stopping at punctuation or path-segment
    /// boundaries, with slash-only mode when the word under the cursor
    /// contains `/` or `\`).
    fn fine_grained_word_right_pos(&self) -> usize {
        let end = self.buf.len();
        let class_fn: fn(char) -> u8 = if Self::has_slash_in_word(&self.buf, self.cursor_byte) {
            Self::less_strict_class_slash_only
        } else {
            Self::less_strict_class
        };
        let mut iter = self
            .buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_byte);
        match iter.next() {
            Some((_, first_c)) => {
                let class = class_fn(first_c);
                iter.find_map(|(i, c)| if class_fn(c) != class { Some(i) } else { None })
                    .unwrap_or(end)
            }
            None => end,
        }
    }

    pub fn delete_one_word_left(&mut self, delim: WordDelim) {
        self.push_snapshot(true);
        let old_cursor_col = self.cursor_byte;

        // First, find the position reached by skipping back over any contiguous
        // run of whitespace immediately before the cursor.
        let after_ws_skip = self.buf[..old_cursor_col]
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8());
        let ws_chars = self.buf[after_ws_skip..old_cursor_col].chars().count();

        // If there are 2+ contiguous whitespace chars before the cursor, just
        // delete the whitespace and stop. Otherwise (0 or 1 ws chars), also
        // consume the previous word using the per-delim word-boundary logic.
        let new_cursor = if ws_chars >= 2 {
            after_ws_skip
        } else if delim == WordDelim::WhiteSpace {
            self.move_one_word_left_pos(WordDelim::WhiteSpace)
        } else {
            self.fine_grained_word_left_pos()
        };

        assert!(new_cursor <= old_cursor_col);
        self.cursor_byte = new_cursor;
        self.buf.drain(new_cursor..old_cursor_col);
    }

    pub fn delete_right_one_word(&mut self, delim: WordDelim) {
        self.push_snapshot(true);
        let start_cursor = self.cursor_byte;
        let end = self.buf.len();

        // First, find the position reached by skipping forward over any
        // contiguous run of whitespace immediately after the cursor.
        let after_ws_skip = self.buf[start_cursor..]
            .char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map_or(end, |(i, _)| start_cursor + i);
        let ws_chars = self.buf[start_cursor..after_ws_skip].chars().count();

        // If there are 2+ contiguous whitespace chars after the cursor, just
        // delete the whitespace and stop. Otherwise (0 or 1 ws chars), also
        // consume the next word using the per-delim word-boundary logic.
        let end_cursor = if ws_chars >= 2 {
            after_ws_skip
        } else if delim == WordDelim::WhiteSpace {
            self.buf
                .char_indices()
                .skip_while(|(i, _)| *i <= self.cursor_byte)
                .skip_while(|(_, c)| delim.is_word_boundary(*c))
                .skip_while(|(_, c)| !delim.is_word_boundary(*c))
                .next()
                .map_or(end, |(i, _)| i)
        } else {
            self.fine_grained_word_right_pos()
        };

        assert!(end_cursor >= self.cursor_byte);
        self.buf.drain(self.cursor_byte..end_cursor);
    }

    pub fn replace_word_under_cursor(
        &mut self,
        new_word: &str,
        sub_string: &SubString,
    ) -> anyhow::Result<SubString> {
        let end = sub_string.start + sub_string.s.len();

        match self.buf.get(sub_string.start..end) {
            Some(s) if s == sub_string.s => {
                // Delete the word and position cursor at the start
                self.push_snapshot(false);
                self.buf.drain(sub_string.start..end);
                self.cursor_byte = sub_string.start;
                self.insert_str_no_snapshot(new_word);
                Ok(SubString {
                    s: new_word.to_string(),
                    start: sub_string.start,
                })
            }
            Some(s) => Err(anyhow::anyhow!(
                "Expected word '{}' at position {}, but found '{}'",
                sub_string.s,
                sub_string.start,
                s
            )),
            _ => Err(anyhow::anyhow!(
                "Expected word '{}' at position {}, but the range was out of bounds",
                sub_string.s,
                sub_string.start,
            )),
        }
    }

    pub fn replace_buffer(&mut self, new_buffer: &str) {
        self.push_snapshot(false);
        self.buf = new_buffer.to_string();
        self.cursor_byte = new_buffer.len();
    }

    pub fn delete_until_start_of_line(&mut self) {
        self.push_snapshot(true);
        let old_cursor = self.cursor_byte;
        self.move_start_of_line();
        self.buf.drain(self.cursor_byte..old_cursor);
    }

    pub fn delete_until_end_of_line(&mut self) {
        self.push_snapshot(true);
        let old_cursor = self.cursor_byte;
        self.move_end_of_line();
        self.buf.drain(old_cursor..self.cursor_byte);
        self.cursor_byte = old_cursor;
    }
}

#[cfg(test)]
mod test_editing_advanced {

    use super::*;

    #[test]
    fn delete_back() {
        let mut tb = TextBuffer::new("Hello, World!");
        tb.delete_left();
        assert_eq!(tb.buffer(), "Hello, World");
        tb.delete_left();
        assert_eq!(tb.buffer(), "Hello, Worl");
        tb.delete_left();
        assert_eq!(tb.buffer(), "Hello, Wor");
    }

    fn create_substring(buffer: &str, word: &str) -> SubString {
        let start = buffer.find(word).unwrap();
        SubString {
            s: word.to_string(),
            start,
        }
    }

    #[test]
    fn replace_word_under_cursor_at_start_of_line() {
        // Cursor at position 0 (start of line) with non-ASCII word
        let mut tb = TextBuffer::new("café option 日本語 🎯");
        tb.move_to_start(); // Cursor at position 0, at start of "café"
        tb.replace_word_under_cursor("coffee", &create_substring(&tb.buffer(), "café"))
            .unwrap();
        assert_eq!(tb.buffer(), "coffee option 日本語 🎯");
        assert_eq!(tb.cursor_byte, "coffee".len());
    }

    #[test]
    fn replace_word_under_cursor_in_middle_of_word() {
        // Cursor in the middle of a word with Cyrillic characters
        let mut tb = TextBuffer::new("git файл --message 'привет' 🚀");
        tb.move_to_start();
        for _ in 0..6 {
            tb.move_right();
        } // Position at "git фа|йл" (middle of "файл")
        tb.replace_word_under_cursor("file", &create_substring(&tb.buffer(), "файл"))
            .unwrap();
        assert_eq!(tb.buffer(), "git file --message 'привет' 🚀");
        assert_eq!(tb.cursor_byte, "git file".len());
    }

    #[test]
    fn replace_word_under_cursor_at_end_of_line() {
        // Cursor at the end of line on an emoji word
        let mut tb = TextBuffer::new("hello world 🎉🎊🎈");
        // Cursor is already at the end, on the emoji sequence
        tb.replace_word_under_cursor("celebration", &create_substring(&tb.buffer(), "🎉🎊🎈"))
            .unwrap();
        assert_eq!(tb.buffer(), "hello world celebration");
        assert_eq!(tb.cursor_byte, "hello world celebration".len());
    }

    #[test]
    fn replace_word_under_cursor_accented_at_word_end() {
        // Cursor at the end of a word with heavy accents
        let mut tb = TextBuffer::new("find naïve résumé café 📄");
        tb.move_to_start();
        for _ in 0..10 {
            tb.move_right();
        } // Position at "find naïve| résumé" (end of "naïve")
        tb.replace_word_under_cursor("simple", &create_substring(&tb.buffer(), "naïve"))
            .unwrap();
        assert_eq!(tb.buffer(), "find simple résumé café 📄");
        assert_eq!(tb.cursor_byte, "find simple".len());
    }

    #[test]
    #[should_panic(expected = "range was out of bounds")]
    fn replace_word_under_cursor_out_of_bounds() {
        // Cursor at the end of a word with heavy accents
        let mut tb = TextBuffer::new("find naïve résumé café 📄");
        tb.move_to_start();
        tb.replace_word_under_cursor(
            "test",
            &SubString {
                s: "nonexistent".to_string(),
                start: 100,
            },
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "Expected word 'wrong_word' at position 0, but found 'hello worl'")]
    fn replace_word_under_cursor_wrong_word() {
        // Cursor at the end of a word with heavy accents
        let mut tb = TextBuffer::new("hello world");
        tb.move_to_start();
        tb.replace_word_under_cursor(
            "test",
            &SubString {
                s: "wrong_word".to_string(),
                start: 0,
            },
        )
        .unwrap();
    }

    #[test]
    fn delete_one_word_left() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   ");
        // Two or more contiguous trailing whitespace chars are deleted alone,
        // without consuming the previous word.
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi");
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo test ");
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo ");
    }

    #[test]
    fn delete_one_word_left_trailing_whitespace_cases() {
        // Single trailing whitespace: delete the whitespace AND the previous word.
        let mut tb = TextBuffer::new("foo ");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");

        // Two trailing whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("foo  ");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // Many trailing whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("foo           ");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // No trailing whitespace: delete the word.
        let mut tb = TextBuffer::new("foo");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_left_less_strict() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /etc/");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /etc");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   ");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc");
    }

    #[test]
    fn delete_one_word_right() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), " test abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), " abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "   /etc/asd");
        // Three or more contiguous leading whitespace chars are deleted alone,
        // without consuming the next word.
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "/etc/asd");
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_right_leading_whitespace_cases() {
        // Single leading whitespace: delete the whitespace AND the next word.
        let mut tb = TextBuffer::new(" foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");

        // Two leading whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("  foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // Many leading whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("                foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // No leading whitespace: delete the word.
        let mut tb = TextBuffer::new("foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_right_less_strict() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), " test abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "test abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), " abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_left_less_strict_path() {
        // When the word to the left contains slashes, only / and \ are treated as
        // punctuation boundaries, so filename components with dots are not split.
        let mut tb = TextBuffer::new("echo ./foo_bar/baz.jeb");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ./foo_bar/");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ./foo_bar");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ./");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo .");
        // No more slashes in the remaining word; full punctuation mode resumes.
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ");
    }

    #[test]
    fn delete_one_word_right_less_strict_path() {
        // Symmetric: forward deletion is also slash-aware.
        let mut tb = TextBuffer::new("echo ./foo_bar/baz.jeb");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), " ./foo_bar/baz.jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "./foo_bar/baz.jeb");
        // Now the word starts with "./" which contains a slash; slash-only mode.
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/foo_bar/baz.jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "foo_bar/baz.jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/baz.jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "baz.jeb");
        // No more slashes; full punctuation mode.
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), ".jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_left_slash_aware_from_right() {
        // When cursor is after a dotted path prefix but the slash is only to the
        // right of the cursor, slash-only mode should still apply so the whole
        // filename component is deleted as one unit.
        let mut tb = TextBuffer::new("echo baz.jeb/foo_bar");
        tb.cursor_byte = "echo baz.jeb".len();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo /foo_bar");
    }

    #[test]
    fn delete_one_word_right_slash_aware_from_left() {
        // When cursor is right after the last slash in a path, the slash to the
        // left of the cursor should trigger slash-only mode so the dotted filename
        // component is deleted as one unit rather than being split at the dot.
        let mut tb = TextBuffer::new("echo /foo_bar/baz.jeb");
        tb.cursor_byte = "echo /foo_bar/".len();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo /foo_bar/");
    }

    #[test]
    fn delete_word_backslash_path_bidirectional() {
        // Same behaviour with backslash path separators.
        let mut tb = TextBuffer::new("echo baz.txt\\foo\\bar");
        tb.cursor_byte = "echo baz.txt".len();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo \\foo\\bar");

        let mut tb2 = TextBuffer::new("echo \\foo\\baz.txt");
        tb2.cursor_byte = "echo \\foo\\".len();
        tb2.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb2.buffer(), "echo \\foo\\");
    }

    #[test]
    fn delete_until_end_of_line_multiline() {
        let mut tb = TextBuffer::new("hello\nworld\nfoo");
        tb.cursor_byte = 2; // Cursor after 'he|llo\nworld\nfoo'
        tb.delete_until_end_of_line();
        assert_eq!(tb.buffer(), "he\nworld\nfoo");
        // Move to next line and test again
        tb.cursor_byte = 3; // At start of 'world'
        tb.delete_until_end_of_line();
        assert_eq!(tb.buffer(), "he\n\nfoo");
    }

    #[test]
    fn delete_until_start_of_line_multiline() {
        let mut tb = TextBuffer::new("abc\ndef\nghi");
        tb.cursor_byte = 5;
        tb.delete_until_start_of_line();
        assert_eq!(tb.buffer(), "abc\nef\nghi");
        // Move to next line and test again
        tb.move_to_end();
        tb.delete_until_start_of_line();
        assert_eq!(tb.buffer(), "abc\nef\n");
    }
}

///////////////////////////////////////////////////////// Accessors
impl TextBuffer {
    pub fn buffer(&self) -> &str {
        &self.buf
    }

    pub fn is_cursor_at_start(&self) -> bool {
        self.cursor_byte == 0
    }

    pub fn is_cursor_at_end(&self) -> bool {
        self.cursor_byte == self.buf.len()
    }

    pub fn is_cursor_at_trimmed_end(&self) -> bool {
        self.cursor_byte >= self.buf.trim_end().len()
    }

    pub fn is_cursor_on_final_line(&self) -> bool {
        !self.buf[self.cursor_byte..].contains('\n')
    }

    #[allow(dead_code)]
    pub fn debug_buffer(&self) {
        for (i, char) in self.buf.chars().enumerate() {
            let cursor_marker = if i == self.cursor_byte {
                "<-- cursor"
            } else {
                ""
            };

            let char_display = match char {
                '\n' => "\\n".to_string(),
                '\r' => "\\r".to_string(),
                '\t' => "\\t".to_string(),
                _ => char.to_string(),
            };
            log::debug!("Byte {}: '{}' {}", i, char_display, cursor_marker);
        }

        for (i, grapheme) in self.buf.graphemes(true).enumerate() {
            let cursor_marker = if self.buf[..self.cursor_byte].graphemes(true).count() == i {
                "<-- cursor"
            } else {
                ""
            };
            let grapheme_display = match grapheme {
                "\n" => "\\n".to_string(),
                "\r" => "\\r".to_string(),
                "\t" => "\\t".to_string(),
                _ => grapheme.to_string(),
            };
            log::debug!("Grapheme {}: '{}' {}", i, grapheme_display, cursor_marker);
        }
    }

    pub fn cursor_2d_position(&self) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;
        for (i, grapheme) in self.buf.grapheme_indices(true) {
            if i >= self.cursor_byte {
                break;
            }
            if grapheme.contains('\n') {
                row += 1;
                col = 0;
            } else {
                col += grapheme.width();
            }
        }
        (row, col)
    }

    pub fn cursor_row(&self) -> usize {
        self.cursor_2d_position().0
    }

    // pub fn cursor_col(&self) -> usize {
    //     self.cursor_2d_position().1
    // }

    // pub fn cursor_char_pos(&self) -> usize {
    //     self.buf[..self.cursor_byte].chars().count()
    // }

    pub fn cursor_byte_pos(&self) -> usize {
        self.cursor_byte
    }

    pub fn lines_with_cursor(&self) -> Vec<(&str, Option<u16>)> {
        // additionally return and empty string if the buffer finishes with a newline
        let mut lines = self.buf.lines().collect::<Vec<_>>();
        if self.buf.ends_with('\n') {
            lines.push("");
        }
        if lines.is_empty() {
            lines.push("");
        }
        let (cursor_row, cursor_col) = self.cursor_2d_position();

        lines
            .into_iter()
            .enumerate()
            .map(|(i, line)| {
                if i == cursor_row {
                    (line, Some(cursor_col as u16))
                } else {
                    (line, None)
                }
            })
            .collect()
    }

    // pub fn last_line_is_empty(&self) -> bool {
    //     self.buf.lines().last().map_or(true, |line| line.is_empty())
    // }
}

mod test_accessors {
    // Add accessor-specific tests here if needed
    // Currently most accessor methods are tested implicitly in other modules
}

///////////////////////////////////////////////////////// undo and redo
impl TextBuffer {
    fn create_snapshot(&self) -> Snapshot {
        Snapshot::new(&self.buf, self.cursor_byte, self.selection_byte)
    }

    fn push_snapshot(&mut self, merge_with_recent: bool) {
        let snapshot = self.create_snapshot();

        self.undo_redo.add_snapshot(snapshot, merge_with_recent);
    }

    pub fn undo(&mut self) {
        let current_state = self.create_snapshot();

        if let Some(snapshot) = self.undo_redo.prev_snapshot(current_state) {
            self.buf = snapshot.buf;
            self.cursor_byte = snapshot.cursor_byte;
            self.selection_byte = snapshot.selection_byte;
        }
    }

    pub fn redo(&mut self) {
        let current_state = self.create_snapshot();

        if let Some(snapshot) = self.undo_redo.next_snapshot(current_state) {
            self.buf = snapshot.buf;
            self.cursor_byte = snapshot.cursor_byte;
            self.selection_byte = snapshot.selection_byte;
        }
    }

    #[allow(dead_code)]
    fn debug_undo_stack(&self) -> String {
        format!(
            "Undo stack: {:?}, redo stack: {:?}",
            self.undo_redo.undos,
            self.undo_redo.redos.iter().rev().collect::<Vec<_>>()
        )
    }
}

impl SnapshotManager {
    // Most of the time the edit buffer will be small so Im choosing to push and pop the entire edit buffer
    // as opposed to a more complex diffing approach.
    fn new() -> Self {
        SnapshotManager {
            undos: Vec::new(),
            redos: Vec::new(),
            last_snapshot_time: std::time::Instant::now(),
        }
    }

    fn add_snapshot(&mut self, snapshot: Snapshot, merge_with_recent: bool) {
        if Some(&snapshot) == self.undos.last() {
            return;
        }

        let now = std::time::Instant::now();
        let duration_since_last = now.duration_since(self.last_snapshot_time);

        if merge_with_recent
            && !cfg!(test)
            && duration_since_last < std::time::Duration::from_millis(1000)
            && !self.undos.is_empty()
        {
            // log::debug!("Reusing recent snapshot: age {:?} ", duration_since_last);
        } else {
            self.last_snapshot_time = now;
            self.undos.push(snapshot);
        }

        self.redos.clear(); // clear redo stack on new edit
    }

    fn next_snapshot(&mut self, current_state: Snapshot) -> Option<Snapshot> {
        if self.redos.is_empty() {
            log::debug!("No redos available");
            None
        } else {
            self.undos.push(current_state);
            let snapshot = self.redos.pop().unwrap();

            if &snapshot == self.undos.last().unwrap() {
                self.redos.pop()
            } else {
                // log::debug!("Redoing to snapshot: {:?}", snapshot);
                Some(snapshot)
            }
        }
    }

    fn prev_snapshot(&mut self, current_state: Snapshot) -> Option<Snapshot> {
        if self.undos.is_empty() {
            log::debug!("At oldest snapshot, cannot undo further");
            None
        } else {
            self.redos.push(current_state);
            let snapshot = self.undos.pop().unwrap();

            if &snapshot == self.redos.last().unwrap() {
                self.undos.pop()
            } else {
                // log::debug!("Undoing to snapshot: {:?}", snapshot);
                Some(snapshot)
            }
        }
    }
}

#[cfg(test)]
mod test_undo_redo {
    use super::*;

    use log::{LevelFilter, Log, Metadata, Record};

    fn setup_logging() {
        struct StdoutLogger;
        impl Log for StdoutLogger {
            fn enabled(&self, _metadata: &Metadata) -> bool {
                true
            }
            fn log(&self, record: &Record) {
                if self.enabled(record.metadata()) {
                    println!("[{}] {}", record.level(), record.args());
                }
            }
            fn flush(&self) {}
        }
        static LOGGER: StdoutLogger = StdoutLogger;
        let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Debug));
    }

    #[test]
    fn undo_stack() {
        setup_logging();

        let snap = |s: &str| Snapshot::new(s, 0, None);

        let mut s = SnapshotManager::new();
        assert_eq!(s.undos, vec![]);
        assert_eq!(s.redos, vec![]);

        s.add_snapshot(snap("apple"), false);
        assert_eq!(s.undos, vec![snap("apple")]);
        assert_eq!(s.redos, vec![]);

        s.add_snapshot(snap("banana"), false);
        assert_eq!(s.undos, vec![snap("apple"), snap("banana")]);
        assert_eq!(s.redos, vec![]);

        s.add_snapshot(snap("cow"), false);
        assert_eq!(s.undos, vec![snap("apple"), snap("banana"), snap("cow")]);
        assert_eq!(s.redos, vec![]);

        let p = s.prev_snapshot(snap("cow"));
        assert_eq!(p.unwrap(), snap("banana"));

        let p = s.prev_snapshot(snap("banana"));
        assert_eq!(p.unwrap(), snap("apple"));

        let p = s.prev_snapshot(snap("apple"));
        assert!(p.is_none());

        let n = s.next_snapshot(snap("apple"));
        assert_eq!(n.unwrap(), snap("banana"));

        let n = s.next_snapshot(snap("banana"));
        assert_eq!(n.unwrap(), snap("cow"));
    }

    #[test]
    fn undo_redo_basic() {
        setup_logging();
        let mut tb = TextBuffer::new("Hello");
        tb.insert_str(" World");
        println!("{}", tb.debug_undo_stack());
        assert_eq!(tb.buffer(), "Hello World");
        tb.undo();
        println!("{}", tb.debug_undo_stack());
        assert_eq!(tb.buffer(), "Hello");
        tb.redo();
        println!("{}", tb.debug_undo_stack());
        assert_eq!(tb.buffer(), "Hello World");
    }

    #[test]
    fn undo_redo_multiple_steps() {
        setup_logging();
        let mut tb = TextBuffer::new("Start");
        tb.insert_str(" One");
        tb.insert_str(" Two");
        tb.insert_str(" Three");
        assert_eq!(tb.buffer(), "Start One Two Three");

        tb.undo();
        assert_eq!(tb.buffer(), "Start One Two");

        tb.undo();
        assert_eq!(tb.buffer(), "Start One");

        tb.redo();
        assert_eq!(tb.buffer(), "Start One Two");

        tb.redo();
        assert_eq!(tb.buffer(), "Start One Two Three");
    }

    #[test]
    fn undo_and_start_new_edit() {
        setup_logging();
        let mut tb = TextBuffer::new("Base");
        tb.insert_str(" Edit1");
        tb.insert_str(" Edit2");
        assert_eq!(tb.buffer(), "Base Edit1 Edit2");

        tb.undo();
        assert_eq!(tb.buffer(), "Base Edit1");

        // Start a new edit after undo
        tb.insert_str(" NewEdit");
        assert_eq!(tb.buffer(), "Base Edit1 NewEdit");

        // Redo should not work now
        tb.redo();
        assert_eq!(tb.buffer(), "Base Edit1 NewEdit");
    }

    #[test]
    fn undo_replace_word_under_cursor() {
        setup_logging();
        let mut tb = TextBuffer::new("The quick brown fox");
        let word = {
            let i = tb.buffer().find("quick").unwrap();
            &tb.buffer()[i..i + "quick".len()]
        };
        let sub_string = SubString::new(&tb.buffer(), word).unwrap();

        tb.replace_word_under_cursor("slow", &sub_string).unwrap();
        assert_eq!(tb.buffer(), "The slow brown fox");

        tb.undo();
        assert_eq!(tb.buffer(), "The quick brown fox");

        tb.redo();
        assert_eq!(tb.buffer(), "The slow brown fox");
    }

    #[test]
    fn undo_restores_selection_after_delete() {
        setup_logging();
        let mut tb = TextBuffer::new("Hello World");
        // Select "World"
        let start = tb.buffer().find("World").unwrap();
        let end = start + "World".len();
        tb.set_selection_range(start..end, false);
        assert_eq!(tb.selected_text().as_deref(), Some("World"));

        // Delete the selection.
        assert!(tb.delete_selection());
        assert_eq!(tb.buffer(), "Hello ");
        assert!(tb.selection_byte().is_none());

        // Undo should restore both the buffer and the selection.
        tb.undo();
        assert_eq!(tb.buffer(), "Hello World");
        assert_eq!(tb.selected_text().as_deref(), Some("World"));

        // Redo should re-apply the deletion and clear the selection again.
        tb.redo();
        assert_eq!(tb.buffer(), "Hello ");
        assert!(tb.selection_byte().is_none());
    }

    #[test]
    fn selection_change_does_not_create_snapshot() {
        setup_logging();
        let mut tb = TextBuffer::new("Hello World");
        tb.insert_str("!");
        assert_eq!(tb.buffer(), "Hello World!");

        // Move cursor and toggle selection a few times — these should not
        // produce any new undo entries.
        tb.set_selection_range(0..5, false);
        tb.clear_selection();
        tb.select_entire_buffer();
        tb.clear_selection();

        // A single undo should revert the only real edit (the "!" insertion).
        tb.undo();
        assert_eq!(tb.buffer(), "Hello World");
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SubString {
    pub s: String,    // contents expected to be found between start and end
    pub start: usize, // byte index in the original buffer
}

impl SubString {
    pub fn new(buffer: &str, substring: &str) -> anyhow::Result<Self> {
        let substring_ptr = substring.as_ptr() as usize;
        let buf_ptr = buffer.as_ptr() as usize;

        if substring_ptr < buf_ptr || substring_ptr + substring.len() > buf_ptr + buffer.len() {
            return Err(anyhow::anyhow!("Substring not found in buffer"));
        }

        let start = substring_ptr - buf_ptr;

        Ok(Self {
            s: substring.to_string(),
            start,
        })
    }

    pub fn end(&self) -> usize {
        self.start + self.s.len()
    }

    pub fn overlaps_with(&self, other: &SubString) -> bool {
        (self.start..=self.end()).contains(&other.start)
            || (other.start..=other.end()).contains(&self.start)
    }
}

impl AsRef<str> for SubString {
    fn as_ref(&self) -> &str {
        &self.s
    }
}
