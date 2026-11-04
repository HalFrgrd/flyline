use crate::editing_advanced::{
    WordPartMovementMode, clamp_to_char_boundary, fine_grained_word_left_pos_from,
    fine_grained_word_right_pos_from, is_fine_grained_boundary,
};
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
            .find(|(i, _)| *i > self.cursor_byte)
            .map_or(self.buf.len(), |(i, _)| i)
    }

    pub fn move_right_selection(&mut self) {
        self.start_selection_if_none();
        self.cursor_byte = self.right_move_pos();
    }

    pub(crate) fn move_one_word_left_pos(&self, delim: WordDelim) -> usize {
        self.move_one_word_left_pos_from(self.cursor_byte, delim)
    }

    fn move_one_word_left_pos_from(&self, start: usize, delim: WordDelim) -> usize {
        // The blank-line landing documented at `is_inside_delimiter_run`.
        if let Some("\n\n") = self.buf.get(start.saturating_sub(2)..start) {
            return start - 1;
        }
        self.buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= start)
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

    /// Steps the cursor back one `delim`-delimited word:
    /// `WordDelim::WhiteSpace` stops at whitespace, and
    /// `WordDelim::FineGrained` at whitespace or any ASCII punctuation. A blank
    /// line is the one exception, for either `delim`: when the two characters
    /// immediately before the cursor are both `\n`, the step stops at the index
    /// of the second of them, one byte short of where the next line starts;
    /// [`TextBuffer::move_one_word_left_far_side`] adjusts that landing, for
    /// the reason documented at `is_inside_delimiter_run`. Three word-part
    /// moves sit beside it: [`TextBuffer::move_one_word_left_fine_grained`] is
    /// the slash-aware rule that keeps `.` and `-` inside a token holding `/`,
    /// [`TextBuffer::move_one_word_left_strict`] splits such a token at every
    /// ASCII punctuation character, and
    /// [`TextBuffer::move_one_word_left_far_side`] composes this step with
    /// `WordDelim::FineGrained` and adjusts a landing that would fall inside a
    /// delimiter run.
    pub fn move_one_word_left(&mut self, delim: WordDelim) {
        self.cursor_byte = self.move_one_word_left_pos(delim);
    }

    pub(crate) fn move_one_word_right_pos(&self, delim: WordDelim) -> usize {
        self.move_one_word_right_pos_from(self.cursor_byte, delim)
    }

    fn move_one_word_right_pos_from(&self, start: usize, delim: WordDelim) -> usize {
        // The blank-line landing documented at `is_inside_delimiter_run`.
        if let Some("\n\n") = self.buf.get(start..start + 2) {
            return start + 1;
        }

        self.buf
            .char_indices()
            .skip_while(|(i, _)| *i < start)
            .skip_while(|(_, c)| delim.is_word_boundary(*c))
            .find(|(_, c)| delim.is_word_boundary(*c))
            .map_or(self.buf.len(), |(i, _)| i)
    }

    /// Steps the cursor forward one `delim`-delimited word:
    /// `WordDelim::WhiteSpace` stops at whitespace, and
    /// `WordDelim::FineGrained` at whitespace or any ASCII punctuation. A blank
    /// line is the one exception, for either `delim`: when the two characters
    /// at the cursor are both `\n`, the step stops at the index of the second
    /// of them, one byte short of where the next line starts;
    /// [`TextBuffer::move_one_word_right_far_side`] adjusts that landing, for
    /// the reason documented at `is_inside_delimiter_run`. Three word-part
    /// moves sit beside it: [`TextBuffer::move_one_word_right_fine_grained`] is
    /// the slash-aware rule that keeps `.` and `-` inside a token holding `/`,
    /// [`TextBuffer::move_one_word_right_strict`] splits such a token at every
    /// ASCII punctuation character, and
    /// [`TextBuffer::move_one_word_right_far_side`] composes this step with
    /// `WordDelim::FineGrained` and adjusts a landing that would fall inside a
    /// delimiter run.
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

    /// Steps the cursor to the start of the previous word-part under the
    /// slash-aware rule: a token holding `/` or `\` splits only at those
    /// boundaries, while every other token splits at all ASCII punctuation.
    /// [`TextBuffer::move_one_word_left_strict`] is the unconditional rule. The
    /// cursor must sit on a character boundary of the buffer, because the rule
    /// reads the token around it: a cursor past the end or inside a character
    /// panics, unlike the strict and far-side moves, which clamp such a value.
    pub fn move_one_word_left_fine_grained(&mut self) {
        self.cursor_byte = self.fine_grained_word_left_pos(WordPartMovementMode::SlashAware);
    }

    /// Steps the cursor to the end of the current word-part under the
    /// slash-aware rule documented on
    /// [`TextBuffer::move_one_word_left_fine_grained`], including the cursor
    /// precondition stated there. [`TextBuffer::move_one_word_right_strict`] is
    /// the unconditional rule.
    pub fn move_one_word_right_fine_grained(&mut self) {
        self.cursor_byte = self.fine_grained_word_right_pos(WordPartMovementMode::SlashAware);
    }

    /// Steps the cursor back to the start of the class run immediately to the
    /// left of the cursor, where a class is one of whitespace, ASCII
    /// punctuation (with `/` and `\` among it) and word characters, and a class
    /// run is one maximal stretch of characters of a single class. The strict
    /// and far-side word-part moves share three rules: a word character is any
    /// character that is neither whitespace nor ASCII punctuation, so non-ASCII
    /// punctuation such as `—` or `，` counts as a word character; a cursor
    /// value past the end of the buffer is clamped to the buffer end, and one
    /// inside a character is clamped back to that character's first byte; an
    /// active selection keeps its anchor, so only the cursor moves.
    pub fn move_one_word_left_strict(&mut self) {
        let start = clamp_to_char_boundary(&self.buf, self.cursor_byte);
        self.cursor_byte =
            fine_grained_word_left_pos_from(&self.buf, start, WordPartMovementMode::Strict);
    }

    /// Steps the cursor forward to the first character after the class run at
    /// the cursor, stopping at the end of the buffer when that run reaches it.
    /// The class definition, clamping, and selection behaviour are those of
    /// [`TextBuffer::move_one_word_left_strict`].
    pub fn move_one_word_right_strict(&mut self) {
        let start = clamp_to_char_boundary(&self.buf, self.cursor_byte);
        self.cursor_byte =
            fine_grained_word_right_pos_from(&self.buf, start, WordPartMovementMode::Strict);
    }

    /// Steps the cursor to the far side of the current or previous word, so it
    /// lands on a word edge rather than on a delimiter. A delimiter run is one
    /// maximal stretch of adjacent delimiters, where a delimiter is any
    /// character the strict classification splits at: from beside such a run
    /// the move skips the whole run and stops at the start of the previous
    /// word, so it never stops inside one. The cursor stops at byte 0 when no
    /// word precedes it; clamping and selection are as in
    /// [`TextBuffer::move_one_word_left_strict`].
    pub fn move_one_word_left_far_side(&mut self) {
        let start = clamp_to_char_boundary(&self.buf, self.cursor_byte);
        let landing = self.move_one_word_left_pos_from(start, WordDelim::FineGrained);
        self.cursor_byte = far_side_left_landing(&self.buf, landing);
    }

    /// Steps the cursor to the far side of the current or next word, so it
    /// lands on a word edge rather than on a delimiter. A delimiter run is one
    /// maximal stretch of adjacent delimiters, where a delimiter is any
    /// character the strict classification splits at: from beside such a run
    /// the move skips the whole run and stops at the end of the next word, so
    /// it never stops inside one. The cursor stops at the end of the buffer
    /// when no word follows; clamping and selection are as in
    /// [`TextBuffer::move_one_word_right_strict`].
    pub fn move_one_word_right_far_side(&mut self) {
        let start = clamp_to_char_boundary(&self.buf, self.cursor_byte);
        let landing = self.move_one_word_right_pos_from(start, WordDelim::FineGrained);
        self.cursor_byte = far_side_right_landing(&self.buf, landing);
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

        self.cursor_byte = clamp_to_char_boundary(&self.buf, byte_pos);
    }
}

/// Whether `pos` sits strictly inside a contiguous delimiter run, with a strict
/// delimiter on both sides. Each `move_one_word_{left,right}_pos_from` scan can
/// report such a position only through its `\n\n` special case, which steps one
/// byte into a blank line; every other landing it reports is already a run
/// edge.
fn is_inside_delimiter_run(buf: &str, pos: usize) -> bool {
    let before = buf.get(..pos).and_then(|head| head.chars().next_back());
    let at = buf.get(pos..).and_then(|tail| tail.chars().next());
    match (before, at) {
        (Some(before), Some(at)) => {
            is_fine_grained_boundary(before) && is_fine_grained_boundary(at)
        }
        _ => false,
    }
}

/// The far-side landing to the right of `landing`: `landing` itself whenever it
/// already sits on a run edge, otherwise the end of the next word, or the end
/// of the buffer when only delimiters follow.
fn far_side_right_landing(buf: &str, landing: usize) -> usize {
    if !is_inside_delimiter_run(buf, landing) {
        return landing;
    }
    let word_start = buf
        .get(landing..)
        .and_then(|tail| {
            tail.char_indices()
                .find(|(_, c)| !is_fine_grained_boundary(*c))
                .map(|(i, _)| landing + i)
        })
        .unwrap_or(buf.len());
    fine_grained_word_right_pos_from(buf, word_start, WordPartMovementMode::Strict)
}

/// The far-side landing to the left of `landing`: `landing` itself whenever it
/// already sits on a run edge, otherwise the start of the previous word, or
/// byte 0 when only delimiters precede it.
fn far_side_left_landing(buf: &str, landing: usize) -> usize {
    if !is_inside_delimiter_run(buf, landing) {
        return landing;
    }
    let word_end = buf
        .get(..landing)
        .and_then(|head| {
            head.char_indices()
                .rev()
                .find(|(_, c)| !is_fine_grained_boundary(*c))
                .map(|(i, c)| i + c.len_utf8())
        })
        .unwrap_or(0);
    fine_grained_word_left_pos_from(buf, word_end, WordPartMovementMode::Strict)
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

    #[test]
    fn move_one_word_strict_stops_on_the_dot_inside_a_call() {
        let text = "this.function('/some/path', parameter2)";
        let mut tb = TextBuffer::new(text);
        tb.move_to_start();
        tb.move_one_word_right_strict();
        assert_eq!(tb.cursor_byte, "this".len());
        tb.move_one_word_right_strict();
        assert_eq!(tb.cursor_byte, "this.".len());

        // Right to left the dot is a stop of its own, so the cursor does not
        // jump from `function` straight across it to the start of `this`.
        tb.cursor_byte = "this.function".len();
        tb.move_one_word_left_strict();
        assert_eq!(tb.cursor_byte, "this.".len());
        tb.move_one_word_left_strict();
        assert_eq!(tb.cursor_byte, "this".len());
        tb.move_one_word_left_strict();
        assert_eq!(tb.cursor_byte, 0);
    }

    #[test]
    fn move_one_word_far_side_crosses_the_full_docker_tag_in_fewer_presses_than_strict() {
        // The literal reported command, so the "fewer presses to cross the
        // whole tag" claim rests on an assertion rather than on code
        // equivalence with a shorter case.
        let text = "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26.04";
        let tag_start = "docker image pull ".len();

        // Strict stops on both sides of every delimiter run, so the tag needs
        // one press per word run and per delimiter run.
        let mut strict = TextBuffer::new(text);
        strict.cursor_byte = tag_start;
        let mut strict_positions = Vec::new();
        while strict.cursor_byte < text.len() {
            strict.move_one_word_right_strict();
            strict_positions.push(strict.cursor_byte);
        }
        assert_eq!(
            strict_positions,
            vec![
                "docker image pull nvidia".len(),
                "docker image pull nvidia/".len(),
                "docker image pull nvidia/cuda".len(),
                "docker image pull nvidia/cuda:".len(),
                "docker image pull nvidia/cuda:13".len(),
                "docker image pull nvidia/cuda:13.".len(),
                "docker image pull nvidia/cuda:13.4".len(),
                "docker image pull nvidia/cuda:13.4.".len(),
                "docker image pull nvidia/cuda:13.4.2".len(),
                "docker image pull nvidia/cuda:13.4.2-".len(),
                "docker image pull nvidia/cuda:13.4.2-devel".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26.".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26.04".len(),
            ]
        );

        // Far-side skips each delimiter run and lands on the far edge of the
        // next word, so it crosses the tag in fewer presses.
        let mut far = TextBuffer::new(text);
        far.cursor_byte = tag_start;
        let mut far_positions = Vec::new();
        while far.cursor_byte < text.len() {
            far.move_one_word_right_far_side();
            far_positions.push(far.cursor_byte);
        }
        assert_eq!(
            far_positions,
            vec![
                "docker image pull nvidia".len(),
                "docker image pull nvidia/cuda".len(),
                "docker image pull nvidia/cuda:13".len(),
                "docker image pull nvidia/cuda:13.4".len(),
                "docker image pull nvidia/cuda:13.4.2".len(),
                "docker image pull nvidia/cuda:13.4.2-devel".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26.04".len(),
            ]
        );
        assert!(
            far_positions.len() < strict_positions.len(),
            "far-side crossed the full tag left to right in {} presses; strict took {}",
            far_positions.len(),
            strict_positions.len()
        );

        // The reported scenario navigates the tag from right to left, so the
        // comparison runs in that direction too. Read right to left, strict
        // stops at the start of every class run, so it needs one press per word
        // run and per delimiter run, the mirror of the rightward sequence.
        let mut strict_left = TextBuffer::new(text);
        strict_left.cursor_byte = text.len();
        let mut strict_left_positions = Vec::new();
        while strict_left.cursor_byte > tag_start {
            strict_left.move_one_word_left_strict();
            strict_left_positions.push(strict_left.cursor_byte);
        }
        assert_eq!(
            strict_left_positions,
            vec![
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26.".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-".len(),
                "docker image pull nvidia/cuda:13.4.2-devel".len(),
                "docker image pull nvidia/cuda:13.4.2-".len(),
                "docker image pull nvidia/cuda:13.4.2".len(),
                "docker image pull nvidia/cuda:13.4.".len(),
                "docker image pull nvidia/cuda:13.4".len(),
                "docker image pull nvidia/cuda:13.".len(),
                "docker image pull nvidia/cuda:13".len(),
                "docker image pull nvidia/cuda:".len(),
                "docker image pull nvidia/cuda".len(),
                "docker image pull nvidia/".len(),
                "docker image pull nvidia".len(),
                "docker image pull ".len(),
            ]
        );

        // Far-side crosses each delimiter run to the far edge of the word
        // behind it, so the tag needs fewer presses in this direction as well.
        let mut far_left = TextBuffer::new(text);
        far_left.cursor_byte = text.len();
        let mut far_left_positions = Vec::new();
        while far_left.cursor_byte > tag_start {
            far_left.move_one_word_left_far_side();
            far_left_positions.push(far_left.cursor_byte);
        }
        assert_eq!(
            far_left_positions,
            vec![
                "docker image pull nvidia/cuda:13.4.2-devel-ubuntu26.".len(),
                "docker image pull nvidia/cuda:13.4.2-devel-".len(),
                "docker image pull nvidia/cuda:13.4.2-".len(),
                "docker image pull nvidia/cuda:13.4.".len(),
                "docker image pull nvidia/cuda:13.".len(),
                "docker image pull nvidia/cuda:".len(),
                "docker image pull nvidia/".len(),
                "docker image pull ".len(),
            ]
        );
        assert!(
            far_left_positions.len() < strict_left_positions.len(),
            "far-side crossed the full tag right to left in {} presses; strict took {}",
            far_left_positions.len(),
            strict_left_positions.len()
        );
    }

    #[test]
    fn move_one_word_left_far_side() {
        let mut tb = TextBuffer::new("this.function('/some/path', parameter2)");
        tb.cursor_byte = "this.function".len();
        tb.move_one_word_left_far_side();
        assert_eq!(tb.cursor_byte, "this.".len());
        tb.move_one_word_left_far_side();
        assert_eq!(tb.cursor_byte, 0);
    }

    #[test]
    fn move_one_word_far_side_from_inside_a_word_lands_on_its_edge() {
        // Far-side lands on the word edge the strict classification reports,
        // not the slash-aware one, even when the token holds a path segment
        // boundary.
        let mut tb = TextBuffer::new("this.function");
        tb.cursor_byte = 2;
        tb.move_one_word_right_far_side();
        assert_eq!(tb.cursor_byte, "this".len());

        let mut tb2 = TextBuffer::new("nvidia/cuda:13.4");
        tb2.cursor_byte = "nvidia/".len();
        tb2.move_one_word_right_far_side();
        assert_eq!(tb2.cursor_byte, "nvidia/cuda".len());
        tb2.move_one_word_left_far_side();
        assert_eq!(tb2.cursor_byte, "nvidia/".len());
    }

    #[test]
    fn move_one_word_strict_crosses_whole_whitespace_runs() {
        let mut tb = TextBuffer::new("foo\nbar\n\nbaz");
        tb.cursor_byte = "foo\nbar".len();
        // A blank line is a whitespace run of two characters, and the move
        // crosses both rather than stopping between them.
        tb.move_one_word_right_strict();
        assert_eq!(tb.cursor_byte, "foo\nbar\n\n".len());
        tb.move_one_word_right_strict();
        assert_eq!(tb.cursor_byte, "foo\nbar\n\nbaz".len());
        tb.move_one_word_left_strict();
        assert_eq!(tb.cursor_byte, "foo\nbar\n\n".len());
    }

    #[test]
    fn move_one_word_far_side_crosses_a_blank_line_to_the_word_beyond_it() {
        // A blank line is the one landing the delegated scan reports strictly
        // inside a delimiter run, so the far side of it is the word beyond.
        let mut tb = TextBuffer::new("foo\n\nbar");
        tb.cursor_byte = "foo".len();
        tb.move_one_word_right_far_side();
        assert_eq!(tb.cursor_byte, "foo\n\nbar".len());

        let mut tb2 = TextBuffer::new("foo\n\nbar");
        tb2.cursor_byte = "foo\n\n".len();
        tb2.move_one_word_left_far_side();
        assert_eq!(tb2.cursor_byte, 0);

        // A blank line that reaches the end of the buffer stops at the edge.
        let mut tb3 = TextBuffer::new("foo\n\n");
        tb3.cursor_byte = "foo".len();
        tb3.move_one_word_right_far_side();
        assert_eq!(tb3.cursor_byte, "foo\n\n".len());
        tb3.move_one_word_left_far_side();
        assert_eq!(tb3.cursor_byte, 0);
    }

    // The strict and far-side moves classify only ASCII punctuation as a
    // delimiter, so non-ASCII punctuation stays inside the run it joins.
    #[test]
    fn word_part_moves_treat_non_ascii_punctuation_as_a_word_character() {
        let mut strict = TextBuffer::new("13.4—devel");
        strict.move_to_start();
        strict.move_one_word_right_strict();
        assert_eq!(strict.cursor_byte, "13".len());
        strict.move_one_word_right_strict();
        assert_eq!(strict.cursor_byte, "13.".len());
        strict.move_one_word_right_strict();
        assert_eq!(strict.cursor_byte, "13.4—devel".len());

        let mut far = TextBuffer::new("13.4—devel");
        far.move_to_start();
        far.move_one_word_right_far_side();
        assert_eq!(far.cursor_byte, "13".len());
        far.move_one_word_right_far_side();
        assert_eq!(far.cursor_byte, "13.4—devel".len());
    }

    #[test]
    fn word_part_moves_keep_a_pre_existing_selection_anchor() {
        let text = "this.function('/some/path', parameter2)";

        // Strict crosses the `.` and stops at the start of `function`.
        let mut strict = TextBuffer::new(text);
        strict.move_to_start();
        strict.start_selection_if_none();
        strict.cursor_byte = "this".len();
        strict.move_one_word_right_strict();
        assert_eq!(strict.selection_byte(), Some(0));
        assert_eq!(strict.selected_text().as_deref(), Some("this."));

        // Far-side stops at the end of `function`, then crosses the whole
        // `('/` run and stops at the end of `some`.
        let mut far = TextBuffer::new(text);
        far.move_to_start();
        far.start_selection_if_none();
        far.cursor_byte = "this".len();
        far.move_one_word_right_far_side();
        assert_eq!(far.selection_byte(), Some(0));
        assert_eq!(far.selected_text().as_deref(), Some("this.function"));
        far.move_one_word_right_far_side();
        assert_eq!(far.selection_byte(), Some(0));
        assert_eq!(far.selected_text().as_deref(), Some("this.function('/some"));
    }

    // The four strict and far-side cursor moves, for tests that exercise all of
    // them alike.
    const WORD_PART_MOVES: [fn(&mut TextBuffer); 4] = [
        TextBuffer::move_one_word_left_strict,
        TextBuffer::move_one_word_right_strict,
        TextBuffer::move_one_word_left_far_side,
        TextBuffer::move_one_word_right_far_side,
    ];

    #[test]
    fn word_part_moves_clamp_a_cursor_past_the_end_of_the_buffer() {
        let text = "cuda:13.4";
        for word_part_move in WORD_PART_MOVES {
            let mut past_end = TextBuffer::new(text);
            past_end.cursor_byte = text.len() + 5;
            word_part_move(&mut past_end);

            let mut at_end = TextBuffer::new(text);
            word_part_move(&mut at_end);

            assert_eq!(past_end.cursor_byte, at_end.cursor_byte);
        }
    }

    #[test]
    fn word_part_moves_clamp_a_cursor_inside_a_character() {
        // The `é` in "café x" occupies bytes 3 and 4, so byte 4 splits it and
        // the cursor is clamped back to byte 3.
        for word_part_move in WORD_PART_MOVES {
            let mut split_character = TextBuffer::new("café x");
            split_character.cursor_byte = 4;
            let mut at_character_start = TextBuffer::new("café x");
            at_character_start.cursor_byte = 3;

            word_part_move(&mut split_character);
            word_part_move(&mut at_character_start);

            assert_eq!(split_character.cursor_byte, at_character_start.cursor_byte);
        }
    }

    #[test]
    fn word_part_moves_stop_at_the_buffer_edge_when_no_word_is_left() {
        // A delimiter-only token, a whitespace-only buffer and an empty buffer
        // all run out of word characters.
        for text in ["///", "   ", ""] {
            let mut rightward = TextBuffer::new(text);
            rightward.move_one_word_right_strict();
            rightward.move_one_word_right_far_side();
            assert_eq!(rightward.cursor_byte, text.len());

            let mut leftward = TextBuffer::new(text);
            leftward.move_end_of_line();
            leftward.move_one_word_left_strict();
            leftward.move_one_word_left_far_side();
            assert_eq!(leftward.cursor_byte, 0);
        }
    }

    // The delimiter is spelled out here rather than taken from the crate, so
    // the run-edge oracle below can disagree with the implementation it checks.
    fn is_delimiter(c: char) -> bool {
        c.is_whitespace() || c.is_ascii_punctuation()
    }

    // The character immediately before `pos`, or `None` at the start of the
    // buffer.
    fn char_before(text: &str, pos: usize) -> Option<char> {
        text[..pos].chars().next_back()
    }

    // The character at `pos`, or `None` at the end of the buffer.
    fn char_at(text: &str, pos: usize) -> Option<char> {
        text[pos..].chars().next()
    }

    // Whether `pos` sits on the edge of a run, so the characters on either side
    // belong to different classes, or on the edge of the buffer.
    fn is_run_edge(text: &str, pos: usize) -> bool {
        match (char_before(text, pos), char_at(text, pos)) {
            (Some(before), Some(at)) => is_delimiter(before) != is_delimiter(at),
            _ => true,
        }
    }

    // Whether `pos` is the far side of the word run a rightward far-side move
    // crosses: a delimiter starts at `pos`, so the cursor sits at the end of
    // the crossed word, or `pos` is the end of the buffer.
    fn is_right_far_edge(text: &str, pos: usize) -> bool {
        char_at(text, pos).is_none_or(is_delimiter)
    }

    // Whether `pos` is the far side of the word run a leftward far-side move
    // crosses: a word character starts at `pos` behind a delimiter, so the
    // cursor sits at the start of the crossed word, or `pos` is the start of
    // the buffer.
    fn is_left_far_edge(text: &str, pos: usize) -> bool {
        if pos == 0 {
            return true;
        }
        match char_at(text, pos) {
            Some(at) => !is_delimiter(at) && char_before(text, pos).is_some_and(is_delimiter),
            None => false,
        }
    }

    #[test]
    fn far_side_landings_stay_on_the_far_edge_of_the_word_run_they_cross() {
        // The far-edge predicates above read the text alone, so a landing on
        // the near edge of the run the move has to cross, where the strict
        // moves stop, fails here even though it sits on a run edge. Far-side is
        // also `move_one_word_left/right(WordDelim::FineGrained)` plus the
        // inside-a-run adjustment, which is what the delegated-landing
        // comparisons pin.
        let text = "docker run nvidia/cuda:13.4\n\n\n  --gpus all\n\tfoo::bar \n\n";
        for cursor in (0..=text.len()).filter(|pos| text.is_char_boundary(*pos)) {
            let mut far = TextBuffer::new(text);
            far.cursor_byte = cursor;
            far.move_one_word_right_far_side();
            assert!(is_run_edge(text, far.cursor_byte));
            assert!(
                is_right_far_edge(text, far.cursor_byte),
                "far-side right from byte {cursor} landed on byte {}",
                far.cursor_byte
            );

            let mut delegated = TextBuffer::new(text);
            delegated.cursor_byte = cursor;
            let landing = delegated.move_one_word_right_pos(WordDelim::FineGrained);
            if is_run_edge(text, landing) {
                assert_eq!(far.cursor_byte, landing);
            }

            let mut far = TextBuffer::new(text);
            far.cursor_byte = cursor;
            far.move_one_word_left_far_side();
            assert!(is_run_edge(text, far.cursor_byte));
            assert!(
                is_left_far_edge(text, far.cursor_byte),
                "far-side left from byte {cursor} landed on byte {}",
                far.cursor_byte
            );

            let mut delegated = TextBuffer::new(text);
            delegated.cursor_byte = cursor;
            let landing = delegated.move_one_word_left_pos(WordDelim::FineGrained);
            if is_run_edge(text, landing) {
                assert_eq!(far.cursor_byte, landing);
            }
        }
    }
}
