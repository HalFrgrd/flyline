use crate::{SubString, TextBuffer, WordDelim};

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
    pub(crate) fn fine_grained_word_left_pos_from(&self, cursor_byte: usize) -> usize {
        let class_fn: fn(char) -> u8 = if Self::has_slash_in_word(&self.buf, cursor_byte) {
            Self::less_strict_class_slash_only
        } else {
            Self::less_strict_class
        };
        let mut iter = self
            .buf
            .char_indices()
            .rev()
            .skip_while(|(i, _)| *i >= cursor_byte);
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

    pub(crate) fn fine_grained_word_left_pos(&self) -> usize {
        self.fine_grained_word_left_pos_from(self.cursor_byte)
    }

    /// Computes the target cursor byte position when moving/deleting one
    /// fine-grained word to the right (stopping at punctuation or path-segment
    /// boundaries, with slash-only mode when the word under the cursor
    /// contains `/` or `\`).
    pub(crate) fn fine_grained_word_right_pos_from(&self, cursor_byte: usize) -> usize {
        let end = self.buf.len();
        let class_fn: fn(char) -> u8 = if Self::has_slash_in_word(&self.buf, cursor_byte) {
            Self::less_strict_class_slash_only
        } else {
            Self::less_strict_class
        };
        let mut iter = self
            .buf
            .char_indices()
            .skip_while(|(i, _)| *i < cursor_byte);
        match iter.next() {
            Some((_, first_c)) => {
                let class = class_fn(first_c);
                iter.find_map(|(i, c)| if class_fn(c) != class { Some(i) } else { None })
                    .unwrap_or(end)
            }
            None => end,
        }
    }

    pub(crate) fn fine_grained_word_right_pos(&self) -> usize {
        self.fine_grained_word_right_pos_from(self.cursor_byte)
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
            self.fine_grained_word_left_pos_from(after_ws_skip)
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
                .find(|(_, c)| delim.is_word_boundary(*c))
                .map_or(end, |(i, _)| i)
        } else {
            self.fine_grained_word_right_pos_from(after_ws_skip)
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

    pub fn is_cursor_on_s(&self, s: &str) -> Option<SubString> {
        if s.is_empty() {
            return None;
        }
        let cursor = self.cursor_byte;
        let mut start = 0;
        while let Some(pos) = self.buf[start..].find(s) {
            let actual_start = start + pos;
            let actual_end = actual_start + s.len();
            if actual_start <= cursor && cursor <= actual_end {
                return Some(SubString {
                    s: s.to_string(),
                    start: actual_start,
                });
            }
            start = actual_start + 1;
        }
        None
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
        tb.replace_word_under_cursor("coffee", &create_substring(tb.buffer(), "café"))
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
        tb.replace_word_under_cursor("file", &create_substring(tb.buffer(), "файл"))
            .unwrap();
        assert_eq!(tb.buffer(), "git file --message 'привет' 🚀");
        assert_eq!(tb.cursor_byte, "git file".len());
    }

    #[test]
    fn replace_word_under_cursor_at_end_of_line() {
        // Cursor at the end of line on an emoji word
        let mut tb = TextBuffer::new("hello world 🎉🎊🎈");
        // Cursor is already at the end, on the emoji sequence
        tb.replace_word_under_cursor("celebration", &create_substring(tb.buffer(), "🎉🎊🎈"))
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
        tb.replace_word_under_cursor("simple", &create_substring(tb.buffer(), "naïve"))
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
    fn delete_one_word_left_less_strict_single_space_also_deletes_word_part() {
        let mut tb = TextBuffer::new("foo bar");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "foo ");

        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "");
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
        assert_eq!(tb.buffer(), " abc::def::ghi   /etc/asd");
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
    fn delete_one_word_right_less_strict_single_space_also_deletes_word_part() {
        let mut tb = TextBuffer::new("foo bar");
        tb.cursor_byte = "foo".len();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "foo");
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
        assert_eq!(tb.buffer(), "/foo_bar/baz.jeb");
        // After consuming the single leading space and '.', the remaining word
        // starts with '/' so slash-only mode applies.
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

    #[test]
    fn test_is_cursor_on_s() {
        // Cursor at the end: "hello world|" (index 11)
        let tb = TextBuffer::new_with_cursor("hello world█");
        let sub = tb.is_cursor_on_s("world").unwrap();
        assert_eq!(sub.s, "world");
        assert_eq!(sub.start, 6);

        // Cursor inside: "hello wo|rld" (index 8)
        let tb = TextBuffer::new_with_cursor("hello wo█rld");
        let sub = tb.is_cursor_on_s("world").unwrap();
        assert_eq!(sub.s, "world");
        assert_eq!(sub.start, 6);

        // Cursor at start of word: "hello |world" (index 6)
        let tb = TextBuffer::new_with_cursor("hello █world");
        let sub = tb.is_cursor_on_s("world").unwrap();
        assert_eq!(sub.s, "world");
        assert_eq!(sub.start, 6);

        // Cursor not touching/on it: "|hello world" (index 0)
        let tb = TextBuffer::new_with_cursor("█hello world");
        assert!(tb.is_cursor_on_s("world").is_none());

        // Empty word
        let tb = TextBuffer::new_with_cursor("hello █world");
        assert!(tb.is_cursor_on_s("").is_none());
    }
}
