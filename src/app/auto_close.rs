use crossterm::event::KeyEvent;

use crate::{
    app::{App, LastKeyPressAction},
    dparser,
};

impl<'a> App<'a> {
    pub(crate) fn handle_char_insertion(
        &mut self,
        _key: KeyEvent,
        c: char,
    ) -> Option<LastKeyPressAction> {
        if self.would_overwrite_auto_inserted_closing(c) {
            log::info!(
                "Not inserting char '{}' to avoid overwriting auto-inserted closing token",
                c
            );
            self.buffer.move_right();
        } else {
            let initial_cursor_pos = self.buffer.cursor_byte_pos();
            self.buffer.insert_char(c);
            if let Some((auto_char, auto_pos)) = self.insert_closing_char(c, initial_cursor_pos) {
                return Some(LastKeyPressAction::InsertedAutoClosing {
                    char: auto_char,
                    byte_pos: auto_pos,
                });
            }
        }
        None
    }

    pub(crate) fn would_overwrite_auto_inserted_closing(&self, c: char) -> bool {
        let cursor_pos = self.buffer.cursor_byte_pos();
        if cursor_pos == 0 {
            return false;
        }
        if let Some(dparser_token) = self
            .dparser_tokens_cache
            .iter()
            .find(|t| t.token.byte_range().contains(&cursor_pos))
        {
            if let Some(dparser::ClosingAnnotation {
                is_auto_inserted: true,
                ..
            }) = &dparser_token.annotations.closing
            {
                return dparser_token.token.value.starts_with(c);
            }
        }
        false
    }

    /// After a character `c` has been inserted into the buffer, insert the corresponding
    /// closing character when `c` is an unmatched opening delimiter.
    ///
    /// The decision is made using `dparser_tokens_cache`, which represents the buffer state
    /// *before* `c` was typed (one character out of date).  The cache is passed to
    /// [`buffer_format::FormattedBuffer::closing_char_to_insert`] which uses the stale token
    /// annotations to determine whether `c` opens a new pair or closes an existing one.
    ///
    /// Returns the byte position of the auto-inserted closing character, or `None` if no
    /// closing character was inserted.
    pub(crate) fn insert_closing_char(
        &mut self,
        c: char,
        initial_cursor_pos: usize,
    ) -> Option<(char, usize)> {
        if let Some(closing) = dparser::DParser::closing_char_to_insert(
            &self.dparser_tokens_cache,
            c,
            initial_cursor_pos,
        ) {
            self.buffer.insert_char(closing);
            self.buffer.move_left();
            // After move_left, cursor is at the start of the auto-inserted closing char.
            log::info!(
                "Inserted auto-closing char '{}' at byte position {}",
                closing,
                self.buffer.cursor_byte_pos()
            );
            Some((closing, self.buffer.cursor_byte_pos()))
        } else {
            None
        }
    }

    /// Mark the dparser token at `byte_pos` as auto-inserted in the cache.
    pub(crate) fn mark_auto_inserted_closing(
        dparser_tokens: &mut [dparser::AnnotatedToken],
        c: char,
        byte_pos: usize,
    ) {
        for token in dparser_tokens {
            if token.token.byte_range().start == byte_pos
                && token.token.value.starts_with(c)
                && let Some(dparser::ClosingAnnotation {
                    is_auto_inserted, ..
                }) = &mut token.annotations.closing
            {
                *is_auto_inserted = true;
                log::info!(
                    "Marked token '{}' at byte {} as auto-inserted",
                    token.token.value,
                    byte_pos
                );
                return;
            }
        }
        log::warn!(
            "Failed to mark auto-inserted closing char '{}' at byte position {}: no matching token found in cache",
            c,
            byte_pos
        );
    }

    /// If the token immediately to the right of the cursor is an auto-inserted closing token
    /// that is paired with the token the cursor is right after, delete it.
    /// This is called before a simple Backspace so that deleting an auto-paired opener also
    /// removes the auto-inserted closer.
    pub(crate) fn delete_auto_inserted_closing_if_present(&mut self) {
        let cursor_pos = self.buffer.cursor_byte_pos();
        if cursor_pos == 0 {
            return;
        }

        // Find the token that ends at cursor_pos (the one about to be deleted by Backspace).
        let opening_annotation = self
            .dparser_tokens_cache
            .iter()
            .find(|t| t.token.byte_range().contains(&(cursor_pos - 1)))
            .map(|t| t.annotations.opening.clone());

        if let Some(Some(dparser::OpeningState::Matched(closing_idx))) = opening_annotation {
            // Check if the closing token starts immediately at cursor_pos and is auto-inserted.
            if let Some(closing_token) = self.dparser_tokens_cache.get(closing_idx)
                && closing_token.token.byte_range().start == cursor_pos
                && let Some(dparser::ClosingAnnotation {
                    is_auto_inserted: true,
                    ..
                }) = closing_token.annotations.closing
            {
                self.buffer.delete_forwards();
            }
        }
    }
}
