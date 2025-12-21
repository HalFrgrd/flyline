use unicode_segmentation::UnicodeSegmentation;
// use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use itertools::Itertools;

pub struct TextBuffer {
    buf: String,
    // Byte index of the cursor position in the buffer
    // Need to ensure it lines up with grapheme boundaries.
    // The cursor is on the left of the grapheme at this index.
    cursor_col: usize,
}

impl TextBuffer {
    pub fn new(starting_str: &str) -> Self {
        TextBuffer {
            buf: starting_str.to_string(),
            cursor_col: starting_str.len(),
        }
    }

    pub fn buffer(&self) -> &str {
        &self.buf
    }

    pub fn insert_char(&mut self, c: char) {
        self.buf.insert(self.cursor_col, c);
        self.cursor_col += c.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.buf.insert_str(self.cursor_col, s);
        self.cursor_col += s.len();
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn cursor_2d_position(&self) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;
        for (i, ch) in self.buf.char_indices() {
            if i >= self.cursor_col {
                break;
            }
            if ch == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (row, col)
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor_col = self
            .buf
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < self.cursor_col)
            .last()
            .map_or(0, |(i, _)| i);
    }

    pub fn move_cursor_right(&mut self) {
        self.cursor_col = self.cursor_pos_right_move();
    }

    fn cursor_pos_right_move(&self) -> usize {
        // the next grapheme boundary after the cursor
        self.buf
            .grapheme_indices(true)
            .skip_while(|(i, _)| *i <= self.cursor_col)
            .next()
            .map_or(self.buf.len(), |(i, _)| i)
    }

    pub fn delete_backwards(&mut self) {
        // delete one grapheme to the left
        let old_cursor_col = self.cursor_col;
        self.move_cursor_left();
        assert!(self.cursor_col <= old_cursor_col);
        self.buf.drain(self.cursor_col..old_cursor_col);
    }

    pub fn delete_forwards(&mut self) {
        // delete one grapheme to the right
        let cursor_pos_right = self.cursor_pos_right_move();
        assert!(self.cursor_col <= cursor_pos_right);
        self.buf.drain(self.cursor_col..cursor_pos_right);
    }

    pub fn delete_word_under_cursor(&mut self) {
        todo!("Implement delete_word_under_cursor");
    }

    pub fn move_one_word_left(&mut self) {
        self.cursor_col = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_col)
            .skip_while(|(_, c)| c.is_whitespace())
            .tuple_windows()
            .find_map(|((i, c), (_, next_c))| {
                if !c.is_whitespace() && next_c.is_whitespace() {
                    Some(i)
                } else {
                    None
                }
            })
            .unwrap_or(0);
    }

    fn cursor_pos_right_word_move(&self) -> usize {
        self.buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_col)
            .skip_while(|(_, c)| !c.is_whitespace())
            .skip_while(|(_, c)| c.is_whitespace())
            .next()
            .map_or(self.buf.len(), |(i, _)| i)
    }

    pub fn move_one_word_right(&mut self) {
        self.cursor_col = self.cursor_pos_right_word_move();
    }

    pub fn delete_one_word_left(&mut self) {
        let old_cursor_col = self.cursor_col;
        self.move_one_word_left();
        assert!(self.cursor_col <= old_cursor_col);
        self.buf.drain(self.cursor_col..old_cursor_col);
    }

    pub fn delete_one_word_right(&mut self) {
        let cursor_pos_right = self.cursor_pos_right_word_move();
        assert!(self.cursor_col <= cursor_pos_right);
        self.buf.drain(self.cursor_col..cursor_pos_right);
    }

    pub fn move_to_start(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_to_end(&mut self) {
        self.cursor_col = self.buf.len();
    }

    pub fn is_cursor_at_end(&self) -> bool {
        self.cursor_col == self.buf.len()
    }

    pub fn is_cursor_on_final_line(&self) -> bool {
        !self.buf[self.cursor_col..].contains('\n')
    }

    pub fn move_end_of_line(&mut self) {
        todo!("Implement move_end_of_line");
    }

    pub fn move_start_of_line(&mut self) {
        todo!("Implement move_start_of_line");
    }

    pub fn move_line_up(&mut self) {
        todo!("Implement move_line_up");
    }

    pub fn move_line_down(&mut self) {
        todo!("Implement move_line_down");
    }

    pub fn cursor_row(&self) -> usize {
        0
    }

    pub fn lines(&self) -> Vec<&str> {
        self.buf.lines().collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_buffer_creation() {
        let tb = TextBuffer::new("abc");
        assert_eq!(tb.buffer(), "abc");
        assert_eq!(tb.cursor_col, 3);
    }

    #[test]
    fn delete_back() {
        let mut tb = TextBuffer::new("Hello, World!");
        tb.delete_backwards();
        assert_eq!(tb.buffer(), "Hello, World");
        tb.delete_backwards();
        assert_eq!(tb.buffer(), "Hello, Worl");
        tb.delete_backwards();
        assert_eq!(tb.buffer(), "Hello, Wor");
    }

    #[test]
    fn move_cursor_left() {
        let mut tb = TextBuffer::new("test 👩‍💻");
        assert_eq!(tb.cursor_col, 16);
        tb.move_cursor_left();
        assert_eq!(tb.cursor_col, 5);
        tb.move_cursor_left();
        tb.move_cursor_left();
        tb.move_cursor_left();
        tb.move_cursor_left();
        assert_eq!(tb.cursor_col, 1);
        tb.move_cursor_left();
        assert_eq!(tb.cursor_col, 0);
        tb.move_cursor_left();
        assert_eq!(tb.cursor_col, 0);
    }

    #[test]
    fn move_cursor_right() {
        let mut tb = TextBuffer::new("test 👩‍💻");
        tb.move_cursor_left();
        tb.move_cursor_left();
        tb.move_cursor_left();
        assert_eq!(tb.cursor_col, 3);
        tb.move_cursor_right();
        assert_eq!(tb.cursor_col, 4);
        tb.move_cursor_right();
        assert_eq!(tb.cursor_col, 5);
        tb.move_cursor_right();
        assert_eq!(tb.cursor_col, 16);
        tb.move_cursor_right();
        assert_eq!(tb.cursor_col, 16);
    }

    #[test]
    fn move_one_word_left() {
        let mut tb = TextBuffer::new("abc def   ");
        assert_eq!(tb.cursor_col, "abc def   ".len());
        tb.move_one_word_left();
        assert_eq!(tb.cursor_col, "abc ".len());
        tb.move_cursor_left();
        assert_eq!(tb.cursor_col, "abc".len());
        tb.move_one_word_left();
        assert_eq!(tb.cursor_col, "".len());
    }

    #[test]
    fn move_one_word_right() {
        let mut tb = TextBuffer::new("  abc def");
        tb.move_to_start();
        tb.move_one_word_right();
        assert_eq!(tb.cursor_col, "  ".len());
        tb.move_one_word_right();
        assert_eq!(tb.cursor_col, "  abc ".len());
        tb.move_one_word_right();
        assert_eq!(tb.cursor_col, "  abc def".len());
    }

    // === insert_char tests ===

    #[test]
    fn zwj_emoji_insertion() {
        let mut tb = TextBuffer::new("test ");
        assert_eq!(tb.cursor_col, 5);
        tb.insert_char('👩');
        assert_eq!(tb.cursor_col, 5 + 4);
        tb.insert_char('\u{200d}'); // ZWJ
        assert_eq!(tb.cursor_col, 5 + 4 + 3);
        tb.insert_char('💻');
        assert_eq!(tb.buffer(), "test 👩‍💻");
        assert_eq!(tb.cursor_col, 5 + 4 + 3 + 4);
    }

    #[test]
    fn insert_char_emoji_with_modifier() {
        // Emoji with skin tone modifier (should be treated as single grapheme)
        let mut tb = TextBuffer::new("wave ");
        tb.insert_char('👋');
        tb.insert_char('\u{1F3FB}'); // Light skin tone modifier
        assert_eq!(tb.buffer(), "wave 👋🏻");
        assert_eq!(tb.cursor_col, 13); // Base emoji (4 bytes) + modifier (4 bytes) + "wave " (5 bytes)
    }

    #[test]
    fn insert_char_combining_diacritics() {
        // Character with combining diacritical marks (NFD form)
        let mut tb = TextBuffer::new("caf");
        tb.insert_char('e');
        tb.insert_char('\u{0301}'); // Combining acute accent
        assert_eq!(tb.buffer(), "cafe\u{0301}"); // NFD (decomposed) form
        assert_eq!(tb.cursor_col, 6); // 'e' (1 byte) + combining accent (2 bytes) + "caf" (3 bytes)
    }

    #[test]
    fn insert_char_regional_indicator() {
        // Regional indicator symbols (flag emojis are pairs of these)
        let mut tb = TextBuffer::new("Flag: ");
        tb.insert_char('🇺'); // Regional indicator U
        tb.insert_char('🇸'); // Regional indicator S
        assert_eq!(tb.buffer(), "Flag: 🇺🇸");
        assert_eq!(tb.cursor_col, 14); // Each regional indicator is 4 bytes
    }

    // === insert_str tests ===

    #[test]
    fn insert_str_mixed_width_characters() {
        // Mix of ASCII, wide characters (CJK), and emoji
        let mut tb = TextBuffer::new("Start: ");
        tb.insert_str("Hello 世界 🌍");
        assert_eq!(tb.buffer(), "Start: Hello 世界 🌍");
        // "Start: " = 7, "Hello " = 6, "世界" = 6, " " = 1, "🌍" = 4 = 24 bytes total
        assert_eq!(tb.cursor_col, 24);
    }

    #[test]
    fn insert_str_family_emoji_sequence() {
        // Family emoji is a ZWJ sequence of multiple emojis
        let mut tb = TextBuffer::new("Family: ");
        tb.insert_str("👨‍👩‍👧‍👦"); // Man, woman, girl, boy with ZWJ
        assert_eq!(tb.buffer(), "Family: 👨‍👩‍👧‍👦");
        // This is: 👨 (4) + ZWJ (3) + 👩 (4) + ZWJ (3) + 👧 (4) + ZWJ (3) + 👦 (4) = 25 bytes
        assert_eq!(tb.cursor_col, 33); // "Family: " (8) + emoji sequence (25)
    }

    #[test]
    fn insert_str_right_to_left_text() {
        // Arabic and Hebrew text (right-to-left scripts)
        let mut tb = TextBuffer::new("Text: ");
        tb.insert_str("مرحبا שלום"); // Arabic "hello" + space + Hebrew "hello"
        assert_eq!(tb.buffer(), "Text: مرحبا שלום");
        // "Text: " = 6, "مرحبا" = 10 bytes, " " = 1, "שלום" = 8 bytes
        assert_eq!(tb.cursor_col, 25);
    }

    #[test]
    fn insert_str_zero_width_joiner_sequences() {
        // Multiple ZWJ sequences in one string
        let mut tb = TextBuffer::new("");
        tb.insert_str("👨‍💻 and 👩‍🔬"); // Programmer and scientist
        assert_eq!(tb.buffer(), "👨‍💻 and 👩‍🔬");
        // 👨‍💻 = 11 bytes, " and " = 5 bytes, 👩‍🔬 = 11 bytes
        assert_eq!(tb.cursor_col, 27);
    }
}
