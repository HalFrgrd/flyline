use crate::{app::App, dparser};

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

impl<'a> App<'a> {
    pub(crate) fn handle_char_insertion(&mut self, c: char) {
        if dparser::DParser::consume_overwritten_auto_inserted_closing(
            &mut self.dparser_tokens_cache,
            c,
            self.buffer.cursor_byte_pos(),
        ) {
            log::info!(
                "Not inserting char '{}' to avoid overwriting auto-inserted closing token",
                c
            );
            self.buffer.move_right();
        } else {
            let inserted_pos = self.buffer.cursor_byte_pos();
            self.buffer.insert_char(c);

            let tokens_after_insertion = dparser::DParser::parse_and_transfer_auto_inserted_flags(
                self.buffer.buffer(),
                &self.dparser_tokens_cache,
            );

            if let Some(closing) = dparser::DParser::closing_char_to_insert_after_insertion(
                &tokens_after_insertion,
                c,
                inserted_pos,
            ) {
                self.buffer.insert_char(closing);
                self.buffer.move_left();
                let closing_pos = self.buffer.cursor_byte_pos();
                let mut final_tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(
                    self.buffer.buffer(),
                    &tokens_after_insertion,
                );

                if dparser::DParser::mark_auto_inserted_closing(
                    &mut final_tokens,
                    closing,
                    closing_pos,
                ) {
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

                self.dparser_tokens_cache = final_tokens;
            } else {
                self.dparser_tokens_cache = tokens_after_insertion;
            }
        }
    }

    /// If the token immediately to the right of the cursor is an auto-inserted closing token
    /// that is paired with the token the cursor is right after, delete it.
    /// This is called before a simple Backspace so that deleting an auto-paired opener also
    /// removes the auto-inserted closer.
    pub(crate) fn delete_auto_inserted_closing_if_present(&mut self) {
        if dparser::DParser::should_delete_auto_inserted_closing(
            &self.dparser_tokens_cache,
            self.buffer.cursor_byte_pos(),
        ) {
            self.buffer.delete_right();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(input: &str) -> Vec<dparser::AnnotatedToken> {
        dparser::DParser::parse_and_annotate(input)
    }

    #[test]
    fn parser_driven_quote_autoclose_uses_post_insertion_buffer() {
        let previous = parsed("echo ");
        let current = "echo \"";
        let tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(current, &previous);

        assert_eq!(
            dparser::DParser::closing_char_to_insert_after_insertion(&tokens, '\"', 5),
            Some('\"')
        );
    }

    #[test]
    fn parser_driven_quote_does_not_autoclose_when_it_closed_an_existing_pair() {
        let previous = parsed("echo \"hello");
        let current = "echo \"hello\"";
        let tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(current, &previous);

        assert_eq!(
            dparser::DParser::closing_char_to_insert_after_insertion(&tokens, '\"', 11),
            None
        );
    }

    #[test]
    fn parser_driven_dollar_expansion_inside_double_quotes_still_autocloses() {
        let previous = parsed("\"$\"");
        let current = "\"$(\"";
        let tokens = dparser::DParser::parse_and_transfer_auto_inserted_flags(current, &previous);

        assert_eq!(
            dparser::DParser::closing_char_to_insert_after_insertion(&tokens, '(', 2),
            Some(')')
        );
    }

    #[test]
    fn consume_overwritten_auto_inserted_closing_clears_flag_without_reparsing() {
        let mut tokens = parsed("\"\"");
        assert!(dparser::DParser::mark_auto_inserted_closing(
            &mut tokens,
            '"',
            1
        ));

        assert!(dparser::DParser::consume_overwritten_auto_inserted_closing(
            &mut tokens,
            '"',
            1
        ));
        assert!(
            tokens[1]
                .annotations
                .closing
                .as_ref()
                .is_some_and(|closing| !closing.is_auto_inserted)
        );
    }

    #[test]
    fn delete_helper_detects_matching_auto_inserted_closing() {
        let mut tokens = parsed("\"\"");
        assert!(dparser::DParser::mark_auto_inserted_closing(
            &mut tokens,
            '\"',
            1
        ));

        assert!(dparser::DParser::should_delete_auto_inserted_closing(
            &tokens, 1
        ));
        assert!(!dparser::DParser::should_delete_auto_inserted_closing(
            &tokens, 0
        ));
    }
}
