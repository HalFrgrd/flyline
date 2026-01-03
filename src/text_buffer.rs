use unicode_segmentation::UnicodeSegmentation;
// use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use itertools::Itertools;
use unicode_width::UnicodeWidthStr;

pub struct TextBuffer {
    buf: String,
    // Byte index of the cursor position in the buffer
    // Need to ensure it lines up with grapheme boundaries.
    // The cursor is on the left of the grapheme at this index.
    cursor_byte: usize,
}

impl TextBuffer {
    pub fn new(starting_str: &str) -> Self {
        TextBuffer {
            buf: starting_str.to_string(),
            cursor_byte: starting_str.len(),
        }
    }

    pub fn buffer(&self) -> &str {
        &self.buf
    }

    pub fn insert_char(&mut self, c: char) {
        self.buf.insert(self.cursor_byte, c);
        self.cursor_byte += c.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.buf.insert_str(self.cursor_byte, s);
        self.cursor_byte += s.len();
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn move_left(&mut self) {
        self.cursor_byte = self
            .buf
            .grapheme_indices(true)
            .take_while(|(i, _)| *i < self.cursor_byte)
            .last()
            .map_or(0, |(i, _)| i);
    }

    pub fn move_right(&mut self) {
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

    pub fn delete_backwards(&mut self) {
        // delete one grapheme to the left
        let old_cursor_col = self.cursor_byte;
        self.move_left();
        assert!(self.cursor_byte <= old_cursor_col);
        self.buf.drain(self.cursor_byte..old_cursor_col);
    }

    pub fn delete_forwards(&mut self) {
        // delete one grapheme to the right
        let cursor_pos_right = self.right_move_pos();
        assert!(self.cursor_byte <= cursor_pos_right);
        self.buf.drain(self.cursor_byte..cursor_pos_right);
    }

    pub fn replace_word_under_cursor(&mut self, new_word: &str) {
        let (word_start, word_end, _) = extract_word_at_byte(&self.buf, self.cursor_byte);

        // Delete the word and position cursor at the start
        if word_start < word_end {
            self.buf.drain(word_start..word_end);
            self.cursor_byte = word_start;
        }
        self.insert_str(new_word);
    }

    pub fn move_one_word_left(&mut self) {
        self.cursor_byte = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte)
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

    fn right_word_move_pos(&self) -> usize {
        self.buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_byte)
            .skip_while(|(_, c)| !c.is_whitespace())
            .skip_while(|(_, c)| c.is_whitespace())
            .next()
            .map_or(self.buf.len(), |(i, _)| i)
    }

    pub fn move_one_word_right(&mut self) {
        self.cursor_byte = self.right_word_move_pos();
    }

    pub fn delete_one_word_left(&mut self) {
        let old_cursor_col = self.cursor_byte;
        self.move_one_word_left();
        assert!(self.cursor_byte <= old_cursor_col);
        self.buf.drain(self.cursor_byte..old_cursor_col);
    }

    pub fn delete_one_word_right(&mut self) {
        let cursor_pos_right = self.right_word_move_pos();
        assert!(self.cursor_byte <= cursor_pos_right);
        self.buf.drain(self.cursor_byte..cursor_pos_right);
    }

    pub fn move_to_start(&mut self) {
        self.cursor_byte = 0;
    }

    pub fn move_to_end(&mut self) {
        self.cursor_byte = self.buf.len();
    }

    pub fn is_cursor_at_end(&self) -> bool {
        self.cursor_byte == self.buf.len()
    }

    pub fn is_cursor_on_final_line(&self) -> bool {
        !self.buf[self.cursor_byte..].contains('\n')
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
        let mut cur_row = 0;
        let mut cur_col = 0;
        for (i, grapheme) in self.buf.grapheme_indices(true) {
            if cur_row == target_row && cur_col >= target_col {
                self.cursor_byte = i;
                return;
            }
            if grapheme.contains('\n') {
                cur_row += 1;
                cur_col = 0;
            } else {
                cur_col += grapheme.width_cjk();
            }
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
                col += grapheme.width_cjk(); // TOOD is cjk correct here?
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

    pub fn cursor_char_pos(&self) -> usize {
        self.buf[..self.cursor_byte].chars().count()
    }

    pub fn lines(&self) -> Vec<&str> {
        self.buf.lines().collect()
    }
}

pub fn extract_word_at_byte<'a>(s: &'a str, byte_pos: usize) -> (usize, usize, &'a str) {
    // Find the start of the word (last whitespace before byte_pos, or 0)
    let start = s
        .char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .filter(|(idx, _)| *idx < byte_pos)
        .last()
        .map_or(0, |(idx, c)| idx + c.len_utf8());

    // Find the end of the word (next whitespace at or after byte_pos, or end of string)
    let end = s
        .char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .filter(|(idx, _)| *idx >= byte_pos)
        .next()
        .map_or(s.len(), |(idx, _)| idx);

    (start, end, &s[start..end])
}

#[cfg(test)]
mod word_extraction_tests {
    use super::*;

    #[test]
    fn test_extract_word_at_byte() {
        let (_start, _end, word) = extract_word_at_byte("café option", "café o".len());
        assert_eq!(word, "option");

        let (_start, _end, word) = extract_word_at_byte("café option", "café ".len());
        assert_eq!(word, "option");

        let (_start, _end, word) = extract_word_at_byte("café option", "café".len());
        assert_eq!(word, "café");

        let (_start, _end, word) = extract_word_at_byte("grep 'pättërn' файл.txt 日本語", "grep 'pättërn' ".len());
        assert_eq!(word, "файл.txt");
    }
}

#[cfg(test)]
mod text_buffer_tests {
    use super::*;

    #[test]
    fn text_buffer_creation() {
        let tb = TextBuffer::new("abc");
        assert_eq!(tb.buffer(), "abc");
        assert_eq!(tb.cursor_byte, 3);
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
        let mut tb = TextBuffer::new("abc def   ");
        assert_eq!(tb.cursor_byte, "abc def   ".len());
        tb.move_one_word_left();
        assert_eq!(tb.cursor_byte, "abc ".len());
        tb.move_left();
        assert_eq!(tb.cursor_byte, "abc".len());
        tb.move_one_word_left();
        assert_eq!(tb.cursor_byte, "".len());
    }

    #[test]
    fn move_one_word_right() {
        let mut tb = TextBuffer::new("  abc def");
        tb.move_to_start();
        tb.move_one_word_right();
        assert_eq!(tb.cursor_byte, "  ".len());
        tb.move_one_word_right();
        assert_eq!(tb.cursor_byte, "  abc ".len());
        tb.move_one_word_right();
        assert_eq!(tb.cursor_byte, "  abc def".len());
    }

    // === moving lines tests ===

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
        assert_eq!(tb.cursor_byte,  "Line 1\nLine 2\nLine".len());
    }

    // === insert_char tests ===

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

    // === insert_str tests ===

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

    // === replace_word_under_cursor tests ===

    #[test]
    fn replace_word_under_cursor_at_start_of_line() {
        // Cursor at position 0 (start of line) with non-ASCII word
        let mut tb = TextBuffer::new("café option 日本語 🎯");
        tb.move_to_start(); // Cursor at position 0, at start of "café"
        tb.replace_word_under_cursor("coffee");
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
        tb.replace_word_under_cursor("file");
        assert_eq!(tb.buffer(), "git file --message 'привет' 🚀");
        assert_eq!(tb.cursor_byte, "git file".len());
    }

    #[test]
    fn replace_word_under_cursor_at_end_of_line() {
        // Cursor at the end of line on an emoji word
        let mut tb = TextBuffer::new("hello world 🎉🎊🎈");
        // Cursor is already at the end, on the emoji sequence
        tb.replace_word_under_cursor("celebration");
        assert_eq!(tb.buffer(), "hello world celebration");
        assert_eq!(tb.cursor_byte, "hello world celebration".len());
    }

    #[test]
    fn replace_word_under_cursor_on_blank_space() {
        // Cursor on a blank space between words with Arabic text
        let mut tb = TextBuffer::new("cat مرحبا --option 🔥");
        tb.move_to_start();
        for _ in 0..3 {
            tb.move_right();
        } // Position at "cat| مرحبا"
        tb.move_right(); // Now on the space: "cat | مرحبا"
        
        // When on whitespace, should replace the next word
        tb.replace_word_under_cursor("hello");
        assert_eq!(tb.buffer(), "cat hello --option 🔥");
        assert_eq!(tb.cursor_byte, "cat hello".len());
    }

    #[test]
    fn replace_word_under_cursor_chinese_at_word_start() {
        // Cursor at the start of a Chinese word
        let mut tb = TextBuffer::new("echo 文件名 --verbose 日本語");
        tb.move_to_start();
        for _ in 0..5 {
            tb.move_right();
        } // Position at "echo |文件名" (right at start of Chinese word)
        tb.replace_word_under_cursor("filename");
        assert_eq!(tb.buffer(), "echo filename --verbose 日本語");
        assert_eq!(tb.cursor_byte, "echo filename".len());
    }

    #[test]
    fn replace_word_under_cursor_accented_at_word_end() {
        // Cursor at the end of a word with heavy accents
        let mut tb = TextBuffer::new("find naïve résumé café 📄");
        tb.move_to_start();
        for _ in 0..10 {
            tb.move_right();
        } // Position at "find naïve| résumé" (end of "naïve")
        tb.replace_word_under_cursor("simple");
        assert_eq!(tb.buffer(), "find simple résumé café 📄");
        assert_eq!(tb.cursor_byte, "find simple".len());
    }

    #[test]
    fn replace_word_under_cursor_emoji_zwj_sequence() {
        // Cursor in middle of ZWJ emoji sequence (family emoji)
        let mut tb = TextBuffer::new("hello 👨‍👩‍👧‍👦 world ไฟล์ 🌟");
        tb.move_to_start();
        for _ in 0..7 {
            tb.move_right();
        } // Position in the middle of the family emoji
        tb.replace_word_under_cursor("family");
        assert_eq!(tb.buffer(), "hello family world ไฟล์ 🌟");
        assert_eq!(tb.cursor_byte, "hello family".len());
    }
}
