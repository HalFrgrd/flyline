use crate::dparser::{self, AnnotatedToken};
use crate::text_buffer::TextBuffer;

/// Check if typing `c` at the current cursor position would overwrite an auto-inserted
/// closing token rather than inserting a new character.
pub fn would_overwrite_auto_inserted_closing(
    tokens: &[AnnotatedToken],
    cursor_pos: usize,
    c: char,
) -> bool {
    if cursor_pos == 0 {
        return false;
    }
    if let Some(dparser_token) = tokens
        .iter()
        .find(|t| t.token.byte_range().contains(&cursor_pos))
    {
        if let dparser::TokenAnnotation::IsClosing {
            is_auto_inserted: true,
            ..
        } = dparser_token.annotation
        {
            return dparser_token.token.value.starts_with(c);
        }
    }
    false
}

/// After `c` has been inserted at `initial_cursor_pos`, potentially insert the
/// corresponding closing character.
///
/// `tokens` must represent the buffer state *before* `c` was inserted.
///
/// Returns `(closing_char, byte_pos)` if a closing character was inserted, or `None`.
pub fn insert_closing_char(
    tokens: &[AnnotatedToken],
    buffer: &mut TextBuffer,
    c: char,
    initial_cursor_pos: usize,
) -> Option<(char, usize)> {
    if let Some(closing) = dparser::DParser::closing_char_to_insert(tokens, c, initial_cursor_pos) {
        buffer.insert_char(closing);
        buffer.move_left();
        // After move_left, cursor is at the start of the auto-inserted closing char.
        log::info!(
            "Inserted auto-closing char '{}' at byte position {}",
            closing,
            buffer.cursor_byte_pos()
        );
        Some((closing, buffer.cursor_byte_pos()))
    } else {
        None
    }
}

/// Mark the token at `byte_pos` as auto-inserted in the token slice.
pub fn mark_auto_inserted_closing(tokens: &mut [AnnotatedToken], c: char, byte_pos: usize) {
    for token in tokens {
        if token.token.byte_range().start == byte_pos
            && token.token.value.starts_with(c)
            && let dparser::TokenAnnotation::IsClosing {
                is_auto_inserted, ..
            } = &mut token.annotation
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
/// paired with the token the cursor is right after, delete it.
///
/// This is called before a simple Backspace so that deleting an auto-paired opener also
/// removes the auto-inserted closer.
pub fn delete_auto_inserted_closing_if_present(tokens: &[AnnotatedToken], buffer: &mut TextBuffer) {
    let cursor_pos = buffer.cursor_byte_pos();
    if cursor_pos == 0 {
        return;
    }

    // Find the token that ends at cursor_pos (the one about to be deleted by Backspace).
    let opening_annotation = tokens
        .iter()
        .find(|t| t.token.byte_range().contains(&(cursor_pos - 1)))
        .map(|t| t.annotation.clone());

    if let Some(dparser::TokenAnnotation::IsOpening(Some(closing_idx))) = opening_annotation {
        // Check if the closing token starts immediately at cursor_pos and is auto-inserted.
        if let Some(closing_token) = tokens.get(closing_idx)
            && closing_token.token.byte_range().start == cursor_pos
            && let dparser::TokenAnnotation::IsClosing {
                is_auto_inserted: true,
                ..
            } = closing_token.annotation
        {
            buffer.delete_forwards();
        }
    }
}

/// Rebuild the token cache from `buffer`, transferring auto-inserted flags from `old_tokens`.
/// If `auto_close` is `Some((c, byte_pos))`, the new token at `byte_pos` is marked as
/// auto-inserted before the transfer.
fn rebuild_tokens(
    old_tokens: &[AnnotatedToken],
    buffer: &TextBuffer,
    auto_close: Option<(char, usize)>,
) -> Vec<AnnotatedToken> {
    let mut parser = dparser::DParser::from(buffer.buffer());
    parser.walk_to_end();
    let mut new_tokens = parser.into_tokens();
    if let Some((c, byte_pos)) = auto_close {
        mark_auto_inserted_closing(&mut new_tokens, c, byte_pos);
    }
    dparser::DParser::transfer_auto_inserted_flags(old_tokens, &mut new_tokens);
    new_tokens
}

/// Process inserting character `c` with auto-close logic, mirroring the `KeyCode::Char`
/// branch in `on_keypress`.
///
/// `tokens` must represent the buffer state *before* `c` is inserted.
/// Both `buffer` and `tokens` are updated in place.
pub fn process_char_insert(tokens: &mut Vec<AnnotatedToken>, buffer: &mut TextBuffer, c: char) {
    let stale_tokens = tokens.clone();
    if would_overwrite_auto_inserted_closing(&stale_tokens, buffer.cursor_byte_pos(), c) {
        buffer.move_right();
        *tokens = rebuild_tokens(&stale_tokens, buffer, None);
    } else {
        let initial_cursor_pos = buffer.cursor_byte_pos();
        buffer.insert_char(c);
        let auto_close = insert_closing_char(&stale_tokens, buffer, c, initial_cursor_pos);
        *tokens = rebuild_tokens(&stale_tokens, buffer, auto_close);
    }
}

/// Process a backspace with auto-close logic, mirroring the `KeyCode::Backspace` branch
/// in `on_keypress`.
///
/// Both `buffer` and `tokens` are updated in place.
pub fn process_backspace(tokens: &mut Vec<AnnotatedToken>, buffer: &mut TextBuffer) {
    let stale_tokens = tokens.clone();
    delete_auto_inserted_closing_if_present(&stale_tokens, buffer);
    buffer.delete_backwards();
    *tokens = rebuild_tokens(&stale_tokens, buffer, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dparser::DParser;

    /// Parse a string with `█` marking the cursor position.
    /// Returns `(tokens, buffer)` ready for use with the `process_*` functions.
    fn setup(input_with_cursor: &str) -> (Vec<AnnotatedToken>, TextBuffer) {
        let cursor_byte_pos = input_with_cursor
            .find('█')
            .expect("Cursor marker █ not found");
        let buffer_str = input_with_cursor.replace('█', "");
        let tokens = {
            let mut parser = DParser::from(&buffer_str);
            parser.walk_to_end();
            parser.into_tokens()
        };
        let mut buffer = TextBuffer::new(&buffer_str);
        buffer.try_move_cursor_to_byte_pos(cursor_byte_pos, false);
        (tokens, buffer)
    }

    /// Return the buffer string with `█` inserted at the current cursor position.
    fn buffer_with_cursor(buffer: &TextBuffer) -> String {
        let s = buffer.buffer();
        let pos = buffer.cursor_byte_pos();
        format!("{}█{}", &s[..pos], &s[pos..])
    }

    #[test]
    fn insert_double_quote_auto_closes() {
        let (mut tokens, mut buffer) = setup(r#"foo █"#);
        process_char_insert(&mut tokens, &mut buffer, '"');
        assert_eq!(buffer_with_cursor(&buffer), r#"foo "█""#);
    }

    #[test]
    fn insert_open_bracket_auto_closes() {
        let (mut tokens, mut buffer) = setup("foo █");
        process_char_insert(&mut tokens, &mut buffer, '[');
        assert_eq!(buffer_with_cursor(&buffer), "foo [█]");
    }

    #[test]
    fn backspace_after_two_brackets() {
        let (mut tokens, mut buffer) = setup("█");

        process_char_insert(&mut tokens, &mut buffer, '[');
        assert_eq!(buffer_with_cursor(&buffer), "[█]");

        process_char_insert(&mut tokens, &mut buffer, '[');
        assert_eq!(buffer_with_cursor(&buffer), "[[█]]");

        // Backspace: `[[` is a `DoubleLBracket` token paired with `DoubleRBracket("]]")`.
        // The auto-inserted `]]` is correctly tracked, so backspace removes both inner
        // `[` and its closing `]`, leaving the outer pair intact.
        process_backspace(&mut tokens, &mut buffer);
        assert_eq!(buffer_with_cursor(&buffer), "[█]");
    }
}
