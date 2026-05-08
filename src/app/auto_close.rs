use crate::{app::App, dparser, text_buffer::TextBuffer};

/// Returns the corresponding closing character for surrounding a selection,
/// or `None` if `c` is not a recognised pairing character.
pub(crate) fn surround_closing_char(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

pub(crate) fn handle_char_insertion(
    buffer: &mut TextBuffer,
    dparser_tokens_cache: &mut Vec<dparser::AnnotatedToken>,
    c: char,
) {
    if dparser::DParser::consume_overwritten_auto_inserted_closing(
        dparser_tokens_cache,
        c,
        buffer.cursor_byte_pos(),
    ) {
        log::info!(
            "Not inserting char '{}' to avoid overwriting auto-inserted closing token",
            c
        );
        buffer.move_right();
    } else {
        let inserted_pos = buffer.cursor_byte_pos();
        buffer.insert_char(c);

        let tokens_after_insertion = dparser::DParser::parse_and_transfer_auto_inserted_flags(
            buffer.buffer(),
            dparser_tokens_cache,
        );

        if let Some(closing) = dparser::DParser::closing_char_to_insert_after_insertion(
            &tokens_after_insertion,
            c,
            inserted_pos,
        ) {
            buffer.insert_char(closing);
            buffer.move_left();
            let closing_pos = buffer.cursor_byte_pos();
            let mut final_tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(
                buffer.buffer(),
                &tokens_after_insertion,
            );

            if dparser::DParser::mark_auto_inserted_closing(&mut final_tokens, closing, closing_pos)
            {
                log::info!(
                    "Inserted auto-closing char '{}' at byte position {}",
                    closing,
                    closing_pos
                );
            } else {
                log::warn!(
                    "Inserted auto-closing char '{}' at byte position {}, but failed to mark it in dparser cache",
                    closing,
                    closing_pos
                );
            }

            *dparser_tokens_cache = final_tokens;
        } else {
            *dparser_tokens_cache = tokens_after_insertion;
        }
    }
}

/// If the token immediately to the right of the cursor is an auto-inserted closing token
/// that is paired with the token the cursor is right after, delete it.
/// This is called before a simple Backspace so that deleting an auto-paired opener also
/// removes the auto-inserted closer.
pub(crate) fn delete_auto_inserted_closing_if_present(
    buffer: &mut TextBuffer,
    dparser_tokens_cache: &[dparser::AnnotatedToken],
) {
    let cursor_pos = buffer.cursor_byte_pos();

    if dparser::DParser::should_delete_auto_inserted_closing(dparser_tokens_cache, cursor_pos) {
        buffer.delete_right();
        return;
    }

    // Fallback for parser edge cases (notably consecutive `(` that can be tokenized
    // as arithmetic-command boundaries): if the cursor is directly between `(` and `)`,
    // delete the right paren first so Backspace removes the innermost pair.
    if cursor_pos > 0 {
        let left_char = buffer.buffer()[..cursor_pos].chars().next_back();
        let right_char = buffer.buffer()[cursor_pos..].chars().next();
        if left_char == Some('(') && right_char == Some(')') {
            buffer.delete_right();
        }
    }
}

impl<'a> App<'a> {
    pub(crate) fn handle_char_insertion(&mut self, c: char) {
        handle_char_insertion(&mut self.buffer, &mut self.dparser_tokens_cache, c);
    }

    pub(crate) fn delete_auto_inserted_closing_if_present(&mut self) {
        delete_auto_inserted_closing_if_present(&mut self.buffer, &self.dparser_tokens_cache);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_buffer::TextBuffer;

    fn parsed(input: &str) -> Vec<dparser::AnnotatedToken> {
        dparser::DParser::parse_and_annotate(input)
    }

    #[test]
    fn parser_driven_quote_autoclose_uses_post_insertion_buffer() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '"');

        assert_eq!(buffer.buffer(), "echo \"\"");
        assert_eq!(buffer.cursor_byte_pos(), 6);
    }

    #[test]
    fn parser_driven_quote_does_not_autoclose_when_it_closed_an_existing_pair() {
        let mut buffer = TextBuffer::new("echo \"hello");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '"');

        assert_eq!(buffer.buffer(), "echo \"hello\"");
        assert_eq!(buffer.cursor_byte_pos(), 12);
    }

    #[test]
    fn parser_driven_dollar_expansion_inside_double_quotes_still_autocloses() {
        let mut buffer = TextBuffer::new("\"$\"");
        let mut tokens = parsed(buffer.buffer());
        buffer.move_left();

        handle_char_insertion(&mut buffer, &mut tokens, '(');

        assert_eq!(buffer.buffer(), "\"$()\"");
        assert_eq!(buffer.cursor_byte_pos(), 3);
    }

    #[test]
    fn consume_overwritten_auto_inserted_closing_clears_flag_without_reparsing() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '"');
        assert_eq!(buffer.buffer(), "echo \"\"");
        assert_eq!(buffer.cursor_byte_pos(), 6);

        handle_char_insertion(&mut buffer, &mut tokens, '"');
        assert_eq!(buffer.buffer(), "echo \"\"");
        assert_eq!(buffer.cursor_byte_pos(), 7);
    }

    #[test]
    fn delete_helper_detects_matching_auto_inserted_closing() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '"');
        assert_eq!(buffer.buffer(), "echo \"\"");
        assert_eq!(buffer.cursor_byte_pos(), 6);

        delete_auto_inserted_closing_if_present(&mut buffer, &tokens);
        assert_eq!(buffer.buffer(), "echo \"");
        assert_eq!(buffer.cursor_byte_pos(), 6);

        let mut no_pair_buffer = TextBuffer::new("echo \"");
        let no_pair_tokens = parsed(no_pair_buffer.buffer());
        delete_auto_inserted_closing_if_present(&mut no_pair_buffer, &no_pair_tokens);
        assert_eq!(no_pair_buffer.buffer(), "echo \"");
    }

    #[test]
    fn inserting_opening_quote_inserts_closing_and_positions_cursor_inside() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '"');

        assert_eq!(buffer.buffer(), "echo \"\"");
        assert_eq!(buffer.cursor_byte_pos(), 6);
    }

    #[test]
    fn inserting_closing_quote_over_auto_inserted_one_moves_cursor_without_duplicating() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '"');
        assert_eq!(buffer.buffer(), "echo \"\"");
        assert_eq!(buffer.cursor_byte_pos(), 6);

        handle_char_insertion(&mut buffer, &mut tokens, '"');
        assert_eq!(buffer.buffer(), "echo \"\"");
        assert_eq!(buffer.cursor_byte_pos(), 7);
    }

    #[test]
    fn backspace_on_opener_also_removes_auto_inserted_closer() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '"');
        assert_eq!(buffer.buffer(), "echo \"\"");

        delete_auto_inserted_closing_if_present(&mut buffer, &tokens);
        buffer.delete_left();
        let tokens =
            dparser::DParser::parse_and_transfer_auto_inserted_flags(buffer.buffer(), &tokens);

        assert_eq!(buffer.buffer(), "echo ");
        assert_eq!(buffer.cursor_byte_pos(), 5);
        let _ = tokens;
    }

    #[test]
    fn single_quote_inside_cmdsubst_inside_double_quote_autocloses() {
        let mut buffer = TextBuffer::new("echo \"$(echo foo  )\"");
        buffer.move_left();
        buffer.move_left();
        buffer.move_left();
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '\'');

        assert_eq!(buffer.buffer(), "echo \"$(echo foo '' )\"");
        assert_eq!(buffer.cursor_byte_pos(), 18);
    }

    #[test]
    fn single_quote_inside_nested_cmdsubst_inside_double_quote_autocloses() {
        let mut buffer = TextBuffer::new("echo \"$($(echo foo  ))\"");
        let insertion_pos = buffer
            .buffer()
            .find("  ))\"")
            .expect("fixture should contain the nested cmdsubst tail")
            + 1;
        buffer.move_to_start();
        while buffer.cursor_byte_pos() < insertion_pos {
            buffer.move_right();
        }
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '\'');

        assert_eq!(buffer.buffer(), "echo \"$($(echo foo '' ))\"");
        assert_eq!(buffer.cursor_byte_pos(), insertion_pos + 1);
    }

    #[test]
    fn repeated_backspace_on_nested_auto_inserted_parens_removes_inner_pair_each_time() {
        let mut buffer = TextBuffer::new("");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '(');
        handle_char_insertion(&mut buffer, &mut tokens, '(');
        handle_char_insertion(&mut buffer, &mut tokens, '(');
        assert_eq!(buffer.buffer(), "((()))");
        assert_eq!(buffer.cursor_byte_pos(), 3);

        delete_auto_inserted_closing_if_present(&mut buffer, &tokens);
        buffer.delete_left();
        tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(buffer.buffer(), &tokens);
        assert_eq!(buffer.buffer(), "(())");
        assert_eq!(buffer.cursor_byte_pos(), 2);

        delete_auto_inserted_closing_if_present(&mut buffer, &tokens);
        buffer.delete_left();
        tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(buffer.buffer(), &tokens);
        assert_eq!(buffer.buffer(), "()");
        assert_eq!(buffer.cursor_byte_pos(), 1);

        delete_auto_inserted_closing_if_present(&mut buffer, &tokens);
        buffer.delete_left();
        tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(buffer.buffer(), &tokens);
        assert_eq!(buffer.buffer(), "");
        assert_eq!(buffer.cursor_byte_pos(), 0);

        let _ = tokens;
    }

    // ---- square-bracket auto-close tests (after flash upgrade) ----

    #[test]
    fn inserting_open_square_bracket_after_command_inserts_closing_and_positions_cursor_inside() {
        // `[` after a command word is a regular argument and should auto-close.
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '[');

        assert_eq!(buffer.buffer(), "echo []");
        assert_eq!(buffer.cursor_byte_pos(), 6);
    }

    #[test]
    fn inserting_close_square_bracket_over_auto_inserted_one_moves_cursor_without_duplicating() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '[');
        assert_eq!(buffer.buffer(), "echo []");
        assert_eq!(buffer.cursor_byte_pos(), 6);

        handle_char_insertion(&mut buffer, &mut tokens, ']');
        assert_eq!(buffer.buffer(), "echo []");
        assert_eq!(buffer.cursor_byte_pos(), 7);
    }

    #[test]
    fn backspace_on_open_square_bracket_also_removes_auto_inserted_close() {
        let mut buffer = TextBuffer::new("echo ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '[');
        assert_eq!(buffer.buffer(), "echo []");

        delete_auto_inserted_closing_if_present(&mut buffer, &tokens);
        buffer.delete_left();
        let tokens =
            dparser::DParser::parse_and_transfer_auto_inserted_flags(buffer.buffer(), &tokens);

        assert_eq!(buffer.buffer(), "echo ");
        assert_eq!(buffer.cursor_byte_pos(), 5);
        let _ = tokens;
    }

    #[test]
    fn open_square_bracket_at_start_of_command_is_not_autoclosed() {
        // At any command position (here: empty buffer; see the pipe/semicolon
        // tests below for the other cases) `[` is the POSIX `[` test command —
        // the user types `[ ... ]` themselves so we must not auto-insert `]`.
        let mut buffer = TextBuffer::new("");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '[');

        assert_eq!(buffer.buffer(), "[");
        assert_eq!(buffer.cursor_byte_pos(), 1);
    }

    #[test]
    fn open_square_bracket_after_pipe_is_in_command_position_and_not_autoclosed() {
        // After a pipe, the next word is a fresh command — so `[` is again at
        // command position and must not be auto-closed.
        let mut buffer = TextBuffer::new("echo hi | ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '[');

        assert_eq!(buffer.buffer(), "echo hi | [");
        assert_eq!(buffer.cursor_byte_pos(), 11);
    }

    #[test]
    fn open_square_bracket_after_semicolon_is_in_command_position_and_not_autoclosed() {
        let mut buffer = TextBuffer::new("echo hi; ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '[');

        assert_eq!(buffer.buffer(), "echo hi; [");
        assert_eq!(buffer.cursor_byte_pos(), 10);
    }

    #[test]
    fn open_square_bracket_in_middle_of_argument_list_is_autoclosed() {
        // `[` after an existing argument is not in command position and should
        // be auto-closed like any other `[` opener.
        let mut buffer = TextBuffer::new("ls -l ");
        let mut tokens = parsed(buffer.buffer());

        handle_char_insertion(&mut buffer, &mut tokens, '[');

        assert_eq!(buffer.buffer(), "ls -l []");
        assert_eq!(buffer.cursor_byte_pos(), 7);
    }
}
