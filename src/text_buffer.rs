use std::fmt::Debug;

use crossterm::event::KeyEvent;
use unicode_segmentation::UnicodeSegmentation;
// use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use itertools::Itertools;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Eq, PartialEq)]
struct Snapshot {
    buf: String,
    cursor_byte: usize,
}

impl Snapshot {
    pub fn new(buf: &str, cursor_byte: usize) -> Self {
        Snapshot {
            buf: buf.to_string(),
            cursor_byte,
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
    LessStrict,
}

impl WordDelim {
    fn is_word_boundary(&self, c: char) -> bool {
        match self {
            WordDelim::WhiteSpace => c.is_whitespace(),
            WordDelim::LessStrict => c.is_whitespace() || c.is_ascii_punctuation(),
        }
    }
}

pub struct TextBuffer {
    buf: String,
    // Byte index of the cursor position in the buffer
    // Need to ensure it lines up with grapheme boundaries.
    // The cursor is on the left of the grapheme at this index.
    cursor_byte: usize,
    undo_redo: SnapshotManager,
}

///////////////////////////////////////////////////////// misc
impl TextBuffer {
    pub fn new(starting_str: &str) -> Self {
        TextBuffer {
            buf: starting_str.to_string(),
            cursor_byte: starting_str.len(),
            undo_redo: SnapshotManager::new(),
        }
    }

    /// Handle basic text editing keypresses
    /// Useful reference:
    /// https://en.wikipedia.org/wiki/Table_of_keyboard_shortcuts#Command_line_shortcuts
    pub fn on_keypress(&mut self, key: KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key {
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.delete_backwards();
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                self.delete_until_start_of_line();
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::SUPER,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_until_start_of_line();
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                self.delete_one_word_left(WordDelim::LessStrict);
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                // control backspace show up as these ones for me
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('w'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                self.delete_one_word_left(WordDelim::WhiteSpace);
            }
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                self.delete_one_word_right(WordDelim::LessStrict);
            }
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                self.delete_one_word_right(WordDelim::WhiteSpace);
            }
            KeyEvent {
                code: KeyCode::Delete,
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                self.delete_until_end_of_line();
            }
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::SUPER,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_until_end_of_line();
            }
            KeyEvent {
                code: KeyCode::Delete,
                ..
            } => {
                self.delete_forwards();
            }
            KeyEvent {
                code: KeyCode::Home,
                ..
            }
            | KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::SUPER,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::SUPER,
                ..
            } => {
                self.move_start_of_line();
            }
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('b'), // Emacs-style. ghostty sends this for Alt+Left by default
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                self.move_one_word_left(WordDelim::WhiteSpace);
            }
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => {
                self.move_left();
            }
            KeyEvent {
                code: KeyCode::End, ..
            }
            | KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::SUPER,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::SUPER,
                ..
            } => {
                self.move_end_of_line();
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('f'), // Emacs-style. ghostty sends this for Alt+Right by default
                modifiers: KeyModifiers::ALT | KeyModifiers::META,
                ..
            } => {
                self.move_one_word_right(WordDelim::WhiteSpace);
            }
            KeyEvent {
                code: KeyCode::Right,
                ..
            } => {
                self.move_right();
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                if self.cursor_row() > 0 {
                    self.move_line_up();
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                if !self.is_cursor_on_final_line() {
                    self.move_line_down();
                }
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => {
                self.insert_char(c);
            }
            KeyEvent {
                code: KeyCode::Char('y'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::SUPER,
                ..
            } => {
                self.redo();
            }
            KeyEvent {
                code: KeyCode::Char('z'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::SUPER,
                ..
            } => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            _ => {}
        }
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

    pub fn move_one_word_left(&mut self, delim: WordDelim) {
        self.cursor_byte = self
            .buf
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
            .unwrap_or(0);
    }

    pub fn move_one_word_right(&mut self, delim: WordDelim) {
        self.cursor_byte = self
            .buf
            .char_indices()
            .skip_while(|(i, _)| *i < self.cursor_byte)
            .skip_while(|(_, c)| delim.is_word_boundary(*c))
            .skip_while(|(_, c)| !delim.is_word_boundary(*c))
            .next()
            .map_or(self.buf.len(), |(i, _)| i)
    }

    pub fn move_to_start(&mut self) {
        self.cursor_byte = 0;
    }

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
                cur_col += grapheme.width_cjk();
            }
        }
        self.cursor_byte = self.buf.len();
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
        self.buf.insert_str(self.cursor_byte, s);
        self.cursor_byte += s.len();
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

    pub fn delete_backwards(&mut self) {
        // delete one grapheme to the left
        self.push_snapshot(true);
        let old_cursor_col = self.cursor_byte;
        self.move_left();
        assert!(self.cursor_byte <= old_cursor_col);
        self.buf.drain(self.cursor_byte..old_cursor_col);
    }

    pub fn delete_forwards(&mut self) {
        // delete one grapheme to the right
        self.push_snapshot(true);
        let cursor_pos_right = self.right_move_pos();
        assert!(self.cursor_byte <= cursor_pos_right);
        self.buf.drain(self.cursor_byte..cursor_pos_right);
    }

    pub fn delete_one_word_left(&mut self, delim: WordDelim) {
        self.push_snapshot(true);
        let old_cursor_col = self.cursor_byte;
        let mut iter = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= self.cursor_byte);
        if delim == WordDelim::WhiteSpace {
            self.cursor_byte = iter
                .skip_while(|(_, c)| delim.is_word_boundary(*c))
                .tuple_windows()
                .find_map(|((i, c), (_, next_c))| {
                    if !delim.is_word_boundary(c) && delim.is_word_boundary(next_c) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
        } else {
            self.cursor_byte = match iter.next() {
                Some((first_i, first_c)) => {
                    let class = Self::less_strict_class(first_c);
                    iter.scan((first_i, first_c), |prev, (i, c)| {
                        let (prev_i, prev_c) = *prev;
                        let boundary = if Self::less_strict_class(prev_c) == class
                            && Self::less_strict_class(c) != class
                        {
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
            };
        }

        assert!(self.cursor_byte <= old_cursor_col);
        self.buf.drain(self.cursor_byte..old_cursor_col);
    }

    pub fn delete_one_word_right(&mut self, delim: WordDelim) {
        self.push_snapshot(true);
        let end = self.buf.len();
        let end_cursor = if delim == WordDelim::WhiteSpace {
            self.buf
                .char_indices()
                .skip_while(|(i, _)| *i <= self.cursor_byte)
                .skip_while(|(_, c)| delim.is_word_boundary(*c))
                .skip_while(|(_, c)| !delim.is_word_boundary(*c))
                .next()
                .map_or(end, |(i, _)| i)
        } else {
            let mut iter = self
                .buf
                .char_indices()
                .skip_while(|(i, _)| *i < self.cursor_byte);
            match iter.next() {
                Some((_, first_c)) => {
                    let class = Self::less_strict_class(first_c);
                    iter.find_map(|(i, c)| {
                        if Self::less_strict_class(c) != class {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(end)
                }
                None => end,
            }
        };

        assert!(end_cursor >= self.cursor_byte);
        self.buf.drain(self.cursor_byte..end_cursor);
    }

    pub fn replace_word_under_cursor(
        &mut self,
        new_word: &str,
        sub_string: &SubString,
    ) -> anyhow::Result<()> {
        let end = sub_string.start + sub_string.s.len();

        match self.buf.get(sub_string.start..end) {
            Some(s) if s == sub_string.s => {
                // Delete the word and position cursor at the start
                self.push_snapshot(false);
                self.buf.drain(sub_string.start..end);
                self.cursor_byte = sub_string.start;
                self.insert_str_no_snapshot(new_word);
                Ok(())
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
        tb.delete_backwards();
        assert_eq!(tb.buffer(), "Hello, World");
        tb.delete_backwards();
        assert_eq!(tb.buffer(), "Hello, Worl");
        tb.delete_backwards();
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
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo test ");
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo ");
    }

    #[test]
    fn delete_one_word_left_less_strict() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /etc/");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /etc");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   ");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::def::");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::def");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc::");
        tb.delete_one_word_left(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "cargo test abc");
    }

    #[test]
    fn delete_one_word_right() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_start_of_line();
        tb.delete_one_word_right(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), " test abc::def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), " abc::def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "   /etc/asd");
        tb.delete_one_word_right(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_right_less_strict() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_start_of_line();
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), " test abc::def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "test abc::def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), " abc::def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "abc::def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "::def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "def::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "::ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "ghi   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "   /etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "/etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "etc/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "/asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "asd");
        tb.delete_one_word_right(WordDelim::LessStrict);
        assert_eq!(tb.buffer(), "");
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
    pub fn substring_matches(&self, sub_string: &SubString) -> bool {
        match self.buf.get(sub_string.start..sub_string.end()) {
            Some(s) => s == sub_string.s,
            None => false,
        }
    }

    pub fn cursor_in_substring(&self, sub_string: &SubString) -> bool {
        self.cursor_byte >= sub_string.start && self.cursor_byte <= sub_string.end()
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
        Snapshot::new(&self.buf, self.cursor_byte)
    }

    fn push_snapshot(&mut self, merge_with_recent: bool) {
        let snapshot = self.create_snapshot();
        log::debug!("Pushing snapshot: snapshot={:?}", snapshot);

        self.undo_redo.add_snapshot(snapshot, merge_with_recent);
    }

    fn undo(&mut self) {
        let current_state = self.create_snapshot();

        log::debug!("stacks: {}", self.debug_undo_stack());

        self.undo_redo.prev_snapshot(current_state).map(|snapshot| {
            // log::debug!("Undoing to state: snapshot={:?}", snapshot);
            self.buf = snapshot.buf;
            self.cursor_byte = snapshot.cursor_byte;
        });
        log::debug!("stacks: {}", self.debug_undo_stack());
    }

    fn redo(&mut self) {
        let current_state = self.create_snapshot();

        log::debug!("stacks: {}", self.debug_undo_stack());

        self.undo_redo.next_snapshot(current_state).map(|snapshot| {
            // log::debug!("Redoing to state: snapshot={:?}", snapshot);
            self.buf = snapshot.buf;
            self.cursor_byte = snapshot.cursor_byte;
        });
        log::debug!("stacks: {}", self.debug_undo_stack());
    }

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

    fn add_snapshot(&mut self, snapshot: Snapshot, merge_with_recent: bool) -> bool {
        if Some(&snapshot) == self.undos.last() {
            log::debug!("Snapshot identical to last one, not pushing a new one");
            return false;
        }

        let now = std::time::Instant::now();
        let duration_since_last = now.duration_since(self.last_snapshot_time);

        if merge_with_recent
            && !cfg!(test)
            && duration_since_last < std::time::Duration::from_millis(1000)
            && self.undos.len() > 0
        {
            log::debug!("Reusing recent snapshot: age {:?} ", duration_since_last);
        } else {
            self.last_snapshot_time = now;
            log::debug!("Pushing new snapshot onto undo stack: {:?}", snapshot);
            self.undos.push(snapshot);
        }

        self.redos.clear(); // clear redo stack on new edit
        true
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
                log::debug!("Redoing to snapshot: {:?}", snapshot);
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
                log::debug!("Undoing to snapshot: {:?}", snapshot);
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

        let snap = |s: &str| Snapshot::new(s, 0);

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
        (self.start..self.end()).contains(&other.start)
            || (other.start..other.end()).contains(&self.start)
    }
}
