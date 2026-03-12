use flash::lexer::{Lexer, Position, Token, TokenKind};
use itertools::Itertools;
use std::collections::VecDeque;
use std::ops::{Range, RangeInclusive};
use log::debug;

/// Split a Word token that contains embedded newlines into separate tokens.
/// Returns a list of `(token, relative_byte_offset)` pairs, where the offset
/// is relative to the byte_start of the original token.
fn split_token_into_lines(token: Token) -> Vec<(Token, usize)> {
    match &token.kind {
        TokenKind::Word(s) => {
            let mut tokens = vec![];

            let mut row = token.position.line;
            let mut col = token.position.column;

            for (_, chunk) in &s
                .char_indices()
                .chunk_by(|(idx, c)| if *c == '\n' { *idx as i32 } else { -1 })
            {
                let chunk: Vec<(usize, char)> = chunk.collect();
                let chunk_str: String = chunk.iter().map(|(_, c)| *c).collect();
                let chunk_byte_start = chunk.first().map(|(idx, _)| *idx).unwrap_or(0);

                match chunk_str.as_str() {
                    "\n" => {
                        tokens.push((
                            Token {
                                kind: TokenKind::Newline,
                                value: chunk_str,
                                position: Position::new(row, col),
                            },
                            chunk_byte_start,
                        ));

                        row += 1;
                        col = 1; // flash lexer uses 1 based column numbers
                    }
                    _ => {
                        tokens.push((
                            Token {
                                kind: TokenKind::Word(chunk_str.clone()),
                                value: chunk_str.clone(),
                                position: Position::new(row, col),
                            },
                            chunk_byte_start,
                        ));

                        // flash lexer uses char indices for col counts instead of grapheme width.
                        col += chunk_str.chars().count();
                    }
                }
            }
            tokens
        }
        _ => vec![(token, 0)],
    }
}

#[test]
fn test_split_token_into_lines() {
    let token = Token {
        kind: TokenKind::Word("hello\nworld".to_string()),
        value: "hello\nworld".to_string(),
        position: Position::new(1, 1),
    };

    let pairs = split_token_into_lines(token);
    assert_eq!(pairs.len(), 3);

    let (tok0, rel0) = &pairs[0];
    assert_eq!(tok0.kind, TokenKind::Word("hello".to_string()));
    assert_eq!(tok0.position.line, 1);
    assert_eq!(tok0.position.column, 1);
    assert_eq!(*rel0, 0);

    let (tok1, rel1) = &pairs[1];
    assert_eq!(tok1.kind, TokenKind::Newline);
    assert_eq!(tok1.position.line, 1);
    assert_eq!(tok1.position.column, 6);
    assert_eq!(*rel1, 5);

    let (tok2, rel2) = &pairs[2];
    assert_eq!(tok2.kind, TokenKind::Word("world".to_string()));
    assert_eq!(tok2.position.line, 2);
    assert_eq!(tok2.position.column, 1);
    assert_eq!(*rel2, 6);

    let pairs2 = split_token_into_lines(pairs[0].0.clone());
    assert_eq!(pairs2.len(), 1);
    assert_eq!(pairs2[0].0.kind, TokenKind::Word("hello".to_string()));
}

/// Collect all tokens from `input`, inserting synthetic whitespace tokens for
/// the spaces/tabs that the flash lexer skips.  Also combines `HereDoc` /
/// `HereDocDash` tokens with the immediately-following delimiter word so that
/// the value (e.g. `"<<EOF"`) matches the old flash behaviour.
///
/// Returns a `Vec<(Token, usize, usize)>` where the second element is the
/// byte start in `input` and the third element is the number of raw bytes
/// from `input` that this token corresponds to (which may differ from
/// `token.value.len()` when the flash lexer applies backslash escaping).
pub fn collect_tokens_include_whitespace(input: &str) -> Vec<(Token, usize, usize)> {
    let mut lexer = Lexer::new(input);
    let mut result: Vec<(Token, usize, usize)> = Vec::new();

    // Pre-compute mapping from char index → byte offset in `input`.
    let char_to_byte: Vec<usize> = input.char_indices().map(|(b, _)| b).collect();
    let input_len = input.len();
    let char_pos_to_byte = |char_pos: usize| -> usize {
        char_to_byte.get(char_pos).copied().unwrap_or(input_len)
    };

    let input_bytes = input.as_bytes();
    // Byte position right after the last emitted (non-whitespace) token.
    let mut prev_end_byte: usize = 0;

    loop {
        // Record where the lexer is before next_token() (which starts by
        // calling skip_whitespace internally).
        let pre_call_char_pos = lexer.position;
        let token = lexer.next_token();
        let post_call_char_pos = lexer.position;

        if matches!(token.kind, TokenKind::EOF) {
            break;
        }

        // ── Find whitespace between prev_end_byte and the token start ───────
        // The token starts after any spaces/tabs the lexer skipped.
        let pre_call_byte = char_pos_to_byte(pre_call_char_pos);
        let post_call_byte = char_pos_to_byte(post_call_char_pos);

        let mut token_start_byte = pre_call_byte;
        while token_start_byte < post_call_byte
            && (input_bytes[token_start_byte] == b' '
                || input_bytes[token_start_byte] == b'\t')
        {
            token_start_byte += 1;
        }

        // Emit a synthetic whitespace token for the gap, if any.
        if token_start_byte > prev_end_byte {
            let ws_str = &input[prev_end_byte..token_start_byte];
            let ws_len = ws_str.len();
            result.push((
                Token {
                    kind: TokenKind::Word(ws_str.to_string()),
                    value: ws_str.to_string(),
                    position: Position::new(0, 0),
                },
                prev_end_byte,
                ws_len,
            ));
        }

        // ── Guard against empty tokens (e.g. unclosed quote at EOF) ─────────
        if token.value.is_empty() {
            break;
        }

        // ── HereDoc / HereDocDash: combine with the following delimiter ──────
        if matches!(token.kind, TokenKind::HereDoc | TokenKind::HereDocDash) {
            let op = token.value.clone(); // "<<" or "<<-"

            // Consume the delimiter token.
            let delim_token = lexer.next_token();
            let delim_post_char = lexer.position;
            let delim = delim_token.value.clone();
            let combined_end_byte = char_pos_to_byte(delim_post_char);

            let combined_value = format!("{}{}", op, delim);
            let combined_raw_len = combined_end_byte - token_start_byte;
            prev_end_byte = combined_end_byte;
            result.push((
                Token {
                    kind: token.kind,
                    value: combined_value,
                    position: token.position,
                },
                token_start_byte,
                combined_raw_len,
            ));
            continue;
        }

        // ── Regular token ────────────────────────────────────────────────────
        let raw_token_len = post_call_byte - token_start_byte;
        prev_end_byte = post_call_byte;

        let sub_tokens = split_token_into_lines(token);
        let sub_count = sub_tokens.len();
        for (i, (sub_token, rel)) in sub_tokens.into_iter().enumerate() {
            // For the last sub-token, use the remaining raw bytes; for others
            // use the sub-token value length (splitting only happens inside
            // quoted strings where value len == raw byte len).
            let sub_raw_len = if i + 1 == sub_count {
                raw_token_len.saturating_sub(rel)
            } else {
                sub_token.value.len()
            };
            result.push((sub_token, token_start_byte + rel, sub_raw_len));
        }
    }

    // Emit any trailing whitespace that the lexer skipped after the last token.
    if prev_end_byte < input_len {
        let remaining = &input[prev_end_byte..];
        let ws_len = remaining.len()
            - remaining
                .trim_start_matches(|c: char| c == ' ' || c == '\t')
                .len();
        if ws_len > 0 {
            let ws_str = &input[prev_end_byte..prev_end_byte + ws_len];
            result.push((
                Token {
                    kind: TokenKind::Word(ws_str.to_string()),
                    value: ws_str.to_string(),
                    position: Position::new(0, 0),
                },
                prev_end_byte,
                ws_len,
            ));
        }
    }

    result
}

pub trait ToInclusiveRange {
    fn to_inclusive(&self) -> RangeInclusive<usize>;
}

impl ToInclusiveRange for Range<usize> {
    fn to_inclusive(&self) -> RangeInclusive<usize> {
        self.start..=self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenAnnotation {
    None,
    IsPartOfQuotedString,
    IsOpening(Option<usize>), // index of the closing token in the tokens vector
    IsClosing(usize),         // index of the opening token in the tokens vector
    IsCommandWord, // the first word of a command. e.g.`git commit -m "message"` -> `git` would be annotated with this
}

#[derive(Debug, Clone)]
pub struct AnnotatedToken {
    pub token: Token,
    pub annotation: TokenAnnotation,
    /// Byte offset of the start of this token within the input string.
    pub byte_start: usize,
    /// Number of raw bytes in the original input that this token corresponds
    /// to.  May differ from `token.value.len()` when the flash lexer applies
    /// backslash escaping (e.g. `\ ` is two raw bytes but produces a value of
    /// one space character).
    raw_byte_len: usize,
}

impl AnnotatedToken {
    pub fn new(token: Token, byte_start: usize, raw_byte_len: usize) -> Self {
        Self {
            token,
            annotation: TokenAnnotation::None,
            byte_start,
            raw_byte_len,
        }
    }

    pub fn byte_range(&self) -> Range<usize> {
        self.byte_start..self.byte_start + self.raw_byte_len
    }

    /// Returns true if this token represents synthesized whitespace (spaces /
    /// tabs between other tokens).
    pub fn is_whitespace(&self) -> bool {
        matches!(self.token.kind, TokenKind::Word(_))
            && !self.token.value.is_empty()
            && self.token.value.chars().all(|c| c == ' ' || c == '\t')
    }

    /// Returns true if this token is an actual word (not synthetic whitespace).
    pub fn is_word(&self) -> bool {
        matches!(self.token.kind, TokenKind::Word(_)) && !self.is_whitespace()
    }
}

#[derive(Debug)]
pub struct DParser {
    tokens: Vec<AnnotatedToken>,

    current_command_range: Option<RangeInclusive<usize>>,
}

impl DParser {
    pub fn new(tokens: Vec<(Token, usize, usize)>) -> Self {
        Self {
            tokens: tokens
                .into_iter()
                .map(|(t, byte_start, raw_byte_len)| AnnotatedToken::new(t, byte_start, raw_byte_len))
                .collect(),

            current_command_range: None,
        }
    }

    pub fn from(input: &str) -> Self {
        let tokens = collect_tokens_include_whitespace(input);
        Self::new(tokens)
    }

    #[allow(dead_code)]
    pub fn tokens(&self) -> &[AnnotatedToken] {
        &self.tokens
    }

    fn nested_opening_satisfied(
        token: &Token,
        current_nesting: Option<&TokenKind>,
        is_command_extraction: bool,
    ) -> bool {
        match token.kind {
            TokenKind::Quote | TokenKind::SingleQuote if is_command_extraction => {
                return false;
            }
            TokenKind::Backtick | TokenKind::Quote | TokenKind::SingleQuote => {
                if Some(&token.kind) == current_nesting {
                    // backtick or quote is acting as closer
                    return false;
                } else {
                    return true;
                }
            }
            _ => true,
        }
    }

    fn nested_closing_satisfied(
        token: &Token,
        current_nesting: Option<&TokenKind>,
        next_token: Option<&&Token>,
    ) -> bool {
        let current_nesting = match current_nesting {
            Some(v) => v,
            None => return false,
        };
        match (&token.kind, current_nesting) {
            (TokenKind::RParen, TokenKind::LParen) => true,
            (TokenKind::RParen, TokenKind::CmdSubst) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstIn) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstOut) => true,
            (TokenKind::RParen, TokenKind::ExtGlob(_)) => true,
            (TokenKind::RBrace, TokenKind::ParamExpansion) => true,
            (TokenKind::RBrace, TokenKind::LBrace) => true,
            (TokenKind::RParen, TokenKind::ArithSubst) // it needs two ))
                if next_token.map_or(false, |t| t.kind == TokenKind::RParen) =>
            {
                true
            }
            (TokenKind::Backtick, TokenKind::Backtick) => true,
            (TokenKind::DoubleRBracket, TokenKind::DoubleLBracket) => true,
            (TokenKind::Quote, TokenKind::Quote) => true,
            (TokenKind::SingleQuote, TokenKind::SingleQuote) => true,
            (TokenKind::Esac, TokenKind::Case) => true,
            (TokenKind::Done, TokenKind::For) => true,
            (TokenKind::Done, TokenKind::While) => true,
            (TokenKind::Done, TokenKind::Until) => true,
            (TokenKind::Fi, TokenKind::If) => true,
            _ => false,
        }
    }

    pub fn walk_to_end(&mut self) {
        self.walk(None);
    }

    pub fn walk_to_cursor(&mut self, cursor_byte_pos: usize) {
        self.walk(Some(cursor_byte_pos));
    }

    fn walk(&mut self, cursor_byte_pos: Option<usize>) {
        // Walk through the tokens until we reach the end or the cursor position, updating nestings and heredocs along the way

        // echo $(( grep 1 + 2      # command is grep
        // echo $(( grep 1 + 2 )    # command is grep
        // echo $(( grep 1 + 2 ))   # command is echo, since the cursor is after the closing ))

        // The index of the last opening nesting token and its kind
        let mut nestings: Vec<(usize, TokenKind)> = Vec::new();
        // Heredocs are tracked separately since they close based on FIFO order, not LIFO like the other nestings
        let mut heredocs: VecDeque<(usize, String)> = VecDeque::new();

        let mut annotated_tokens = self.tokens.iter_mut().enumerate().peekable();
        let mut stop_parsing_at_command_boundary = false;

        let mut command_start_stack = Vec::new();

        let mut previous_token: Option<AnnotatedToken> = None;

        loop {
            let (mut idx, mut annotated_token) = match annotated_tokens.next() {
                Some(t) => t,
                None => break,
            };
            let mut token = &annotated_token.token;

            let word_is_part_of_assignment = if annotated_token.is_word() {
                previous_token.as_ref().map_or(false, |token| {
                    matches!(token.token.kind, TokenKind::Assignment)
                })
            } else {
                false
            };

            let token_inclusively_contains_cursor = cursor_byte_pos.map_or(false, |pos| {
                annotated_token.byte_range().to_inclusive().contains(&pos)
            });

            let token_strictly_contains_cursor =
                cursor_byte_pos.map_or(false, |pos| annotated_token.byte_range().contains(&pos));

            if token_strictly_contains_cursor {
                stop_parsing_at_command_boundary = true;
            }

            match &token.kind {
                TokenKind::LBrace
                | TokenKind::Quote
                | TokenKind::SingleQuote
                | TokenKind::DoubleLBracket
                | TokenKind::Backtick
                | TokenKind::CmdSubst
                | TokenKind::ArithSubst
                | TokenKind::ArithCommand
                | TokenKind::ParamExpansion
                | TokenKind::ProcessSubstIn
                | TokenKind::ProcessSubstOut
                | TokenKind::If
                | TokenKind::Case
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Until
                    if Self::nested_opening_satisfied(
                        &token,
                        nestings.last().map(|(_, k)| k),
                        cursor_byte_pos.is_some(),
                    ) =>
                {
                    annotated_token.annotation = TokenAnnotation::IsOpening(None);

                    if self.current_command_range.is_none() {
                        self.current_command_range = Some(idx..=idx);
                    }
                    nestings.push((idx, token.kind.clone()));
                    command_start_stack.push(self.current_command_range.clone());
                    self.current_command_range = None; // set for next word after this
                }
                TokenKind::HereDoc | TokenKind::HereDocDash => {
                    annotated_token.annotation = TokenAnnotation::IsOpening(None);

                    // Extract the delimiter from the combined value (e.g. "<<EOF" → "EOF").
                    let op_len = if matches!(token.kind, TokenKind::HereDocDash) { 3 } else { 2 };
                    let delim = token.value[op_len..].to_string();
                    heredocs.push_back((idx, delim));
                }
                TokenKind::RParen
                | TokenKind::Quote
                | TokenKind::SingleQuote
                | TokenKind::RBrace
                | TokenKind::Backtick
                | TokenKind::DoubleRBracket
                | TokenKind::Esac
                | TokenKind::Done
                | TokenKind::Fi
                    if Self::nested_closing_satisfied(
                        &token,
                        nestings.last().map(|(_, k)| k),
                        annotated_tokens.peek().map(|(_, t)| &t.token).as_ref(),
                    ) =>
                {
                    let (opening_idx, kind) = nestings.pop().unwrap();
                    annotated_token.annotation = TokenAnnotation::IsClosing(opening_idx);
                    if kind == TokenKind::ArithSubst {
                        assert!(
                            annotated_tokens.peek().unwrap().1.token.kind == TokenKind::RParen,
                            "expected two RParen tokens"
                        );
                        (idx, annotated_token) = annotated_tokens.next().unwrap(); // consume the extra RParen
                        token = &annotated_token.token;
                    }

                    if token.kind == TokenKind::DoubleRBracket && token_strictly_contains_cursor {
                        if let Some(prev_command_range) = command_start_stack.pop() {
                            self.current_command_range = prev_command_range;
                            if let Some(range) = &mut self.current_command_range {
                                *range = *range.start()..=idx;
                            }
                        }
                        break;
                    }

                    if stop_parsing_at_command_boundary {
                        debug!("Stopping parsing at command boundary");
                        break;
                    }

                    if let Some(prev_command_range) = command_start_stack.pop() {
                        self.current_command_range = prev_command_range;
                        if let Some(range) = &mut self.current_command_range {
                            *range = *range.start()..=idx;
                        }
                    }
                }
                TokenKind::Word(_) if word_is_part_of_assignment => {
                    if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }

                    if stop_parsing_at_command_boundary || token_inclusively_contains_cursor {
                        break;
                    }
                    self.current_command_range = None;
                }
                TokenKind::Word(word)
                    if heredocs.front().is_some_and(|(_, delim)| delim == word) =>
                {
                    let (opening_idx, _) = heredocs.pop_front().unwrap();
                    annotated_token.annotation = TokenAnnotation::IsClosing(opening_idx);
                }

                TokenKind::And
                | TokenKind::Or
                | TokenKind::Pipe
                | TokenKind::Semicolon
                | TokenKind::Background
                | TokenKind::DoubleSemicolon => {
                    if stop_parsing_at_command_boundary {
                        break;
                    }
                    self.current_command_range = None;
                }
                _ if annotated_token.is_whitespace() => {
                    if token_inclusively_contains_cursor {
                        if let Some(range) = &mut self.current_command_range {
                            *range = *range.start()..=idx;
                        }
                    }

                    if token_strictly_contains_cursor
                        && stop_parsing_at_command_boundary
                        && self.current_command_range.is_none()
                    {
                        // Stop parsing
                        self.current_command_range = Some(idx..=idx);
                        break;
                    }
                }

                _ => {
                    if token.kind == TokenKind::Newline
                        && let Some(prev_token) = &previous_token
                    {
                        if prev_token.annotation == TokenAnnotation::IsPartOfQuotedString
                            || matches!(
                                prev_token.token.kind,
                                TokenKind::Quote | TokenKind::SingleQuote
                            )
                        {
                            annotated_token.annotation = TokenAnnotation::IsPartOfQuotedString;
                        }
                    }

                    if annotated_token.is_word() {
                        // println!("prev token: {:?}", previous_token.as_ref().map(|t| &t.token));
                        if let Some(prev_token) = &previous_token {
                            match prev_token.token.kind {
                                TokenKind::Quote | TokenKind::SingleQuote => {
                                    annotated_token.annotation =
                                        TokenAnnotation::IsPartOfQuotedString;
                                }
                                TokenKind::Newline
                                    if matches!(
                                        prev_token.annotation,
                                        TokenAnnotation::IsPartOfQuotedString
                                    ) =>
                                {
                                    annotated_token.annotation =
                                        TokenAnnotation::IsPartOfQuotedString;
                                }
                                _ if self.current_command_range.is_none() => {
                                    annotated_token.annotation = TokenAnnotation::IsCommandWord;
                                }
                                _ => {
                                    // leave as None
                                }
                            }
                        } else {
                            annotated_token.annotation = TokenAnnotation::IsCommandWord;
                        }
                    }
                    if self.current_command_range.is_none() {
                        self.current_command_range = Some(idx..=idx);
                    } else if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
                }
            }

            previous_token = Some(annotated_token.clone());
        }

        if cfg!(test) {
            dbg!("Final nestings:");
            dbg!(&nestings);
        }

        // Mark the opening tokens with the closing tokens:
        // We need to collect the updates first to avoid mutable borrow issues
        let mut updates = Vec::new();
        for (idx, annotated_token) in self.tokens.iter().enumerate() {
            if let TokenAnnotation::IsClosing(opening_idx) = annotated_token.annotation {
                updates.push((opening_idx, idx));
            }
        }

        for (opening_idx, closing_idx) in updates {
            if let TokenAnnotation::IsOpening(None) = self.tokens[opening_idx].annotation {
                self.tokens[opening_idx].annotation = TokenAnnotation::IsOpening(Some(closing_idx));
            }
        }
    }

    pub fn needs_more_input(&self) -> bool {
        self.tokens
            .iter()
            .any(|t| matches!(t.annotation, TokenAnnotation::IsOpening(None)))
    }

    pub fn get_current_command_tokens(&self) -> Vec<&AnnotatedToken> {
        match &self.current_command_range {
            Some(range) => {
                return self.tokens[range.clone()]
                    .iter()
                    .collect::<Vec<_>>();
            }
            None => return Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn get_current_command_str(&self) -> String {
        self.get_current_command_tokens()
            .iter()
            .map(|t| t.token.value.to_string())
            .collect::<Vec<_>>()
            .join("")
    }
}

// Implicitly tested by command acceptance and tab_completion_context
// Just a few tests here
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nested_commands() {
        let input = r#"     echo $(ls $(echo nested) | grep pattern) > output.txt       "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, input.trim_start());
    }

    #[test]
    fn test_in_nested_command() {
        let input = r#"echo $(ls $(   echo nest    "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, "echo nest    ");
    }

    #[test]
    fn test_pipeline() {
        let input = r#"echo "héllo" && echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, r#"echo "wörld""#);
    }

    #[test]
    fn test_pipeline_with_nesting_1() {
        let input = r#"echo "héllo" && echo $(( bar "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"bar "#);
    }

    #[test]
    fn test_pipeline_with_nesting_2() {
        let input = r#"echo "héllo" && echo $(( bar ) "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"bar ) "#);
    }

    #[test]
    fn test_pipeline_with_nesting_3() {
        let input = r#"echo "héllo" && echo $(( bar )) "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"echo $(( bar )) "#);
    }

    #[test]
    fn test_annotations() {
        let input = r#"echo héllo && echo 'wörld'"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            debug!("{:?} - {:?}", t.token, t.annotation);
        }

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotation, TokenAnnotation::IsCommandWord);
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "héllo");
        assert_eq!(tokens[2].annotation, TokenAnnotation::None);
        assert_eq!(tokens[3].token.value, " ");
        assert_eq!(tokens[4].token.value, "&&");
        assert_eq!(tokens[4].annotation, TokenAnnotation::None);
        assert_eq!(tokens[5].token.value, " ");
        assert_eq!(tokens[6].token.value, "echo");
        assert_eq!(tokens[6].annotation, TokenAnnotation::IsCommandWord);
        assert_eq!(tokens[7].token.value, " ");
        assert_eq!(tokens[8].token.value, "'");
        assert_eq!(tokens[8].annotation, TokenAnnotation::IsOpening(Some(10)));
        assert_eq!(tokens[9].token.value, "wörld");
        assert_eq!(tokens[9].annotation, TokenAnnotation::IsPartOfQuotedString);
        assert_eq!(tokens[10].token.value, "'");
        assert_eq!(tokens[10].annotation, TokenAnnotation::IsClosing(8));
    }

    #[test]
    fn test_heredoc_annotations() {
        let input = "cat <<A <<-B\nline1\nA\nline2\nB\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            debug!("{:?} - {:?}", t.token, t.annotation);
        }
        assert_eq!(tokens[0].token.value, "cat");
        assert_eq!(tokens[0].annotation, TokenAnnotation::IsCommandWord);
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "<<A");
        assert_eq!(tokens[2].annotation, TokenAnnotation::IsOpening(Some(8)));
        assert_eq!(tokens[3].token.value, " ");
        assert_eq!(tokens[4].token.value, "<<-B");
        assert_eq!(tokens[4].annotation, TokenAnnotation::IsOpening(Some(12)));
        assert_eq!(tokens[5].token.value, "\n");
        assert_eq!(tokens[5].annotation, TokenAnnotation::None);
        assert_eq!(tokens[6].token.value, "line1");
        assert_eq!(tokens[6].annotation, TokenAnnotation::None);
        assert_eq!(tokens[7].token.value, "\n");
        assert_eq!(tokens[7].annotation, TokenAnnotation::None);
        assert_eq!(tokens[8].token.value, "A");
        assert_eq!(tokens[8].annotation, TokenAnnotation::IsClosing(2));
        assert_eq!(tokens[9].token.value, "\n");
        assert_eq!(tokens[9].annotation, TokenAnnotation::None);
        assert_eq!(tokens[10].token.value, "line2");
        assert_eq!(tokens[10].annotation, TokenAnnotation::None);
        assert_eq!(tokens[11].token.value, "\n");
        assert_eq!(tokens[11].annotation, TokenAnnotation::None);
        assert_eq!(tokens[12].token.value, "B");
        assert_eq!(tokens[12].annotation, TokenAnnotation::IsClosing(4));
    }

    #[test]
    fn test_pipe_and_separator() {
        let input = r#"echo "héllo" |& cat"#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), "cat");
    }

    #[test]
    fn test_pipe_and_separator_with_nesting() {
        let input = r#"echo "héllo" |& echo $(( bar "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"bar "#);
    }

    #[test]
    fn test_background_separator() {
        let input = r#"echo "héllo" & echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"echo "wörld""#);
    }

    #[test]
    fn test_double_semicolon_separator() {
        let input = r#"echo "héllo";; echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(parser.get_current_command_str(), r#"echo "wörld""#);
    }

    #[test]
    fn test_multiline_string_annotations() {
        let input = "echo 'line1\nline2'";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            debug!("{:?} - {:?}", t.token, t.annotation);
        }
        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(tokens[0].annotation, TokenAnnotation::IsCommandWord);
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "'");
        assert_eq!(tokens[2].annotation, TokenAnnotation::IsOpening(Some(6)));
        assert_eq!(tokens[3].token.value, "line1");
        assert_eq!(tokens[3].annotation, TokenAnnotation::IsPartOfQuotedString);
        assert_eq!(tokens[4].token.kind, TokenKind::Newline);
        assert_eq!(tokens[4].annotation, TokenAnnotation::IsPartOfQuotedString);
        assert_eq!(tokens[5].token.value, "line2");
        assert_eq!(tokens[5].annotation, TokenAnnotation::IsPartOfQuotedString);
        assert_eq!(tokens[6].token.value, "'");
        assert_eq!(tokens[6].annotation, TokenAnnotation::IsClosing(2));
    }
}
