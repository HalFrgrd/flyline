//! `App`-level glue around the standalone helpers in [`crate::auto_close`].
//!
//! All decision logic for "should this character be auto-closed / overwrite
//! an auto-inserted closer / pull along an auto-inserted closer on
//! Backspace" lives in `crate::auto_close` and is unit-tested there.  The
//! methods on [`App`] in this file simply mediate between the [`TextBuffer`]
//! and the [`AutoInsertedTracker`].

use crate::{
    app::App,
    auto_close::{should_delete_auto_inserted_closing_pair, would_overwrite_auto_inserted_closing},
    dparser,
};

impl<'a> App<'a> {
    pub(crate) fn handle_char_insertion(&mut self, c: char) {
        // Make sure the tracker reflects the current buffer before we make
        // any decisions on it.  Other actions running before us may have
        // mutated the buffer without going through the tracker.
        self.reconcile_auto_inserted_tracker();

        let cursor_pos = self.buffer.cursor_byte_pos();

        if would_overwrite_auto_inserted_closing(
            self.buffer.buffer(),
            cursor_pos,
            &self.auto_inserted_tracker,
            c,
        ) {
            log::info!(
                "Not inserting char '{}' to avoid overwriting auto-inserted closing token",
                c
            );
            self.auto_inserted_tracker.unmark(cursor_pos);
            self.buffer.move_right();
            // Buffer length is unchanged but record the snapshot anyway so
            // future reconciles see a clean baseline.
            self.last_buffer_for_tracker = self.buffer.buffer().to_string();
            return;
        }

        let initial_cursor_pos = cursor_pos;
        self.buffer.insert_char(c);
        // Reconcile shifts existing tracker entries past `initial_cursor_pos`
        // by the length of `c`.
        self.reconcile_auto_inserted_tracker();

        if let Some(closing) = dparser::DParser::closing_char_to_insert(
            &self.dparser_tokens_cache,
            c,
            initial_cursor_pos,
        ) {
            self.buffer.insert_char(closing);
            self.buffer.move_left();
            // Reconcile so the closer's insertion is accounted for before
            // we mark its position.
            self.reconcile_auto_inserted_tracker();
            let pos = self.buffer.cursor_byte_pos();
            self.auto_inserted_tracker.mark(pos);
            log::info!(
                "Inserted auto-closing char '{}' at byte position {}",
                closing,
                pos
            );
        }
    }

    /// If the character immediately to the right of the cursor is an
    /// auto-inserted closing character that pairs with the character about
    /// to be deleted by Backspace, delete it as well.
    pub(crate) fn delete_auto_inserted_closing_if_present(&mut self) {
        self.reconcile_auto_inserted_tracker();
        let cursor_pos = self.buffer.cursor_byte_pos();
        if should_delete_auto_inserted_closing_pair(
            self.buffer.buffer(),
            cursor_pos,
            &self.auto_inserted_tracker,
        ) {
            self.buffer.delete_right();
            // The deleted closer position is removed by the next reconcile.
            self.reconcile_auto_inserted_tracker();
        }
    }

    /// Reconcile [`Self::auto_inserted_tracker`] against the current buffer
    /// using the previously snapshotted buffer.
    pub(crate) fn reconcile_auto_inserted_tracker(&mut self) {
        let current = self.buffer.buffer();
        if current != self.last_buffer_for_tracker {
            self.auto_inserted_tracker
                .reconcile_after_buffer_change(&self.last_buffer_for_tracker, current);
            self.last_buffer_for_tracker = current.to_string();
        }
    }
}
