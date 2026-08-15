use crate::TextBuffer;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

    pub fn cursor_byte_pos(&self) -> usize {
        self.cursor_byte
    }

    /// Convert a byte offset in `buf` to a character count (matching Readline's `readline_get_char_offset`).
    pub fn byte_to_char_offset(&self, byte_offset: usize) -> usize {
        let clamped = byte_offset.min(self.buf.len());
        self.buf[..clamped].chars().count()
    }

    /// Convert a character count into a byte index in `buf` (matching Readline's `readline_set_char_offset`).
    pub fn char_to_byte_offset(&self, char_offset: usize) -> usize {
        self.buf
            .char_indices()
            .nth(char_offset)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.buf.len())
    }

    /// Returns the current cursor position as a character offset.
    pub fn cursor_char_offset(&self) -> usize {
        self.byte_to_char_offset(self.cursor_byte)
    }

    /// Returns the current selection anchor position as a character offset.
    pub fn selection_char_offset(&self) -> Option<usize> {
        self.selection_byte.map(|b| self.byte_to_char_offset(b))
    }
}

#[cfg(test)]
mod test_accessors {
    // Add accessor-specific tests here if needed
    // Currently most accessor methods are tested implicitly in other modules
}
