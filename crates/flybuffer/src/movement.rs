use crate::{TextBuffer, WordDelim};
use itertools::Itertools;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

    pub(crate) fn left_move_pos(&self) -> usize {
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

    pub(crate) fn right_move_pos(&self) -> usize {
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

    pub(crate) fn move_one_word_left_pos(&self, delim: WordDelim) -> usize {
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

    pub(crate) fn move_one_word_right_pos(&self, delim: WordDelim) -> usize {
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

        let mut pos = byte_pos.min(self.buf.len());
        while pos > 0 && !self.buf.is_char_boundary(pos) {
            pos -= 1;
        }
        self.cursor_byte = pos;
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
