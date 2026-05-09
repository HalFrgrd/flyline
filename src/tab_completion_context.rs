use flash::lexer::TokenKind;
use std::{borrow::Cow, vec};

use crate::{
    dparser::{DParser, ToInclusiveRange},
    globbing,
    text_buffer::SubString,
};

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub enum CompType {
    #[default]
    None,
    FirstWord,      // the first word under the cursor. cursor might be in the middle of it
    FuzzyFirstWord, // fuzzy-match commands when FirstWord prefix-matching finds nothing
    CommandComp {
        // "git commi asdf" with cursor just after com
        command_word: String, // "git"
    },
    FuzzyCommandComp {
        // Fallback after CommandComp: re-run programmable completion with just
        // the first character of the word under cursor as the prefix and
        // fuzzy-match the candidates against the full word under cursor.
        command_word: String,
    },
    EnvVariable,            // the env variable under the cursor, with the leading $
    TildeExpansion,         // the tilde under the cursor, e.g. "~us|erna"
    GlobExpansion,          // the glob pattern under the cursor, e.g. "*.rs|t"
    FilenameExpansion,      // the filename under the cursor, e.g. "fi|le.txt"
    FuzzyFilenameExpansion, // fuzzy-match files in the parent directory when FilenameExpansion finds nothing
}

impl CompType {
    pub fn is_glob_pattern(s: &str) -> bool {
        globbing::is_glob_pattern(s)
    }

    pub fn display_name(&self) -> &str {
        match self {
            CompType::None => "None",
            CompType::FirstWord => "FirstWord",
            CompType::FuzzyFirstWord => "FuzzyFirstWord",
            CompType::CommandComp { .. } => "CommandComp",
            CompType::FuzzyCommandComp { .. } => "FuzzyCommandComp",
            CompType::EnvVariable => "EnvVariable",
            CompType::TildeExpansion => "TildeExpansion",
            CompType::GlobExpansion => "GlobExpansion",
            CompType::FilenameExpansion => "FilenameExpansion",
            CompType::FuzzyFilenameExpansion => "FuzzyFilenameExpansion",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct CompletionContext<'a> {
    pub buffer: Cow<'a, str>,
    pub context: SubString,
    pub cursor_byte_pos: usize,
    pub word_under_cursor: SubString,
    pub comp_types: Vec<CompType>,
}

impl<'a> CompletionContext<'a> {
    pub fn new(
        buffer: &'a str,
        cursor_byte_pos: usize,
        context: &'a str,
        word_under_cursor: SubString,
    ) -> Self {
        if cfg!(test) {
            dbg!(&buffer);
            dbg!(&cursor_byte_pos);
            dbg!(&context);
            dbg!(&word_under_cursor);
        }

        let context = SubString::new(buffer, context).unwrap();
        let comp_types = Self::comp_types_for(&context, cursor_byte_pos, &word_under_cursor);

        CompletionContext {
            buffer: Cow::Borrowed(buffer),
            context,
            cursor_byte_pos,
            word_under_cursor,
            comp_types,
        }
    }

    pub fn dummy(buffer: &'a str) -> Self {
        CompletionContext {
            buffer: Cow::Borrowed(buffer),
            context: SubString::new(buffer, &buffer[0..0]).unwrap(),
            cursor_byte_pos: 0,
            word_under_cursor: SubString::new(buffer, &buffer[0..0]).unwrap(),
            comp_types: vec![CompType::FirstWord, CompType::FuzzyFirstWord],
        }
    }

    pub fn into_owned(self) -> CompletionContext<'static> {
        CompletionContext {
            buffer: Cow::Owned(self.buffer.into_owned()),
            context: self.context,
            cursor_byte_pos: self.cursor_byte_pos,
            word_under_cursor: self.word_under_cursor.to_owned(),
            comp_types: self.comp_types.clone(),
        }
    }

    fn comp_types_for(
        context: &SubString,
        cursor_byte_pos: usize,
        word_under_cursor: &SubString,
    ) -> Vec<CompType> {
        let wuc = word_under_cursor.as_ref();
        let mut comp_types = vec![];

        let wuc_looks_like_path = wuc.starts_with('~') || wuc.contains("/");
        let wuc_looks_like_env_var =
            (wuc.starts_with('$') || wuc.starts_with("\"$")) && !wuc_looks_like_path;

        // We could just rely on CompType::CommandComp and let bash do this expansion
        // but flyline is better and handles more edge cases around tilde expansion.
        // So we prioritize TildeExpansion over CommandComp if the word under cursor looks like it could be a tilde expansion.
        if wuc.starts_with('~') && !wuc.contains("/") {
            log::debug!("Detected tilde expansion context");
            comp_types.push(CompType::TildeExpansion);
        }

        let context_until_cursor = Self::context_until_cursor_for(context, cursor_byte_pos);
        if context.as_ref().trim().is_empty()
            || !context_until_cursor.chars().any(|c| c.is_whitespace())
        {
            comp_types.push(CompType::FirstWord);
            comp_types.push(CompType::FuzzyFirstWord);
        } else {
            let command_word = context
                .as_ref()
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();

            comp_types.push(CompType::CommandComp {
                command_word: command_word.clone(),
            });
        }

        if wuc_looks_like_env_var {
            comp_types.push(CompType::EnvVariable);
        } else if wuc.starts_with('~') && !wuc.contains("/") {
            comp_types.push(CompType::TildeExpansion);
        } else if CompType::is_glob_pattern(wuc) {
            comp_types.push(CompType::GlobExpansion);
        } else {
            comp_types.push(CompType::FilenameExpansion);

            for comp_type in &comp_types {
                if !wuc_looks_like_path && let CompType::CommandComp { command_word } = comp_type {
                    comp_types.push(CompType::FuzzyCommandComp {
                        command_word: command_word.clone(),
                    });
                    break;
                }
            }

            comp_types.push(CompType::FuzzyFilenameExpansion);
        }

        comp_types
    }

    fn context_until_cursor_for(context: &SubString, cursor_byte_pos: usize) -> &str {
        let end = cursor_byte_pos
            .saturating_sub(context.start)
            .min(context.as_ref().len());
        &context.as_ref()[..end]
    }

    #[cfg(test)]
    pub fn context_until_cursor(&self) -> &str {
        Self::context_until_cursor_for(&self.context, self.cursor_byte_pos)
    }

    pub fn cursor_byte_pos_context_relative(&self) -> usize {
        self.cursor_byte_pos.saturating_sub(self.context.start)
    }

    pub fn word_under_cursor_end_context_relative(&self) -> usize {
        self.word_under_cursor
            .end()
            .saturating_sub(self.context.start)
    }

    pub fn word_left_of_cursor(&self) -> &str {
        match self
            .buffer
            .get(self.word_under_cursor.start..self.cursor_byte_pos)
        {
            Some(s) => s,
            None => "",
        }
    }

    pub fn word_right_of_cursor(&self) -> &str {
        match self
            .buffer
            .get(self.cursor_byte_pos..self.word_under_cursor.end())
        {
            Some(s) => s,
            None => "",
        }
    }

    pub fn with_cursor_at_end_of_wuc(&'a self) -> CompletionContext<'a> {
        let cursor_byte_pos = self.word_under_cursor.end();
        CompletionContext {
            buffer: self.buffer.clone(),
            context: self.context.clone(),
            cursor_byte_pos,
            word_under_cursor: self.word_under_cursor.clone(),
            comp_types: self.comp_types.clone(),
        }
    }

    pub fn with_expanded_alias(&self, alias_def: &str) -> CompletionContext<'static> {
        let context = self.context.as_ref();
        let command_word_len = context
            .split_whitespace()
            .next()
            .map(str::len)
            .unwrap_or_default();

        let expanded_context = alias_def.to_string() + &context[command_word_len..];
        let cursor_byte_pos =
            Self::adjust_after_alias_expansion(self.cursor_byte_pos, command_word_len, alias_def);
        let word_under_cursor = SubString::from_parts(
            self.word_under_cursor.as_ref(),
            Self::adjust_after_alias_expansion(
                self.word_under_cursor.start,
                command_word_len,
                alias_def,
            ),
        );
        let context = SubString::from_parts(expanded_context, self.context.start);

        CompletionContext {
            buffer: Cow::Owned(self.buffer.clone().into_owned()),
            context,
            cursor_byte_pos,
            word_under_cursor,
            comp_types: self.comp_types.clone(),
        }
    }

    fn adjust_after_alias_expansion(pos: usize, command_word_len: usize, alias_def: &str) -> usize {
        if alias_def.len() >= command_word_len {
            pos.saturating_add(alias_def.len() - command_word_len)
        } else {
            pos.saturating_sub(command_word_len - alias_def.len())
        }
    }

    /// Returns a new `CompletionContext` with the word under the cursor
    /// replaced by `new_wuc`. The context is updated in-place (the bytes of
    /// the old wuc inside `context` are swapped for `new_wuc`), the wuc's
    /// `start` is preserved, and the cursor position is shifted by the
    /// length delta if it was at or after the end of the old wuc — otherwise
    /// the cursor is placed at the end of the new wuc.
    ///
    /// `comp_types` is intentionally not recomputed; callers are expected to
    /// use this for short-lived rewrites (e.g. running bash completion
    /// against a broader prefix) where the original comp-type pipeline still
    /// applies.
    pub fn with_wuc_replaced(&self, new_wuc: &str) -> CompletionContext<'static> {
        let context = self.context.as_ref();
        let wuc_start_in_context = self
            .word_under_cursor
            .start
            .saturating_sub(self.context.start);
        let wuc_end_in_context = wuc_start_in_context + self.word_under_cursor.as_ref().len();

        let mut new_context_str = String::with_capacity(
            context.len() - self.word_under_cursor.as_ref().len() + new_wuc.len(),
        );
        new_context_str.push_str(&context[..wuc_start_in_context]);
        new_context_str.push_str(new_wuc);
        new_context_str.push_str(&context[wuc_end_in_context..]);

        let new_context = SubString::from_parts(new_context_str, self.context.start);
        let new_word_under_cursor =
            SubString::from_parts(new_wuc.to_string(), self.word_under_cursor.start);

        let old_wuc_end_abs = self.word_under_cursor.end();
        let new_wuc_end_abs = new_word_under_cursor.end();
        let cursor_byte_pos = if self.cursor_byte_pos >= old_wuc_end_abs {
            new_wuc_end_abs + (self.cursor_byte_pos - old_wuc_end_abs)
        } else if self.cursor_byte_pos <= self.word_under_cursor.start {
            self.cursor_byte_pos
        } else {
            new_wuc_end_abs
        };

        CompletionContext {
            buffer: Cow::Owned(self.buffer.clone().into_owned()),
            context: new_context,
            cursor_byte_pos,
            word_under_cursor: new_word_under_cursor,
            comp_types: self.comp_types.clone(),
        }
    }
}

pub fn get_completion_context<'a>(
    buffer: &'a str,
    cursor_byte_pos: usize,
) -> CompletionContext<'a> {
    let mut parser = DParser::from(buffer);

    parser.walk_to_cursor(cursor_byte_pos);

    let context_tokens = parser.get_current_command_tokens();

    // if cfg!(test) {
    //     println!("Context tokens:");
    //     dbg!(cursor_byte_pos);
    //     for t in context_tokens.iter() {
    //         println!("{:#?} byte_range={:?}\n", t, t.token.byte_range());
    //     }
    // }

    // first try and find a non whitespace token that inclusivly contains the cursor.
    // if there is one, that is the word under the cursor.
    // Otherwise allow whitespace tokens to be the word under the cursor.
    // If there still isnt a node, then the word under the cursor is empty and the context is empty.
    let opt_cursor_node = match context_tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.token.kind.is_whitespace())
        .find(|(_, t)| {
            t.token
                .byte_range()
                .to_inclusive()
                .contains(&cursor_byte_pos)
        }) {
        Some(idx_and_node) => Some(idx_and_node),
        None => context_tokens.iter().enumerate().find(|(_, t)| {
            t.token
                .byte_range()
                .to_inclusive()
                .contains(&cursor_byte_pos)
        }),
    };

    let word_under_cursor_range = match opt_cursor_node {
        Some((_, cursor_node))
            if cursor_node.token.kind.is_whitespace()
                || cursor_node.token.kind == TokenKind::Newline =>
        {
            cursor_byte_pos..cursor_byte_pos
        }
        Some((node_idx, cursor_node)) if cursor_node.token.kind.is_word() => {
            let byte_range = cursor_node.token.byte_range();

            let mut start = byte_range.start;
            let mut end = byte_range.end;

            for i in (0..node_idx).rev() {
                let range_contains_dollar = buffer.get(start..end).is_some_and(|s| s.contains('$'));

                match context_tokens.get(i) {
                    Some(t) if t.annotations.is_env_var && !range_contains_dollar => {
                        start = t.token.byte_range().start;
                        while buffer.get(end.saturating_sub(1)..end) == Some(" ") {
                            end = end.saturating_sub(1);
                        }
                    }
                    Some(t)
                        if (t.token.kind == TokenKind::SingleQuote
                            || t.token.kind == TokenKind::Quote)
                            && (!range_contains_dollar
                                || cursor_node.token.value.contains('/')) =>
                    {
                        start = t.token.byte_range().start;
                    }
                    Some(t) if t.token.kind.is_word() && cursor_node.token.value.contains('/') => {
                        start = t.token.byte_range().start;
                    }
                    Some(t) if t.token.kind == TokenKind::Dollar => {
                        start = t.token.byte_range().start;
                    }
                    Some(t) if t.token.kind == TokenKind::RBrace => {
                        // Merge brace expressions like {foo,bar} with following glob patterns like *
                        // Find the matching LBrace by looking at the closing annotation
                        if let Some(closing) = &t.annotations.closing {
                            if let Some(opening_token) = context_tokens.get(closing.opening_idx) {
                                start = opening_token.token.byte_range().start;
                                break; // Stop here after merging the entire brace group
                            }
                        }
                    }
                    _ => break,
                }
            }

            // if let Some(cursor_to_end) = buffer.get(cursor_byte_pos..end) {
            //     // if there is a / in cursor_to_end, move the end closer to cursor so that we dont have the /
            //     if let Some(slash_pos) = cursor_to_end.find('/') {
            //         end = cursor_byte_pos + slash_pos;
            //     }
            // }

            start..end
        }
        Some((_, cursor_node)) => cursor_node.token.byte_range(),
        None if context_tokens.is_empty() => {
            return CompletionContext::dummy(buffer);
        }
        None => {
            log::error!(
                "cursor_byte_pos={cursor_byte_pos} is outside all context tokens; returning empty context"
            );
            for t in context_tokens.iter() {
                log::error!("  Token: {:?} byte_range={:?}", t, t.token.byte_range());
            }
            return CompletionContext::dummy(buffer);
        }
    };

    assert!(
        word_under_cursor_range
            .to_inclusive()
            .contains(&cursor_byte_pos)
    );

    let comp_context_range = if context_tokens.iter().all(|t| t.token.kind.is_whitespace()) {
        cursor_byte_pos..cursor_byte_pos
    } else {
        context_tokens.first().unwrap().token.byte_range().start
            ..context_tokens.last().unwrap().token.byte_range().end
    };

    let context = &buffer[comp_context_range];

    let word_under_cursor = SubString::new(buffer, &buffer[word_under_cursor_range]).unwrap();

    CompletionContext::new(buffer, cursor_byte_pos, context, word_under_cursor)
}

#[cfg(test)]
mod tests {
    use crate::text_buffer::TextBuffer;

    use super::*;

    fn run<'a>(input: &'a str, cursor_byte_pos: usize) -> CompletionContext<'a> {
        get_completion_context(input, cursor_byte_pos)
    }

    /// Parse a test string with `█` marking the cursor position.
    /// Returns (input_without_cursor, cursor_byte_pos).
    fn run_inline(input: &str) -> CompletionContext<'static> {
        let buffer = TextBuffer::new_with_cursor(input);
        let cursor_byte_pos = buffer.cursor_byte_pos();
        let input_without_cursor = buffer.buffer().to_string();
        let input_without_cursor: &'static str = Box::leak(input_without_cursor.into_boxed_str());
        run(input_without_cursor, cursor_byte_pos)
    }

    #[test]
    fn test_command_extraction() {
        let res = run_inline(r#"git com█mi café"#);

        assert_eq!(res.context_until_cursor(), "git com");
        assert_eq!(res.context, "git commi café");

        match res.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "git");
                assert_eq!(res.word_under_cursor.as_ref(), "commi");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_command_extraction_at_end() {
        let res = run_inline(r#"cd a█ b"#);
        assert_eq!(res.context_until_cursor(), "cd a");
        assert_eq!(res.context, "cd a b");

        match res.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(res.word_under_cursor.as_ref(), "a");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_command_extraction_at_end_2() {
        let res = run_inline(r#"cd  █"#);
        assert_eq!(res.context_until_cursor(), "cd  ");
        assert_eq!(res.context, "cd  ");

        match res.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(res.word_under_cursor.as_ref(), "");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_with_expanded_alias_expands_context_and_positions() {
        let res = run_inline(r#"fl_comp_alias b█an"#);
        let expanded = res.with_expanded_alias("fl_comp_util --nosort");

        assert_eq!(expanded.context, "fl_comp_util --nosort ban");
        assert_eq!(expanded.context_until_cursor(), "fl_comp_util --nosort b");
        assert_eq!(
            expanded.cursor_byte_pos_context_relative(),
            "fl_comp_util --nosort b".len()
        );
        assert_eq!(
            expanded.word_under_cursor_end_context_relative(),
            "fl_comp_util --nosort ban".len()
        );
        assert_eq!(
            expanded.word_under_cursor.start,
            "fl_comp_util --nosort ".len()
        );
        assert_eq!(expanded.word_under_cursor.as_ref(), "ban");
        assert_eq!(
            expanded.comp_types.first().unwrap(),
            &CompType::CommandComp {
                command_word: "fl_comp_alias".to_string()
            }
        );
    }

    #[test]
    fn test_with_expanded_alias_preserves_context_start() {
        let res = run_inline(r#"echo ok; ga ch█eckout"#);
        let expanded = res.with_expanded_alias("git add");

        assert_eq!(expanded.context.start, res.context.start);
        assert_eq!(expanded.context, "git add checkout");
        assert_eq!(expanded.context_until_cursor(), "git add ch");
        assert_eq!(
            expanded.cursor_byte_pos_context_relative(),
            "git add ch".len()
        );
        assert_eq!(
            expanded.word_under_cursor_end_context_relative(),
            "git add checkout".len()
        );
    }

    #[test]
    fn test_with_expanded_alias_handles_shorter_alias() {
        let res = run_inline(r#"longcmd u█p"#);
        let expanded = res.with_expanded_alias("l");

        assert_eq!(expanded.context, "l up");
        assert_eq!(expanded.context_until_cursor(), "l u");
        assert_eq!(expanded.cursor_byte_pos_context_relative(), "l u".len());
        assert_eq!(
            expanded.word_under_cursor_end_context_relative(),
            "l up".len()
        );
    }

    #[test]
    fn test_with_wuc_replaced_basic() {
        let res = run_inline(r#"git com█mit"#);
        assert_eq!(res.word_under_cursor.as_ref(), "commit");

        let replaced = res.with_wuc_replaced("c");

        assert_eq!(replaced.context, "git c");
        assert_eq!(replaced.context.start, res.context.start);
        assert_eq!(replaced.word_under_cursor.as_ref(), "c");
        assert_eq!(
            replaced.word_under_cursor.start,
            res.word_under_cursor.start
        );
        // cursor was inside the old wuc, so it gets clamped to the end of the new wuc
        assert_eq!(replaced.cursor_byte_pos_context_relative(), "git c".len());
        assert_eq!(
            replaced.word_under_cursor_end_context_relative(),
            "git c".len()
        );
        // comp_types are preserved verbatim
        assert_eq!(replaced.comp_types, res.comp_types);
    }

    #[test]
    fn test_with_wuc_replaced_cursor_after_wuc_shifts_by_delta() {
        let res = run_inline(r#"git commit█ -m"#);
        assert_eq!(res.word_under_cursor.as_ref(), "commit");

        let replaced = res.with_wuc_replaced("c");

        assert_eq!(replaced.context, "git c -m");
        assert_eq!(replaced.word_under_cursor.as_ref(), "c");
        // cursor was at end of old wuc; should now be at end of new wuc
        assert_eq!(replaced.cursor_byte_pos_context_relative(), "git c".len());
    }

    #[test]
    fn test_with_wuc_replaced_grows_wuc() {
        let res = run_inline(r#"git c█ -m"#);
        assert_eq!(res.word_under_cursor.as_ref(), "c");
        let cursor_offset_past_wuc = res.cursor_byte_pos - res.word_under_cursor.end();

        let replaced = res.with_wuc_replaced("commit");

        assert_eq!(replaced.context, "git commit -m");
        assert_eq!(replaced.word_under_cursor.as_ref(), "commit");
        assert_eq!(
            replaced.word_under_cursor.start,
            res.word_under_cursor.start
        );
        // cursor was past the wuc; preserves its offset past the (now longer) wuc
        assert_eq!(
            replaced.cursor_byte_pos - replaced.word_under_cursor.end(),
            cursor_offset_past_wuc
        );
    }

    #[test]
    fn test_with_wuc_replaced_preserves_context_start() {
        let res = run_inline(r#"echo ok; git com█mit"#);
        let replaced = res.with_wuc_replaced("c");

        assert_eq!(replaced.context.start, res.context.start);
        assert_eq!(replaced.context, "git c");
        assert_eq!(
            replaced.word_under_cursor.start,
            res.word_under_cursor.start
        );
    }

    #[test]
    fn test_with_wuc_replaced_does_not_update_comp_types() {
        // Even though the new wuc looks like it would change which comp_types
        // are produced (e.g. "$" would normally trigger EnvVariable), the
        // method intentionally does not recompute comp_types.
        let res = run_inline(r#"git com█mit"#);
        let original_comp_types = res.comp_types.clone();
        let replaced = res.with_wuc_replaced("$");
        assert_eq!(replaced.comp_types, original_comp_types);
    }

    #[test]
    fn test_with_assignment_basic() {
        let res = run_inline(r#"A=b █ls -la"#);
        assert_eq!(res.context, "ls -la");
        assert_eq!(res.context_until_cursor(), "");
        match res.comp_types.first().unwrap() {
            CompType::FirstWord => {
                assert_eq!(res.word_under_cursor.as_ref(), "ls");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_with_assignment_before_command() {
        let res = run_inline(r#"VAR=valué ABC=qwe   █      ls -la"#);
        assert_eq!(res.context, "");
        assert_eq!(res.context_until_cursor(), "");
    }

    #[test]
    fn test_empty_command() {
        let res = run_inline(r#"█"#);
        assert_eq!(res.context, "");
        assert_eq!(res.context_until_cursor(), "");
        assert_eq!(res.word_under_cursor.as_ref(), "");
    }

    #[test]
    fn test_whitespace_command() {
        let res = run_inline(r#"   █   "#);
        assert_eq!(res.context, "");
        assert_eq!(res.context_until_cursor(), "");
        assert_eq!(res.word_under_cursor.as_ref(), "");
    }

    #[test]
    fn test_with_assignment_at_end() {
        let res = run_inline(r#"VAR=valué ABC=qwe█ ls -la"#);
        assert_eq!(res.context, "ABC=qwe");
        assert_eq!(res.context_until_cursor(), "ABC=qwe");
    }

    #[test]
    fn test_list_of_commands() {
        let res = run_inline(r#"git commit -m "Initial "; ls -la█"#);
        assert_eq!(res.context, "ls -la");
        assert_eq!(res.context_until_cursor(), "ls -la");
    }

    #[test]
    fn test_cursor_at_start_of_word() {
        let res = run_inline(r#"git █commit"#);
        assert_eq!(res.context, "git commit");
        assert_eq!(res.context_until_cursor(), "git ");
        assert_eq!(res.word_under_cursor.as_ref(), "commit");
    }

    #[test]
    fn test_dollar_sign() {
        let res = run_inline(r#"echo $█"#);
        assert_eq!(res.context, "echo $");
        assert_eq!(res.context_until_cursor(), "echo $");
        assert_eq!(res.word_under_cursor.as_ref(), "$");
    }

    #[test]
    fn test_dollar_sign_one_letter() {
        let res = run_inline(r#"echo $A█"#);
        assert_eq!(res.context, "echo $A");
        assert_eq!(res.context_until_cursor(), "echo $A");
        assert_eq!(res.word_under_cursor.as_ref(), "$A");
    }

    #[test]
    fn test_dollar_concatenation() {
        let res = run_inline(r#"echo $A█$B"#);
        assert_eq!(res.context, "echo $A$B");
        assert_eq!(res.context_until_cursor(), "echo $A");
        assert_eq!(res.word_under_cursor.as_ref(), "$A");

        let res = run_inline(r#"echo $A$█B"#);
        assert_eq!(res.context, "echo $A$B");
        assert_eq!(res.context_until_cursor(), "echo $A$");
        assert_eq!(res.word_under_cursor.as_ref(), "$");
    }

    #[test]
    fn test_with_pipeline() {
        let res = run_inline(r#"cat filé.txt | grep "pattern" | sort█"#);
        assert_eq!(res.context, "sort");
        assert_eq!(res.context_until_cursor(), "sort");

        let res2 = run_inline(r#"echo "héllo" && echo "wörld"█"#);
        assert_eq!(res2.context, r#"echo "wörld""#);
        assert_eq!(res2.context_until_cursor(), r#"echo "wörld""#);

        let res3 = run_inline(r#"false || echo "fallback 😅"█"#);
        assert_eq!(res3.context, r#"echo "fallback 😅""#);
        assert_eq!(res3.context_until_cursor(), r#"echo "fallback 😅""#);
    }

    #[test]
    fn test_subshell_in_command() {
        let res = run_inline("echo $(git rev-parse HEAD) résumé█");
        assert_eq!(res.context, "echo $(git rev-parse HEAD) résumé");
        assert_eq!(
            res.context_until_cursor(),
            "echo $(git rev-parse HEAD) résumé"
        );

        assert_eq!(res.context, "echo $(git rev-parse HEAD) résumé");
        assert_eq!(res.word_under_cursor.as_ref(), "résumé");
        assert_eq!(
            res.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyCommandComp {
                    command_word: "echo".to_string()
                },
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_cursor_in_middle_of_subshell_command() {
        let res = run_inline(r#"echo $(git rev-parse HEA█D) café"#);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor(), r#"git rev-parse HEA"#);
    }

    #[test]
    fn test_cursor_at_end_of_subshell_command() {
        let res = run_inline(r#"echo $(git rev-parse HEAD█) 🎉"#);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor(), r#"git rev-parse HEAD"#);
    }

    #[test]
    fn test_cursor_just_outside_of_subshell_command() {
        let res = run_inline(r#"echo $(git rev-parse HEAD)█ 🎉"#);
        assert_eq!(res.context, r#"echo $(git rev-parse HEAD) 🎉"#);
        assert_eq!(res.context_until_cursor(), r#"echo $(git rev-parse HEAD)"#);
    }

    #[test]
    fn test_command_at_end_of_subshell() {
        let res = run_inline(r#"echo $(ls -la)█"#);
        assert_eq!(res.context, "echo $(ls -la)");
        assert_eq!(res.context_until_cursor(), "echo $(ls -la)");
    }

    #[test]
    fn test_param_expansion_in_command() {
        let res = run_inline(r#"echo ${HOME} naïve█"#);
        assert_eq!(res.context, r#"echo ${HOME} naïve"#);
        assert_eq!(res.context_until_cursor(), r#"echo ${HOME} naïve"#);
    }

    #[test]
    fn test_cursor_in_middle_of_param_expansion() {
        let res = run_inline(r#"echo ${HO█ME} asdf"#);
        assert_eq!(res.context, r#"HOME"#);
        assert_eq!(res.context_until_cursor(), "HO");
    }

    #[test]
    fn test_cursor_at_end_of_param_expansion() {
        let res = run_inline(r#"echo ${HOME█} asdf"#);
        assert_eq!(res.context, "HOME");
        assert_eq!(res.context_until_cursor(), "HOME");
    }

    #[test]
    fn test_command_at_end_of_param_expansion() {
        let res = run_inline(r#"ls -la ${PWD}█"#);
        assert_eq!(res.context, "ls -la ${PWD}");
        assert_eq!(res.context_until_cursor(), "ls -la ${PWD}");
    }

    #[test]
    fn test_complex_param_expansion() {
        let res = run_inline(r#"echo ${VAR:-dëfault} test 🎯█"#);
        assert_eq!(res.context, r#"echo ${VAR:-dëfault} test 🎯"#);
        assert_eq!(
            res.context_until_cursor(),
            r#"echo ${VAR:-dëfault} test 🎯"#
        );
    }

    #[test]
    fn test_cursor_inside_complex_param_expansion() {
        let res = run_inline(r#"echo ${VAR:-dëf█ault} tëst"#);
        assert_eq!(res.context, "VAR:-dëfault");
        assert_eq!(res.context_until_cursor(), "VAR:-dëf");
    }

    #[test]
    fn test_backtick_substitution_in_command() {
        let res = run_inline(r#"echo `git rev-parse HEAD` café█"#);
        assert_eq!(res.context, r#"echo `git rev-parse HEAD` café"#);
        assert_eq!(
            res.context_until_cursor(),
            r#"echo `git rev-parse HEAD` café"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_backtick_command() {
        let res = run_inline(r#"echo `git rev-parse█ HEAD` asdf"#);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor(), r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_backtick_command() {
        let res = run_inline(r#"echo `b c█`"#);
        assert_eq!(res.context, "b c");
        assert_eq!(res.context_until_cursor(), "b c");
    }

    #[test]
    fn test_command_at_end_of_backtick() {
        let res = run_inline(r#"echo `ls -la`█ qwe"#);
        assert_eq!(res.context, "echo `ls -la` qwe");
        assert_eq!(res.context_until_cursor(), "echo `ls -la`");
    }

    #[test]
    fn test_nested_backticks_in_command() {
        let res = run_inline(r#"echo `echo \`date\`` tëst 🎯█"#);
        assert_eq!(res.context, r#"echo `echo \`date\`` tëst 🎯"#);
        assert_eq!(
            res.context_until_cursor(),
            r#"echo `echo \`date\`` tëst 🎯"#
        );
    }

    #[test]
    fn test_cursor_in_backtick_with_pipe() {
        let res = run_inline(r#"echo `ls | grep█ test` done"#);
        assert_eq!(res.context, r#"grep test"#);
        assert_eq!(res.context_until_cursor(), r#"grep"#);
    }

    #[test]
    fn test_arith_subst_in_command() {
        let res = run_inline(r#"echo $((5 + 3)) rësult 📊█"#);
        assert_eq!(res.context, r#"echo $((5 + 3)) rësult 📊"#);
        assert_eq!(res.context_until_cursor(), r#"echo $((5 + 3)) rësult 📊"#);
    }

    #[test]
    fn test_cursor_near_end_of_arith_subst() {
        let res = run_inline(r#"echo $((5 + 3█)) result"#);
        assert_eq!(res.context, "5 + 3");
        assert_eq!(res.context_until_cursor(), "5 + 3");
    }

    #[test]
    fn test_cursor_in_middle_of_arith_subst_end() {
        let res = run_inline(r#"echo $((5 + 3)█) result"#);
        assert_eq!(res.context, "echo $((5 + 3)) result");
        assert_eq!(res.context_until_cursor(), "echo $((5 + 3)");
    }

    #[test]
    fn test_cursor_at_end_of_arith_subst() {
        let res = run_inline(r#"echo $((10 * 2))█ bar"#);
        assert_eq!(res.context, "echo $((10 * 2)) bar");
        assert_eq!(res.context_until_cursor(), "echo $((10 * 2))");
    }

    #[test]
    fn test_command_at_mid_end_of_arith_subst() {
        let res = run_inline(r#"result=$((100 / 5)█)"#);
        assert_eq!(res.context, r#"result=$((100 / 5))"#);
        assert_eq!(res.context_until_cursor(), r#"result=$((100 / 5)"#);
    }

    #[test]
    fn test_command_at_end_end_of_arith_subst() {
        let res = run_inline(r#"result=$((100 / 5))█"#);
        assert_eq!(res.context, r#"result=$((100 / 5))"#);
        assert_eq!(res.context_until_cursor(), r#"result=$((100 / 5))"#);
    }

    #[test]
    fn test_complex_arith_with_variables() {
        let res = run_inline(r#"echo $(($VAR + 10)) test█"#);
        assert_eq!(res.context, r#"echo $(($VAR + 10)) test"#);
        assert_eq!(res.context_until_cursor(), r#"echo $(($VAR + 10)) test"#);
    }

    #[test]
    fn test_cursor_inside_complex_arith() {
        let res = run_inline(r#"val=$((VAR * 2█ + 5))"#);
        assert_eq!(res.context, "VAR * 2 + 5");
        assert_eq!(res.context_until_cursor(), "VAR * 2");
    }

    #[test]
    fn test_nested_arith_operations() {
        let res = run_inline(r#"echo $(( $(( 5 +█ 3 )) * 2 )) ënd ✅"#);
        assert_eq!(res.context, r#"5 + 3"#);
        assert_eq!(res.context_until_cursor(), r#"5 +"#);
    }

    #[test]
    fn test_proc_subst_in_command() {
        let res = run_inline(r#"diff <(ls /tmp) <(ls /var) résult 🔍█"#);
        assert_eq!(res.context, r#"diff <(ls /tmp) <(ls /var) résult 🔍"#);
        assert_eq!(
            res.context_until_cursor(),
            r#"diff <(ls /tmp) <(ls /var) résult 🔍"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_in() {
        let res = run_inline(r#"diff <(ls /t█mp) <(ls /var) done"#);
        assert_eq!(res.context, r#"ls /tmp"#);
        assert_eq!(res.context_until_cursor(), r#"ls /t"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_in() {
        let res = run_inline(r#"diff <(ls /tmp█) <(ls /var) done"#);
        assert_eq!(res.context, r#"ls /tmp"#);
        assert_eq!(res.context_until_cursor(), r#"ls /tmp"#);
    }

    #[test]
    fn test_command_at_end_of_proc_subst_in() {
        let res = run_inline(r#"cat <(echo test)█"#);
        assert_eq!(res.context, r#"cat <(echo test)"#);
        assert_eq!(res.context_until_cursor(), r#"cat <(echo test)"#);
    }

    #[test]
    fn test_proc_subst_out_in_command() {
        let res = run_inline(r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉█"#);
        assert_eq!(
            res.context,
            r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#
        );
        assert_eq!(
            res.context_until_cursor(),
            r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#
        );
    }

    #[test]
    fn test_cursor_before_proc_subst_in() {
        // The original failing case: cursor is in the word before a process substitution.
        let res = run_inline(r#"x█ <( echo too )"#);
        assert_eq!(res.context, "x <( echo too )");
        assert_eq!(res.context_until_cursor(), "x");
        assert_eq!(res.word_under_cursor.as_ref(), "x");
        assert_eq!(
            res.comp_types,
            vec![
                CompType::FirstWord,
                CompType::FuzzyFirstWord,
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_cursor_before_proc_subst_in_multi() {
        // Cursor before two consecutive process substitutions.
        let res = run_inline(r#"diff█ <(cat a) <(cat b)"#);
        assert_eq!(res.context, "diff <(cat a) <(cat b)");
        assert_eq!(res.context_until_cursor(), "diff");
        assert_eq!(res.word_under_cursor.as_ref(), "diff");
        assert_eq!(
            res.comp_types,
            vec![
                CompType::FirstWord,
                CompType::FuzzyFirstWord,
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_cursor_before_cmd_subst() {
        // Cursor before a command substitution $(...).
        let res = run_inline(r#"echo█ $(git rev-parse HEAD)"#);
        assert_eq!(res.context, "echo $(git rev-parse HEAD)");
        assert_eq!(res.context_until_cursor(), "echo");
        assert_eq!(res.word_under_cursor.as_ref(), "echo");
        assert_eq!(
            res.comp_types,
            vec![
                CompType::FirstWord,
                CompType::FuzzyFirstWord,
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_cursor_in_word_before_proc_subst_out() {
        // Cursor in the command word before a process substitution out >(...)
        let res = run_inline(r#"tee█ >(gzip > file.gz)"#);
        assert_eq!(res.context, "tee >(gzip > file.gz)");
        assert_eq!(res.context_until_cursor(), "tee");
        assert_eq!(res.word_under_cursor.as_ref(), "tee");
        assert_eq!(
            res.comp_types,
            vec![
                CompType::FirstWord,
                CompType::FuzzyFirstWord,
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_cursor_in_arg_before_proc_subst() {
        // Cursor is in an argument (not first word) that precedes a process substitution.
        let res = run_inline(r#"diff file█ <(echo too)"#);
        assert_eq!(res.context, "diff file <(echo too)");
        assert_eq!(res.context_until_cursor(), "diff file");
        assert_eq!(res.word_under_cursor.as_ref(), "file");
        assert_eq!(
            res.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "diff".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyCommandComp {
                    command_word: "diff".to_string()
                },
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_out() {
        let res = run_inline(r#"tee >(gzip > fi█le.gz) test"#);
        assert_eq!(res.context, r#"gzip > file.gz"#);
        assert_eq!(res.context_until_cursor(), r#"gzip > fi"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_out() {
        let res = run_inline(r#"tee >(cat█) done"#);
        assert_eq!(res.context, r#"cat"#);
        assert_eq!(res.context_until_cursor(), r#"cat"#);
    }

    #[test]
    fn test_mixed_proc_subst_in_and_out() {
        let res = run_inline(r#"cmd <(input cmd) >(output cmd) final█"#);
        assert_eq!(res.context, r#"cmd <(input cmd) >(output cmd) final"#);
        assert_eq!(
            res.context_until_cursor(),
            r#"cmd <(input cmd) >(output cmd) final"#
        );
    }

    #[test]
    // #[ignore] // Need to think more on what the expected behavior is here
    fn test_double_bracket_condition() {
        let res = run_inline(r#"if [[ -f file.txt ]]; then echo found; fi█"#);
        assert_eq!(res.context, "if [[ -f file.txt ]]; then echo found; fi");
        assert_eq!(
            res.context_until_cursor(),
            "if [[ -f file.txt ]]; then echo found; fi"
        );
        assert_eq!(res.word_under_cursor.as_ref(), "fi");
    }

    #[test]
    fn test_cursor_inside_double_bracket() {
        let res = run_inline(r#"[[ -f filé█.txt ]] && echo yës"#);
        assert_eq!(res.context, "-f filé.txt");
        assert_eq!(res.context_until_cursor(), "-f filé");
    }

    #[test]
    fn test_double_bracket_with_string_comparison() {
        let res = run_inline(r#"[[ "$var" == "café" ]] && echo match 🎯█"#);
        assert_eq!(res.context, r#"echo match 🎯"#);
        assert_eq!(res.context_until_cursor(), r#"echo match 🎯"#);
    }

    #[test]
    fn test_double_bracket_with_pattern() {
        let res = run_inline(r#"[[ $file == *.txt ]█] || echo "not a text file""#);
        assert_eq!(res.context, "[[ $file == *.txt ]]");
        assert_eq!(res.context_until_cursor(), "[[ $file == *.txt ]");
    }

    #[test]
    fn test_start_with_subshell() {
        let res = run_inline(r#"$(echo test)█"#);
        assert_eq!(res.context, "$(echo test)");
        assert_eq!(res.context_until_cursor(), "$(echo test)");
    }

    #[test]
    fn test_double_bracket_with_regex() {
        let res = run_inline(r#"[[ $email =~ ^[a-z]+@[a-z]+$ ]]█"#);
        assert_eq!(res.context, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
        assert_eq!(
            res.context_until_cursor(),
            "[[ $email =~ ^[a-z]+@[a-z]+$ ]]"
        );
    }

    #[test]
    fn test_double_bracket_logical_operators() {
        let res = run_inline(r#"[[ -f file.txt && -r file.txt ]] && cat file.txt█"#);
        assert_eq!(res.context, "cat file.txt");
        assert_eq!(res.context_until_cursor(), "cat file.txt");
    }

    #[test]
    fn test_cursor_before_double_bracket() {
        let res = run_inline(r#"if [[ -d /path/caf█é ]]; then ls; fi"#);
        assert_eq!(res.context, "-d /path/café");
        assert_eq!(res.context_until_cursor(), "-d /path/caf");
    }

    #[test]
    fn test_double_bracket_with_emoji() {
        let res = run_inline(r#"[[ "$msg" == "✅ done" ]] && echo success█"#);
        assert_eq!(res.context, "echo success");
        assert_eq!(res.context_until_cursor(), "echo success");
    }

    // Tests for CompletionContext with various cursor positions and non-ASCII characters

    #[test]
    fn test_completion_context_cursor_at_start_of_line() {
        // Cursor at position 0 (start of line)
        let ctx = run_inline("█café --option 🎯");
        match ctx.comp_types.first().unwrap() {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor.as_ref(), "café");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_first_word() {
        // Cursor in the middle of first word with non-ASCII
        let ctx = run_inline("caf█é --option 🎯");
        match ctx.comp_types.first().unwrap() {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor.as_ref(), "café");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_after_first_word_emoji() {
        // Cursor after first word that contains emoji
        let ctx = run_inline("🚀rock█et --verbose naïve");
        dbg!(&ctx);
        match ctx.comp_types.first().unwrap() {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor.as_ref(), "🚀rocket");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_at_end_of_line() {
        // Cursor at end of line with non-ASCII
        let ctx = run_inline("echo 'Tëst message' résumé 📄█");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "echo 'Tëst message' résumé 📄");
                assert_eq!(command_word, "echo");
                assert_eq!(ctx.word_under_cursor.as_ref(), "📄");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_middle_word_with_unicode() {
        // Cursor in middle of word with unicode characters
        let ctx = run_inline("ls --sïze caf█é 日本語");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "ls --sïze café 日本語");
                assert_eq!(command_word, "ls");
                assert_eq!(ctx.word_under_cursor.as_ref(), "café");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_at_start_chinese_chars() {
        // Cursor at start with Chinese characters
        let ctx = run_inline("█文件 --option värde");
        match ctx.comp_types.first().unwrap() {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor.as_ref(), "文件");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_middle_chinese() {
        // Cursor in middle of Chinese word
        let ctx = run_inline("git 提█交 --mëssage 'hëllo'");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "git 提交 --mëssage 'hëllo'");
                assert_eq!(command_word, "git");
                assert_eq!(ctx.word_under_cursor.as_ref(), "提交");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_end_arabic_text() {
        // Cursor at end with Arabic text
        let ctx = run_inline("cat مرحبا --öption 🔥█");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "cat مرحبا --öption 🔥");
                assert_eq!(command_word, "cat");
                assert_eq!(ctx.word_under_cursor.as_ref(), "🔥");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_middle_cyrillic() {
        // Cursor in middle of Cyrillic word
        let ctx = run_inline("ls фай█л --süze привет 🎯");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "ls файл --süze привет 🎯");
                assert_eq!(command_word, "ls");
                assert_eq!(ctx.word_under_cursor.as_ref(), "файл");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_blank_space_mixed_scripts() {
        // Cursor on blank space with mixed scripts
        let ctx = run_inline("grep 'pättërn' █файл.txt 日本語 🚀");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "grep 'pättërn' файл.txt 日本語 🚀");
                assert_eq!(command_word, "grep");
                assert_eq!(ctx.word_under_cursor.as_ref(), "файл.txt");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_start_emoji_only() {
        // Cursor at start of emoji-only command
        let ctx = run_inline("█🎉 🎊 🎈 --flâg");
        match ctx.comp_types.first().unwrap() {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor.as_ref(), "🎉");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_end_accented_characters() {
        // Cursor at end with heavily accented text
        let ctx = run_inline("find . -näme 'fîlé' -type f 🔍█");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "find . -näme 'fîlé' -type f 🔍");
                assert_eq!(command_word, "find");
                assert_eq!(ctx.word_under_cursor.as_ref(), "🔍");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_space_between_multibyte() {
        // Cursor on space between multibyte characters
        let ctx = run_inline("écho 'mëssagé' █文件 🎨");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "écho 'mëssagé' 文件 🎨");
                assert_eq!(command_word, "écho");
                assert_eq!(ctx.word_under_cursor.as_ref(), "文件");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_middle_thai_text() {
        // Cursor in middle of Thai text
        let ctx = run_inline("cat ไฟ█ล์ --öption วันนี้ 🌟");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "cat ไฟล์ --öption วันนี้ 🌟");
                assert_eq!(command_word, "cat");
                assert_eq!(ctx.word_under_cursor.as_ref(), "ไฟล์");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_under_cursor_with_word_after() {
        // This is the bug: when cursor is at END of word AND there's a word after,
        // word_under_cursor should be the current word, not ""
        // Example: "cd fo[cursor] bar" - word_under_cursor should be "fo", not ""
        let ctx = run_inline("cd fo█ bar");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "fo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_under_cursor_in_middle_with_word_after() {
        // Cursor in the middle of "foo" when "bar" follows
        let ctx = run_inline("cd f█oo bar");

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "foo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_double_quote_1() {
        let ctx = run_inline(r#"cd "foo█"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "\"foo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]

    fn test_word_with_double_quote_2() {
        let ctx = run_inline(r#"cd "foo   asdf█"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "\"foo   asdf");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_double_quote_3() {
        let ctx = run_inline(r#"cd "foo █"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "\"foo ");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_double_quote_4() {
        let ctx = run_inline(r#"echo && cd "foo █"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "\"foo ");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_single_quote_1() {
        let ctx = run_inline(r#"cd 'foo█"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "'foo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_single_quote_2() {
        let ctx = run_inline(r#"cd 'foo   asdf█"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "'foo   asdf");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_single_quote_3() {
        let ctx = run_inline(r#"echo && cd 'foo   asdf█"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "'foo   asdf");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_backslash_1() {
        let ctx = run_inline(r#"echo && cd foo\█"#);

        match ctx.comp_types.first().unwrap() {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor.as_ref(), "foo\\");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_backslash_2() {
        let ctx = run_inline(r#"cd foo\ █"#);

        assert_eq!(ctx.word_under_cursor.as_ref(), "foo\\ ");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "cd".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyCommandComp {
                    command_word: "cd".to_string()
                },
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_past_newline() {
        let ctx = run_inline("echo \"\n█");

        assert_eq!(ctx.word_under_cursor.as_ref(), "");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyCommandComp {
                    command_word: "echo".to_string()
                },
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_env_var_completion() {
        let ctx = run_inline("echo $HOM█");

        assert_eq!(ctx.word_under_cursor.as_ref(), "$HOM");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::EnvVariable
            ]
        );
    }

    #[test]
    fn test_env_var_completion_in_double_quotes() {
        let ctx = run_inline("echo \"$HOM█\"");

        assert_eq!(ctx.word_under_cursor.as_ref(), "$HOM");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::EnvVariable
            ]
        );
    }

    #[test]
    fn test_env_var_path_completion_in_double_quotes() {
        let ctx = run_inline("echo \"$HOME/abc█\"");

        assert_eq!(ctx.word_under_cursor.as_ref(), "\"$HOME/abc");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_second_env_var_completion_in_double_quotes() {
        let ctx = run_inline("echo \"$FOO$HOM█\"");

        assert_eq!(ctx.word_under_cursor.as_ref(), "$HOM");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::EnvVariable
            ]
        );
    }

    #[test]
    fn test_env_var_completion_in_double_quotes_trailingspace() {
        let ctx = run_inline("echo \"asdf $HOM█ \"");

        assert_eq!(ctx.word_under_cursor.as_ref(), "$HOM");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::EnvVariable
            ]
        );
    }

    #[test]
    fn test_start_with_env_var() {
        let ctx = run_inline("$HOME/█");

        assert_eq!(ctx.word_under_cursor.as_ref(), "$HOME/");

        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::FirstWord,
                CompType::FuzzyFirstWord,
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_env_var_at_start_but_not_end() {
        let ctx = run_inline(r#"ll $HOME/projects/flyline/qwe\ asd/\$█"#);

        assert_eq!(
            ctx.word_under_cursor.as_ref(),
            r#"$HOME/projects/flyline/qwe\ asd/\$"#
        );

        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "ll".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_completion_context_uses_glob_expansion_for_patterns() {
        let ctx = run_inline(r"echo ./{foo,bar}.txt█");

        assert_eq!(ctx.word_under_cursor.as_ref(), "./{foo,bar}.txt");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::GlobExpansion
            ]
        );

        let ctx = run_inline(r"echo ./foo*█");

        assert_eq!(ctx.word_under_cursor.as_ref(), "./foo*");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::GlobExpansion
            ]
        );
    }

    #[test]
    fn test_completion_context_uses_filename_expansion_for_literals() {
        let ctx = run_inline(r"echo ./foo\*█");

        assert_eq!(ctx.word_under_cursor.as_ref(), r"./foo\*");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );

        let ctx = run_inline(r"echo ./foo{bar}.txt█");

        assert_eq!(ctx.word_under_cursor.as_ref(), "./foo{bar}.txt");
        assert_eq!(
            ctx.comp_types,
            vec![
                CompType::CommandComp {
                    command_word: "echo".to_string()
                },
                CompType::FilenameExpansion,
                CompType::FuzzyFilenameExpansion
            ]
        );
    }

    #[test]
    fn test_brace_expansion() {
        let ctx = run_inline(r"echo {foo,bar}*█");

        assert_eq!(ctx.word_under_cursor.as_ref(), r"{foo,bar}*");
    }
}
