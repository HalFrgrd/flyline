use crate::{SubString, TextBuffer, WordDelim};

/// The boundary rule a word-part move follows.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum WordPartMovementMode {
    /// The slash-aware rule: [`WordDelim::FineGrained`] boundaries, except that
    /// a token containing `/` or `\` splits only at those boundaries, so a file
    /// name is not split at its extension.
    SlashAware,
    /// [`WordDelim::FineGrained`] boundaries unconditionally, so `/` and `\`
    /// split like any other ASCII punctuation.
    Strict,
}

/// The class a character belongs to, which decides where a word-part move
/// stops.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum CharClass {
    /// Whitespace: the boundary every mode splits at, so a word-part never
    /// spans it.
    Whitespace,
    /// Characters the mode's classifier treats as punctuation: `/` and `\`
    /// alone for a slash-aware token that contains a path separator, and every
    /// ASCII punctuation character otherwise.
    Punctuation,
    /// Everything else: what a word-part is made of.
    Word,
}

/// Class of `c` under `is_boundary`, the mode's punctuation predicate: it
/// reports the characters the classifier splits word-parts at, and may or may
/// not also report whitespace. The branch order is load-bearing: whitespace is
/// tested first, so the fine-grained predicate, which reports whitespace
/// alongside punctuation, still yields `Whitespace`.
fn class_with(c: char, is_boundary: fn(char) -> bool) -> CharClass {
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if is_boundary(c) {
        CharClass::Punctuation
    } else {
        CharClass::Word
    }
}

/// The delimiter test of the fine-grained classification, which takes
/// [`WordDelim::FineGrained`] as its definition: whitespace or ASCII
/// punctuation.
pub(crate) fn is_fine_grained_boundary(c: char) -> bool {
    WordDelim::FineGrained.is_word_boundary(c)
}

fn class_by_fine_grained_boundary(c: char) -> CharClass {
    class_with(c, is_fine_grained_boundary)
}

fn is_path_segment_boundary(c: char) -> bool {
    c == '/' || c == '\\'
}

fn class_by_path_segment_boundary(c: char) -> CharClass {
    class_with(c, is_path_segment_boundary)
}

/// Whether the whitespace-delimited token at `cursor_byte` contains a path
/// segment boundary, which switches the slash-aware rule to splitting on those
/// boundaries alone.
fn has_slash_in_word(buf: &str, cursor_byte: usize) -> bool {
    let left = buf[..cursor_byte]
        .chars()
        .rev()
        .take_while(|c| !c.is_whitespace())
        .any(is_path_segment_boundary);
    let right = buf[cursor_byte..]
        .chars()
        .take_while(|c| !c.is_whitespace())
        .any(is_path_segment_boundary);
    left || right
}

impl WordPartMovementMode {
    /// The classifier this mode groups characters by. Each mode names its own
    /// classifier, so a mode added later cannot silently behave like
    /// [`WordPartMovementMode::SlashAware`].
    fn class_fn(self, buf: &str, cursor_byte: usize) -> fn(char) -> CharClass {
        match self {
            WordPartMovementMode::SlashAware => {
                if has_slash_in_word(buf, cursor_byte) {
                    class_by_path_segment_boundary
                } else {
                    class_by_fine_grained_boundary
                }
            }
            WordPartMovementMode::Strict => class_by_fine_grained_boundary,
        }
    }
}

impl TextBuffer {
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
    /// word-part to the left at the cursor, serving both
    /// [`WordPartMovementMode::SlashAware`] and
    /// [`WordPartMovementMode::Strict`]. Under
    /// [`WordPartMovementMode::SlashAware`] the boundary rule and cursor
    /// precondition are those of
    /// [`TextBuffer::move_one_word_left_fine_grained`]; under
    /// [`WordPartMovementMode::Strict`] every ASCII punctuation character is a
    /// boundary and the public move clamps the cursor like
    /// [`TextBuffer::move_one_word_left_strict`].
    pub(crate) fn fine_grained_word_left_pos(&self, mode: WordPartMovementMode) -> usize {
        fine_grained_word_left_pos_from(&self.buf, self.cursor_byte, mode)
    }

    /// Computes the target cursor byte position when moving/deleting one
    /// word-part to the right at the cursor, serving both
    /// [`WordPartMovementMode::SlashAware`] and
    /// [`WordPartMovementMode::Strict`]. Under
    /// [`WordPartMovementMode::SlashAware`] the boundary rule and cursor
    /// precondition are those of
    /// [`TextBuffer::move_one_word_right_fine_grained`]; under
    /// [`WordPartMovementMode::Strict`] every ASCII punctuation character is a
    /// boundary and the public move clamps the cursor like
    /// [`TextBuffer::move_one_word_right_strict`].
    pub(crate) fn fine_grained_word_right_pos(&self, mode: WordPartMovementMode) -> usize {
        fine_grained_word_right_pos_from(&self.buf, self.cursor_byte, mode)
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
            fine_grained_word_left_pos_from(
                &self.buf,
                after_ws_skip,
                WordPartMovementMode::SlashAware,
            )
        };

        assert!(new_cursor <= old_cursor_col);
        self.cursor_byte = new_cursor;
        self.buf.drain(new_cursor..old_cursor_col);
    }

    pub fn delete_right_one_word(&mut self, delim: WordDelim) {
        self.push_snapshot(true);
        let end_cursor = right_one_word_end_pos_in(&self.buf, self.cursor_byte, delim);
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
        self.selection_byte = None;
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

/// How the strict and far-side word-part moves normalise the cursor before they
/// scan: a position past the end of `buf` becomes `buf.len()`, and one inside a
/// character moves back to that character's first byte, the way
/// [`TextBuffer::try_move_cursor_to_byte_pos`] and
/// [`TextBuffer::set_selection_anchor`] clamp.
pub(crate) fn clamp_to_char_boundary(buf: &str, pos: usize) -> usize {
    let mut pos = pos.min(buf.len());
    while pos > 0 && !buf.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// `&str`-based core of [`TextBuffer::fine_grained_word_left_pos`]. The
/// boundary rule and the cursor precondition it imposes here are those of
/// [`TextBuffer::move_one_word_left_fine_grained`]. `Strict` mode reads no
/// token context and uses `cursor_byte` only as a scan threshold, so it neither
/// panics nor clamps: a past-the-end value scans as if it were `buf.len()`, and
/// a value inside a character scans from that character's first byte.
pub(crate) fn fine_grained_word_left_pos_from(
    buf: &str,
    cursor_byte: usize,
    mode: WordPartMovementMode,
) -> usize {
    let class_fn = mode.class_fn(buf, cursor_byte);
    let mut iter = buf
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

/// `&str`-based core of [`TextBuffer::fine_grained_word_right_pos`]. The
/// boundary rule and the cursor precondition it imposes here are those of
/// [`TextBuffer::move_one_word_right_fine_grained`]. `Strict` mode reads no
/// token context and uses `cursor_byte` only as a scan threshold, so it neither
/// panics nor clamps: a past-the-end value scans as if it were `buf.len()`, and
/// a value inside a character scans from the first byte of the character after
/// it, because the scan skips the character that contains the value.
pub(crate) fn fine_grained_word_right_pos_from(
    buf: &str,
    cursor_byte: usize,
    mode: WordPartMovementMode,
) -> usize {
    let end = buf.len();
    let class_fn = mode.class_fn(buf, cursor_byte);
    let mut iter = buf.char_indices().skip_while(|(i, _)| *i < cursor_byte);
    match iter.next() {
        Some((_, first_c)) => {
            let class = class_fn(first_c);
            iter.find_map(|(i, c)| if class_fn(c) != class { Some(i) } else { None })
                .unwrap_or(end)
        }
        None => end,
    }
}

/// Computes the byte position of the end of one word to the right of
/// `start_cursor`, matching the reach of [`TextBuffer::delete_right_one_word`]
/// and classifying characters the slash-aware way. `start_cursor` must be a
/// char boundary of `buf`, and a value that is not — past the end of `buf` or
/// inside a character — panics.
pub(crate) fn right_one_word_end_pos_in(buf: &str, start_cursor: usize, delim: WordDelim) -> usize {
    let end = buf.len();

    // First, find the position reached by skipping forward over any
    // contiguous run of whitespace immediately after `start_cursor`.
    let after_ws_skip = buf[start_cursor..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(end, |(i, _)| start_cursor + i);
    let ws_chars = buf[start_cursor..after_ws_skip].chars().count();

    // If there are 2+ contiguous whitespace chars, just consume the
    // whitespace. Otherwise (0 or 1 ws chars), also consume the next word
    // using the per-delim word-boundary logic.
    if ws_chars >= 2 {
        after_ws_skip
    } else if delim == WordDelim::WhiteSpace {
        buf.char_indices()
            .skip_while(|(i, _)| *i <= start_cursor)
            .skip_while(|(_, c)| delim.is_word_boundary(*c))
            .find(|(_, c)| delim.is_word_boundary(*c))
            .map_or(end, |(i, _)| i)
    } else {
        fine_grained_word_right_pos_from(buf, after_ws_skip, WordPartMovementMode::SlashAware)
    }
}

#[cfg(test)]
mod test_editing_advanced {
    use super::*;

    #[test]
    fn right_one_word_end_pos_whitespace() {
        let s = "git status --short";
        assert_eq!(
            crate::right_one_word_end_pos(s, 3, WordDelim::WhiteSpace),
            "git status".len()
        );
        assert_eq!(
            crate::right_one_word_end_pos(s, "git status".len(), WordDelim::WhiteSpace),
            s.len()
        );
        assert_eq!(
            crate::right_one_word_end_pos("find café now", 4, WordDelim::WhiteSpace),
            "find café".len()
        );
        // Two or more contiguous whitespace chars are consumed alone.
        assert_eq!(
            crate::right_one_word_end_pos("a   b", 1, WordDelim::WhiteSpace),
            4
        );
    }

    #[test]
    fn right_one_word_end_pos_fine_grained() {
        let s = "git commit -m 'msg'";
        assert_eq!(
            crate::right_one_word_end_pos(s, "git commit".len(), WordDelim::FineGrained),
            "git commit -".len()
        );
        assert_eq!(
            crate::right_one_word_end_pos(s, "git commit -".len(), WordDelim::FineGrained),
            "git commit -m".len()
        );
        let p = "cd /etc/nginx/conf.d";
        assert_eq!(
            crate::right_one_word_end_pos(p, 2, WordDelim::FineGrained),
            "cd /".len()
        );
        assert_eq!(
            crate::right_one_word_end_pos(p, "cd /".len(), WordDelim::FineGrained),
            "cd /etc".len()
        );
    }

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

    #[test]
    fn fine_grained_word_pos_from_strict_mode_splits_at_path_segment_boundaries() {
        let s = "docker image pull nvidia/cuda:13.4";
        let inside_path = "docker image pull nvidia/".len();
        assert_eq!(
            fine_grained_word_left_pos_from(s, s.len(), WordPartMovementMode::Strict),
            "docker image pull nvidia/cuda:13.".len()
        );
        assert_eq!(
            fine_grained_word_right_pos_from(s, inside_path, WordPartMovementMode::Strict),
            "docker image pull nvidia/cuda".len()
        );

        // The slash-aware rule crosses the path token instead of splitting it.
        assert_eq!(
            fine_grained_word_left_pos_from(s, s.len(), WordPartMovementMode::SlashAware),
            inside_path
        );
        assert_eq!(
            fine_grained_word_right_pos_from(s, inside_path, WordPartMovementMode::SlashAware),
            s.len()
        );
    }

    #[test]
    fn fine_grained_word_pos_from_steps_over_a_whole_punctuation_run() {
        let s = "abc::def";
        assert_eq!(
            fine_grained_word_left_pos_from(s, s.len(), WordPartMovementMode::Strict),
            "abc::".len()
        );
        assert_eq!(
            fine_grained_word_right_pos_from(s, "abc".len(), WordPartMovementMode::Strict),
            "abc::".len()
        );
    }

    #[test]
    fn fine_grained_boundary_predicate_agrees_with_the_strict_class_partition() {
        // The strict scanners split characters into three classes while the
        // far-side scans ask `is_fine_grained_boundary` for a delimiter, so the
        // two partitions agree only while that predicate reports whitespace as
        // well as punctuation.
        let samples: Vec<char> = ('!'..='~')
            .filter(|c| c.is_ascii_punctuation())
            .chain(['a', 'Z', '0', '9', ' ', '\t', '\n', '\r', 'é', '—', '，'])
            .collect();
        for c in samples {
            let class_is_word = class_by_fine_grained_boundary(c) == CharClass::Word;
            assert_eq!(
                class_is_word,
                !is_fine_grained_boundary(c),
                "class and boundary predicate disagree on {c:?}"
            );
        }
        assert_eq!(class_by_fine_grained_boundary(' '), CharClass::Whitespace);
        assert_eq!(class_by_fine_grained_boundary('\t'), CharClass::Whitespace);
        assert_eq!(class_by_fine_grained_boundary('-'), CharClass::Punctuation);
        assert_eq!(class_by_fine_grained_boundary('a'), CharClass::Word);
    }
}
