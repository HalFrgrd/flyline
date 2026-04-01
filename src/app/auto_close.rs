use crossterm::event::KeyEvent;

use crate::app::{App, LastKeyPressAction};

impl<'a> App<'a> {
    pub(crate) fn handle_char_insertion(
        &mut self,
        key: KeyEvent,
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
            self.buffer.on_keypress(key);
            if let Some((auto_char, auto_pos)) = self.insert_closing_char(c, initial_cursor_pos) {
                return Some(LastKeyPressAction::InsertedAutoClosing {
                    char: auto_char,
                    byte_pos: auto_pos,
                });
            }
        }
        None
    }
}
