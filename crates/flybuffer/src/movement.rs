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

    pub fn left_move_pos(&self) -> usize {
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

    pub fn right_move_pos(&self) -> usize {
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
            .find(|(_, c)| delim.is_word_boundary(*c))
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

    pub fn line_start_pos(&self) -> usize {
        self.buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte)
            .find_map(|(i, c)| if c == '\n' { Some(i + 1) } else { None })
            .unwrap_or(0)
    }

    pub fn line_end_pos(&self) -> usize {
        self.buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_byte)
            .find_map(|(i, c)| if c == '\n' { Some(i) } else { None })
            .unwrap_or(self.buf.len())
    }

    pub fn first_non_whitespace_pos(&self) -> usize {
        let start = self.line_start_pos();
        let end = self.line_end_pos();
        self.buf[start..end]
            .char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map_or(start, |(i, _)| start + i)
    }

    pub fn move_first_non_whitespace(&mut self) {
        self.cursor_byte = self.first_non_whitespace_pos();
    }

    pub fn word_end_pos(&self, delim: WordDelim) -> usize {
        if self.cursor_byte >= self.buf.len() {
            return self.buf.len();
        }
        let cur_char = self.buf[self.cursor_byte..].chars().next().unwrap_or(' ');
        let cur_class = char_class_vim(cur_char, delim);

        let mut iter = self
            .buf
            .char_indices()
            .skip_while(|(i, _)| *i <= self.cursor_byte)
            .peekable();

        let mut target_class = 0;
        let mut in_target_word = false;
        let mut prev_pos = self.cursor_byte;

        if cur_class != 0
            && iter
                .peek()
                .is_some_and(|&(_, next_c)| char_class_vim(next_c, delim) == cur_class)
        {
            in_target_word = true;
            target_class = cur_class;
        }

        while let Some(&(i, c)) = iter.peek() {
            let cls = char_class_vim(c, delim);
            if !in_target_word {
                if cls != 0 {
                    in_target_word = true;
                    target_class = cls;
                    prev_pos = i;
                }
                iter.next();
            } else if cls == target_class {
                prev_pos = i;
                iter.next();
            } else {
                return prev_pos;
            }
        }
        if in_target_word {
            prev_pos
        } else {
            self.buf.len().saturating_sub(1)
        }
    }

    pub fn move_word_end(&mut self, delim: WordDelim) {
        self.cursor_byte = self.word_end_pos(delim);
    }

    pub fn prev_word_end_pos(&self, delim: WordDelim) -> usize {
        if self.cursor_byte == 0 {
            return 0;
        }
        let cur_char = self.buf[self.cursor_byte.min(self.buf.len().saturating_sub(1))..]
            .chars()
            .next()
            .unwrap_or(' ');
        let cur_class = char_class_vim(cur_char, delim);

        let mut iter = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte)
            .peekable();

        if cur_class != 0 {
            while let Some(&(_, c)) = iter.peek() {
                if char_class_vim(c, delim) == cur_class {
                    iter.next();
                } else {
                    break;
                }
            }
        }

        while let Some(&(_, c)) = iter.peek() {
            if char_class_vim(c, delim) == 0 {
                iter.next();
            } else {
                break;
            }
        }

        iter.next().map(|(i, _)| i).unwrap_or(0)
    }

    pub fn move_prev_word_end(&mut self, delim: WordDelim) {
        self.cursor_byte = self.prev_word_end_pos(delim);
    }

    pub fn next_word_start_pos(&self, delim: WordDelim) -> usize {
        if self.cursor_byte >= self.buf.len() {
            return self.buf.len();
        }
        let cur_char = self.buf[self.cursor_byte..].chars().next().unwrap_or(' ');
        let cur_class = char_class_vim(cur_char, delim);
        let iter = self
            .buf
            .char_indices()
            .skip_while(|(i, _)| *i <= self.cursor_byte);

        let mut passed_cur_word = cur_class == 0;
        for (i, c) in iter {
            let cls = char_class_vim(c, delim);
            if !passed_cur_word {
                if cls != cur_class {
                    passed_cur_word = true;
                    if cls != 0 {
                        return i;
                    }
                }
            } else if cls != 0 {
                return i;
            }
        }
        self.buf.len()
    }

    pub fn move_next_word_start(&mut self, delim: WordDelim) {
        self.cursor_byte = self.next_word_start_pos(delim);
    }

    pub fn prev_word_start_pos(&self, delim: WordDelim) -> usize {
        if self.cursor_byte == 0 {
            return 0;
        }
        let iter = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte);
        let mut first_word_char = None;
        let mut word_class = 0;
        let mut last_i = 0;
        for (i, c) in iter {
            let cls = char_class_vim(c, delim);
            if first_word_char.is_none() {
                if cls != 0 {
                    first_word_char = Some(i);
                    word_class = cls;
                    last_i = i;
                }
            } else if cls == word_class {
                last_i = i;
            } else {
                return last_i;
            }
        }
        if first_word_char.is_some() { last_i } else { 0 }
    }

    pub fn move_prev_word_start(&mut self, delim: WordDelim) {
        self.cursor_byte = self.prev_word_start_pos(delim);
    }

    pub fn find_char_forward_pos(&self, target: char) -> Option<usize> {
        let line_end = self.line_end_pos();
        if self.cursor_byte >= line_end {
            return None;
        }
        self.buf[self.cursor_byte..line_end]
            .char_indices()
            .skip(1)
            .find(|(_, c)| *c == target)
            .map(|(i, _)| self.cursor_byte + i)
    }

    pub fn find_char_forward(&mut self, target: char) -> bool {
        if let Some(pos) = self.find_char_forward_pos(target) {
            self.cursor_byte = pos;
            true
        } else {
            false
        }
    }

    pub fn find_char_backward_pos(&self, target: char) -> Option<usize> {
        let line_start = self.line_start_pos();
        if self.cursor_byte <= line_start {
            return None;
        }
        self.buf[line_start..self.cursor_byte]
            .char_indices()
            .rev()
            .find(|(_, c)| *c == target)
            .map(|(i, _)| line_start + i)
    }

    pub fn find_char_backward(&mut self, target: char) -> bool {
        if let Some(pos) = self.find_char_backward_pos(target) {
            self.cursor_byte = pos;
            true
        } else {
            false
        }
    }

    pub fn till_char_forward_pos(&self, target: char) -> Option<usize> {
        let found = self.find_char_forward_pos(target)?;
        let prev = self.buf[..found]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)?;
        if prev >= self.cursor_byte {
            Some(prev)
        } else {
            None
        }
    }

    pub fn till_char_forward(&mut self, target: char) -> bool {
        if let Some(pos) = self.till_char_forward_pos(target) {
            self.cursor_byte = pos;
            true
        } else {
            false
        }
    }

    pub fn till_char_backward_pos(&self, target: char) -> Option<usize> {
        let found = self.find_char_backward_pos(target)?;
        let next = self.buf[found..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| found + i)?;
        if next <= self.cursor_byte {
            Some(next)
        } else {
            None
        }
    }

    pub fn till_char_backward(&mut self, target: char) -> bool {
        if let Some(pos) = self.till_char_backward_pos(target) {
            self.cursor_byte = pos;
            true
        } else {
            false
        }
    }

    pub fn matching_pair_pos(&self) -> Option<usize> {
        let pairs = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];
        let find_pair = |c: char| -> Option<(char, char, bool)> {
            for &(open, close) in &pairs {
                if c == open {
                    return Some((open, close, true));
                } else if c == close {
                    return Some((open, close, false));
                }
            }
            None
        };

        let start_pos = if let Some(c) = self.buf[self.cursor_byte..].chars().next() {
            if find_pair(c).is_some() {
                self.cursor_byte
            } else {
                let line_end = self.line_end_pos();
                self.buf[self.cursor_byte..line_end]
                    .char_indices()
                    .find_map(|(i, c)| {
                        if find_pair(c).is_some() {
                            Some(self.cursor_byte + i)
                        } else {
                            None
                        }
                    })?
            }
        } else {
            return None;
        };

        let c = self.buf[start_pos..].chars().next()?;
        let (open, close, is_forward) = find_pair(c)?;

        if is_forward {
            let mut depth = 0;
            for (i, ch) in self.buf[start_pos..].char_indices() {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(start_pos + i);
                    }
                }
            }
        } else {
            let mut depth = 0;
            for (i, ch) in self.buf[..=start_pos].char_indices().rev() {
                if ch == close {
                    depth += 1;
                } else if ch == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    pub fn move_matching_pair(&mut self) -> bool {
        if let Some(pos) = self.matching_pair_pos() {
            self.cursor_byte = pos;
            true
        } else {
            false
        }
    }

    pub fn text_object_word_range(
        &self,
        inner: bool,
        delim: WordDelim,
    ) -> Option<std::ops::Range<usize>> {
        if self.buf.is_empty() {
            return None;
        }
        let cur = self.cursor_byte.min(self.buf.len().saturating_sub(1));
        let cur_char = self.buf[cur..].chars().next().unwrap_or(' ');
        let cls = char_class_vim(cur_char, delim);
        if cls == 0 && inner {
            return None;
        }

        let mut start = cur;
        for (i, c) in self.buf[..=cur].char_indices().rev() {
            if char_class_vim(c, delim) == cls {
                start = i;
            } else {
                break;
            }
        }

        let mut end = cur;
        for (i, c) in self.buf[cur..].char_indices() {
            if char_class_vim(c, delim) == cls {
                end = cur + i + c.len_utf8();
            } else {
                break;
            }
        }

        if !inner {
            for (i, c) in self.buf[end..].char_indices() {
                if c.is_whitespace() {
                    end = end + i + c.len_utf8();
                } else {
                    break;
                }
            }
        }

        Some(start..end)
    }

    pub fn text_object_quotes_range(
        &self,
        quote: char,
        inner: bool,
    ) -> Option<std::ops::Range<usize>> {
        let line_start = self.line_start_pos();
        let line_end = self.line_end_pos();
        let line = &self.buf[line_start..line_end];
        let rel_cursor = self.cursor_byte.saturating_sub(line_start);

        let quote_positions: Vec<usize> = line
            .char_indices()
            .filter(|(_, c)| *c == quote)
            .map(|(i, _)| i)
            .collect();

        if quote_positions.len() < 2 {
            return None;
        }

        for window in quote_positions.chunks_exact(2) {
            let q1 = window[0];
            let q2 = window[1];
            if rel_cursor <= q2 {
                let start = if inner {
                    line_start + q1 + quote.len_utf8()
                } else {
                    line_start + q1
                };
                let end = if inner {
                    line_start + q2
                } else {
                    line_start + q2 + quote.len_utf8()
                };
                return Some(start..end);
            }
        }
        None
    }

    pub fn text_object_brackets_range(
        &self,
        open: char,
        close: char,
        inner: bool,
    ) -> Option<std::ops::Range<usize>> {
        let mut depth = 0;
        let mut open_idx = None;
        let search_limit = self.cursor_byte.min(self.buf.len().saturating_sub(1));
        for (i, c) in self.buf[..=search_limit].char_indices().rev() {
            if c == close && i != self.cursor_byte {
                depth -= 1;
            } else if c == open {
                if depth == 0 {
                    open_idx = Some(i);
                    break;
                }
                depth += 1;
            }
        }
        let open_pos = open_idx?;
        let mut depth = 0;
        let mut close_idx = None;
        for (i, c) in self.buf[open_pos..].char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(open_pos + i);
                    break;
                }
            }
        }
        let close_pos = close_idx?;
        let start = if inner {
            open_pos + open.len_utf8()
        } else {
            open_pos
        };
        let end = if inner {
            close_pos
        } else {
            close_pos + close.len_utf8()
        };
        Some(start..end)
    }
}

fn char_class_vim(c: char, delim: WordDelim) -> u8 {
    match delim {
        WordDelim::WhiteSpace => {
            if c.is_whitespace() {
                0
            } else {
                1
            }
        }
        WordDelim::FineGrained => {
            if c.is_whitespace() {
                0
            } else if c.is_ascii_punctuation() || !c.is_alphanumeric() && c != '_' {
                1
            } else {
                2
            }
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

    #[test]
    fn test_first_non_whitespace() {
        let mut tb = TextBuffer::new("   hello world");
        tb.move_end_of_line();
        tb.move_first_non_whitespace();
        assert_eq!(tb.cursor_byte, 3);

        let mut tb2 = TextBuffer::new("line 1\n    line 2");
        tb2.move_to_start();
        tb2.move_first_non_whitespace();
        assert_eq!(tb2.cursor_byte, 0);
        tb2.move_line_down();
        tb2.move_first_non_whitespace();
        assert_eq!(tb2.cursor_byte, 11);
    }

    #[test]
    fn test_vim_word_end_and_prev_end() {
        let mut tb = TextBuffer::new("abc def,ghi jkl");
        tb.move_to_start();
        tb.move_word_end(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 2); // 'c'
        tb.move_word_end(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 6); // 'f'
        tb.move_word_end(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 7); // ','
        tb.move_word_end(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 10); // 'i'
        tb.move_prev_word_end(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 7); // ','
        tb.move_prev_word_end(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 6); // 'f'
    }

    #[test]
    fn test_vim_next_word_and_prev_word_start() {
        let mut tb = TextBuffer::new("hello, world! 123");
        tb.move_to_start();
        tb.move_next_word_start(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 5); // ','
        tb.move_next_word_start(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 7); // 'w'
        tb.move_prev_word_start(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 5); // ','
        tb.move_prev_word_start(WordDelim::FineGrained);
        assert_eq!(tb.cursor_byte, 0); // 'h'
    }

    #[test]
    fn test_find_and_till_char() {
        let mut tb = TextBuffer::new("foo bar baz qux");
        tb.move_to_start();
        assert!(tb.find_char_forward('b'));
        assert_eq!(tb.cursor_byte, 4); // first 'b'
        assert!(tb.find_char_forward('b'));
        assert_eq!(tb.cursor_byte, 8); // second 'b'
        assert!(tb.find_char_backward('o'));
        assert_eq!(tb.cursor_byte, 2); // second 'o'
        assert!(tb.till_char_forward('a'));
        assert_eq!(tb.cursor_byte, 4); // before 'a' in 'bar'
    }

    #[test]
    fn test_matching_pair() {
        let mut tb = TextBuffer::new("fn(x, [y, {z}])");
        tb.move_to_start();
        assert!(tb.move_matching_pair());
        assert_eq!(tb.cursor_byte, 14); // matching ')'
        assert!(tb.move_matching_pair());
        assert_eq!(tb.cursor_byte, 2); // back to '('

        tb.try_move_cursor_to_byte_pos(6, false); // on '['
        assert!(tb.move_matching_pair());
        assert_eq!(tb.cursor_byte, 13); // matching ']'
    }

    #[test]
    fn test_text_objects() {
        let tb = TextBuffer::new("hello \"foo bar\" world");
        let tb_inside_quotes = TextBuffer {
            buf: tb.buf.clone(),
            cursor_byte: 8, // inside "foo bar"
            selection_byte: None,
            undo_redo: tb.undo_redo,
        };
        let inner_q = tb_inside_quotes
            .text_object_quotes_range('"', true)
            .unwrap();
        assert_eq!(&tb_inside_quotes.buf[inner_q], "foo bar");
        let around_q = tb_inside_quotes
            .text_object_quotes_range('"', false)
            .unwrap();
        assert_eq!(&tb_inside_quotes.buf[around_q], "\"foo bar\"");

        let tb_brack = TextBuffer::new("call(arg1, arg2)");
        let tb_inside_paren = TextBuffer {
            buf: tb_brack.buf.clone(),
            cursor_byte: 6, // on 'a' of arg1
            selection_byte: None,
            undo_redo: tb_brack.undo_redo,
        };
        let inner_p = tb_inside_paren
            .text_object_brackets_range('(', ')', true)
            .unwrap();
        assert_eq!(&tb_inside_paren.buf[inner_p], "arg1, arg2");
    }
}
