use flash::lexer::{Lexer, Position, Token, TokenKind};
use itertools::Itertools;
use std::collections::VecDeque;
use std::ops::{Range, RangeInclusive};

/// Applies bash quote-removal to a heredoc delimiter word.
///
/// From the bash manual: "If any part of word is quoted, the delimiter is the
/// result of quote removal on word."  Quote removal strips:
///  - surrounding or partial single-quote pairs (`'…'`)
///  - surrounding or partial double-quote pairs (`"…"`)
///  - backslash escapes (`\X` → `X`)
///
/// Examples:
///  `'EOF'`  → `EOF`
///  `"EOF"`  → `EOF`
///  `\EOF`   → `EOF`
///  `E'O'F`  → `EOF`
fn strip_heredoc_delimiter_quotes(delim: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = delim.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                i += 1;
                if i < chars.len() {
                    result.push(chars[i]);
                    i += 1;
                } else {
                    // Trailing backslash with no following character: keep it literally.
                    result.push('\\');
                }
            }
            '\'' => {
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    result.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    // Inside double quotes, backslash is special only before
                    // $, `, ", \, or newline (POSIX quote removal rules).
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        match chars[i + 1] {
                            '$' | '`' | '"' | '\\' | '\n' => {
                                i += 1;
                                result.push(chars[i]);
                            }
                            _ => {
                                result.push('\\');
                            }
                        }
                    } else {
                        result.push(chars[i]);
                    }
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            c => {
                result.push(c);
                i += 1;
            }
        }
    }
    result
}

fn split_token_into_lines(token: Token) -> Vec<Token> {
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
                        tokens.push(Token {
                            kind: TokenKind::Newline,
                            value: chunk_str,
                            position: Position {
                                line: row,
                                column: col,
                                byte: token.position.byte + chunk_byte_start,
                            },
                        });

                        row += 1;
                        col = 1; // flash lexer uses 1 based column numbers
                    }
                    _ => {
                        tokens.push(Token {
                            kind: TokenKind::Word(chunk_str.clone()),
                            value: chunk_str.clone(),
                            position: Position {
                                line: row,
                                column: col,
                                byte: token.position.byte + chunk_byte_start,
                            },
                        });

                        // flash lexer uses char indicies for col counts instead of grapheme width.
                        col += chunk_str.chars().count();
                    }
                }
            }
            tokens
        }
        _ => vec![token],
    }
}

#[test]
fn test_split_token_into_lines() {
    let token = Token {
        kind: TokenKind::Word("hello\nworld".to_string()),
        value: "hello\nworld".to_string(),
        position: Position {
            line: 1,
            column: 1,
            byte: 0,
        },
    };

    let tokens = split_token_into_lines(token);
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].kind, TokenKind::Word("hello".to_string()));
    assert_eq!(tokens[0].position.line, 1);
    assert_eq!(tokens[0].position.column, 1);
    assert_eq!(tokens[0].position.byte, 0);

    assert_eq!(tokens[1].kind, TokenKind::Newline);
    assert_eq!(tokens[1].position.line, 1);
    assert_eq!(tokens[1].position.column, 6);
    assert_eq!(tokens[1].position.byte, 5);

    assert_eq!(tokens[2].kind, TokenKind::Word("world".to_string()));
    assert_eq!(tokens[2].position.line, 2);
    assert_eq!(tokens[2].position.column, 1);
    assert_eq!(tokens[2].position.byte, 6);

    let tokens = split_token_into_lines(tokens[0].clone());
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenKind::Word("hello".to_string()));
}

pub fn collect_tokens_include_whitespace(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        let is_eof = matches!(token.kind, TokenKind::EOF);
        if is_eof {
            break;
        }
        tokens.extend(split_token_into_lines(token));
    }

    tokens
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
    IsPartOfSingleQuotedString,
    IsPartOfDoubleQuotedString,
    IsOpening(Option<usize>), // index of the closing token in the tokens vector
    IsClosing {
        opening_idx: usize,     // index of the opening token in the tokens vector
        is_auto_inserted: bool, // true if this closing token was automatically inserted by the editor
    },
    IsCommandWord(String), // the first word of a command. e.g.`git commit -m "message"` -> `git` would be annotated with this
    IsEnvVar,
}

#[derive(Debug, Clone)]
pub struct AnnotatedToken {
    pub token: Token,
    pub annotation: TokenAnnotation,
}

impl AnnotatedToken {
    pub fn new(token: Token) -> Self {
        Self {
            token,
            annotation: TokenAnnotation::None,
        }
    }
}

#[derive(Debug)]
pub struct DParser {
    tokens: Vec<AnnotatedToken>,

    current_command_range: Option<RangeInclusive<usize>>,
}

impl DParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens: tokens.into_iter().map(AnnotatedToken::new).collect(),

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

    pub fn into_tokens(self) -> Vec<AnnotatedToken> {
        self.tokens
    }

    fn nested_opening_satisfied(
        token: &Token,
        current_nesting: Option<&TokenKind>,
        is_command_extraction: bool,
    ) -> bool {
        match token.kind {
            TokenKind::Quote | TokenKind::SingleQuote if is_command_extraction => false,
            TokenKind::Backtick | TokenKind::Quote | TokenKind::SingleQuote => {
                if Some(&token.kind) == current_nesting {
                    // backtick or quote is acting as closer
                    false
                } else {
                    true
                }
            }
            _ => true,
        }
    }

    fn nested_closing_satisfied(token: &Token, current_nesting: Option<&TokenKind>) -> bool {
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
            (TokenKind::DoubleRParen, TokenKind::ArithSubst) => true,
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

        let mut stop_parsing_at_command_boundary = false;

        let mut command_start_stack = Vec::new();

        let mut previous_token: Option<AnnotatedToken> = None;

        let mut idx = 0;
        while idx < self.tokens.len() {
            // When closing an ArithSubst, two consecutive ) tokens are required.
            // Merge them into a single DoubleRParen by modifying self.tokens[idx] in place
            // and removing the second ) from the vector.
            if nestings.last().map(|(_, k)| k) == Some(&TokenKind::ArithSubst)
                && self.tokens[idx].token.kind == TokenKind::RParen
                && idx + 1 < self.tokens.len()
                && self.tokens[idx + 1].token.kind == TokenKind::RParen
            {
                let second = self.tokens.remove(idx + 1);
                self.tokens[idx].token.value.push_str(&second.token.value);
                self.tokens[idx].token.kind = TokenKind::DoubleRParen;
            }

            // Clone the token so we can match on it while still mutating self.tokens[idx].annotation.
            let token = self.tokens[idx].token.clone();

            let word_is_part_of_assignment = if token.kind.is_word() {
                previous_token
                    .as_ref()
                    .is_some_and(|token| matches!(token.token.kind, TokenKind::Assignment))
            } else {
                false
            };

            let token_inclusively_contains_cursor = cursor_byte_pos.is_some_and(|pos| {
                self.tokens[idx]
                    .token
                    .byte_range()
                    .to_inclusive()
                    .contains(&pos)
            });
            let token_strictly_contains_cursor = cursor_byte_pos
                .is_some_and(|pos| self.tokens[idx].token.byte_range().contains(&pos));
            let cursor_at_start_of_token =
                cursor_byte_pos.is_some_and(|pos| pos == self.tokens[idx].token.byte_range().start);

            let cursor_part_way_through_token =
                token_inclusively_contains_cursor && !cursor_at_start_of_token;

            if token_strictly_contains_cursor {
                stop_parsing_at_command_boundary = true;
            }

            if cfg!(test) {
                dbg!(
                    "Token: {:?}, Nestings: {:?}, Heredocs: {:?}, Current command range: {:?}",
                    &token,
                    &nestings,
                    &heredocs,
                    &self.current_command_range
                );
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
                | TokenKind::ExtGlob(_)
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
                    self.tokens[idx].annotation = TokenAnnotation::IsOpening(None);

                    if self.current_command_range.is_none() {
                        self.current_command_range = Some(idx..=idx);
                    }
                    nestings.push((idx, token.kind.clone()));
                    command_start_stack.push(self.current_command_range.clone());
                    self.current_command_range = None; // set for next word after this
                }
                TokenKind::HereDoc(delim) | TokenKind::HereDocDash(delim) => {
                    self.tokens[idx].annotation = TokenAnnotation::IsOpening(None);

                    heredocs.push_back((idx, strip_heredoc_delimiter_quotes(delim)));
                }
                TokenKind::RParen
                | TokenKind::DoubleRParen
                | TokenKind::Quote
                | TokenKind::SingleQuote
                | TokenKind::RBrace
                | TokenKind::Backtick
                | TokenKind::DoubleRBracket
                | TokenKind::Esac
                | TokenKind::Done
                | TokenKind::Fi
                    if Self::nested_closing_satisfied(&token, nestings.last().map(|(_, k)| k)) =>
                {
                    let (opening_idx, _kind) = nestings.pop().unwrap();
                    self.tokens[idx].annotation = TokenAnnotation::IsClosing {
                        opening_idx,
                        is_auto_inserted: false,
                    };

                    let current_command_range_contains_cursor =
                        cursor_byte_pos.is_some_and(|pos| {
                            self.current_command_range.as_ref().is_some_and(|r| {
                                r.clone().any(|idx| {
                                    self.tokens[idx]
                                        .token
                                        .byte_range()
                                        .to_inclusive()
                                        .contains(&pos)
                                })
                            })
                        });

                    if stop_parsing_at_command_boundary
                        && !cursor_part_way_through_token
                        && current_command_range_contains_cursor
                    {
                        // cursor_part_way_through_token is used to handle multi closing character tokens like )) and ]]
                        // echo $((10 * 2█))      -> cursor context is: 10 * 2
                        // echo $((10 * 2)█)      -> cursor context is: echo $((10 * 2))
                        // dbg!("Stopping parsing at command boundary");
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
                    self.tokens[idx].annotation = TokenAnnotation::IsClosing {
                        opening_idx,
                        is_auto_inserted: false,
                    };
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
                TokenKind::Whitespace(_) => {
                    if token_inclusively_contains_cursor
                        && let Some(range) = &mut self.current_command_range
                    {
                        *range = *range.start()..=idx;
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
                    let in_single_quote =
                        matches!(nestings.last(), Some((_, TokenKind::SingleQuote)));
                    let in_double_quote = matches!(nestings.last(), Some((_, TokenKind::Quote)));

                    if token.kind == TokenKind::Newline {
                        if in_single_quote {
                            self.tokens[idx].annotation =
                                TokenAnnotation::IsPartOfSingleQuotedString;
                        } else if in_double_quote {
                            self.tokens[idx].annotation =
                                TokenAnnotation::IsPartOfDoubleQuotedString;
                        }
                    }

                    if token.kind.is_word() {
                        if in_single_quote {
                            self.tokens[idx].annotation =
                                TokenAnnotation::IsPartOfSingleQuotedString;
                        } else if in_double_quote {
                            self.tokens[idx].annotation =
                                TokenAnnotation::IsPartOfDoubleQuotedString;
                        } else if let Some(prev_token) = &previous_token {
                            match prev_token.token.kind {
                                TokenKind::Dollar => {
                                    if let TokenAnnotation::IsCommandWord(start_of_command) =
                                        &prev_token.annotation
                                    {
                                        self.tokens[idx].annotation =
                                            TokenAnnotation::IsCommandWord(
                                                start_of_command.clone()
                                                    + &self.tokens[idx].token.value,
                                            );
                                    } else {
                                        self.tokens[idx].annotation = TokenAnnotation::IsEnvVar;
                                    }
                                }
                                _ if self.current_command_range.is_none() => {
                                    self.tokens[idx].annotation = TokenAnnotation::IsCommandWord(
                                        self.tokens[idx].token.value.clone(),
                                    );
                                }
                                _ => {
                                    // leave as None
                                }
                            }
                        } else {
                            self.tokens[idx].annotation = TokenAnnotation::IsCommandWord(
                                self.tokens[idx].token.value.clone(),
                            );
                        }
                    }

                    if self.current_command_range.is_none() {
                        if self.tokens[idx].annotation == TokenAnnotation::None {
                            self.tokens[idx].annotation = TokenAnnotation::IsCommandWord(
                                self.tokens[idx].token.value.clone(),
                            );
                        }
                        self.current_command_range = Some(idx..=idx);
                    } else if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
                }
            }

            previous_token = Some(self.tokens[idx].clone());
            idx += 1;
        }

        if cfg!(test) {
            dbg!("Final nestings:");
            dbg!(&nestings);
        }

        // Mark the opening tokens with the closing tokens:
        // We need to collect the updates first to avoid mutable borrow issues
        let mut updates = Vec::new();
        for (idx, annotated_token) in self.tokens.iter().enumerate() {
            if let TokenAnnotation::IsClosing { opening_idx, .. } = annotated_token.annotation {
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

    pub fn get_current_command_tokens(&self) -> Vec<&Token> {
        match &self.current_command_range {
            Some(range) => self.tokens[range.clone()]
                .iter()
                .map(|t| &t.token)
                .collect::<Vec<_>>(),
            None => Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn get_current_command_str(&self) -> String {
        self.get_current_command_tokens()
            .iter()
            .map(|t| t.value.to_string())
            .collect::<Vec<_>>()
            .join("")
    }

    /// Returns the closing character that should be automatically inserted after the character `c`
    /// was typed at byte position `just_inserted_pos`.
    ///
    /// `self` is the **stale** (pre-insertion) formatted buffer — i.e. the state of the buffer
    /// *before* `c` was typed.  This is `self.formatted_buffer_cache` in `App`.
    ///
    /// - `{`, `[`, `(` are unambiguously openers and always produce a closing counterpart.
    /// - `"`, `'`, `` ` `` are ambiguous: they close when there is already an unmatched opener of
    ///   the same kind before `just_inserted_pos` in the stale buffer; otherwise they open.
    pub fn closing_char_to_insert(
        tokens: &[AnnotatedToken],
        c: char,
        just_inserted_pos: usize,
    ) -> Option<char> {
        if tokens.iter().any(|t| {
            t.token
                .byte_range()
                .to_inclusive()
                .contains(&just_inserted_pos)
                && matches!(t.token.kind, TokenKind::Comment)
        }) {
            return None;
        }

        // Unambiguously opening characters – always auto-close.
        match c {
            '{' => return Some('}'),
            '[' => return Some(']'),
            '(' => return Some(')'),
            _ => {}
        }

        // Ambiguous characters: consult the stale token annotations.
        let (closing, opener_kind) = match c {
            '"' => ('"', TokenKind::Quote),
            '\'' => ('\'', TokenKind::SingleQuote),
            '`' => ('`', TokenKind::Backtick),
            _ => return None,
        };

        // If there is already an unmatched opener of the same kind strictly before the
        // insertion point, the character just typed is closing it – don't auto-insert.
        let has_unmatched_opener = tokens.iter().any(|p| {
            p.token.byte_range().start < just_inserted_pos
                && p.token.kind == opener_kind
                && matches!(p.annotation, TokenAnnotation::IsOpening(None))
        });

        if has_unmatched_opener {
            None
        } else {
            Some(closing)
        }
    }

    pub fn transfer_auto_inserted_flags(
        old_tokens: &[AnnotatedToken],
        new_tokens: &mut [AnnotatedToken],
    ) {
        // Go from the left while we see identical tokens and mark any closing tokens in new_tokens as auto-inserted if the corresponding token in old_tokens was auto-inserted.
        for (old, new) in old_tokens.iter().zip(new_tokens.iter_mut()) {
            if old.token.kind != new.token.kind || old.token.value != new.token.value {
                break;
            }
            if let TokenAnnotation::IsClosing {
                opening_idx: old_opening_idx,
                is_auto_inserted: true,
            } = old.annotation
                && let TokenAnnotation::IsClosing {
                    opening_idx: new_opening_idx,
                    ..
                } = new.annotation
                && old_opening_idx == new_opening_idx
            {
                new.annotation = TokenAnnotation::IsClosing {
                    opening_idx: new_opening_idx,
                    is_auto_inserted: true,
                };
            }
        }

        // Go from the right while we see identical tokens and do the same.
        for (old, new) in old_tokens.iter().rev().zip(new_tokens.iter_mut().rev()) {
            if old.token.kind != new.token.kind || old.token.value != new.token.value {
                break;
            }
            if let TokenAnnotation::IsClosing {
                opening_idx: _,
                is_auto_inserted: true,
            } = old.annotation
                && let TokenAnnotation::IsClosing {
                    opening_idx: new_opening_idx,
                    ..
                } = new.annotation
            {
                // if old_opening_idx == new_opening_idx {
                new.annotation = TokenAnnotation::IsClosing {
                    opening_idx: new_opening_idx,
                    is_auto_inserted: true,
                };
                // }
            }
        }
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
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(
            tokens[0].annotation,
            TokenAnnotation::IsCommandWord("echo".to_string())
        );
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "héllo");
        assert_eq!(tokens[2].annotation, TokenAnnotation::None);
        assert_eq!(tokens[3].token.value, " ");
        assert_eq!(tokens[4].token.value, "&&");
        assert_eq!(tokens[4].annotation, TokenAnnotation::None);
        assert_eq!(tokens[5].token.value, " ");
        assert_eq!(tokens[6].token.value, "echo");
        assert_eq!(
            tokens[6].annotation,
            TokenAnnotation::IsCommandWord("echo".to_string())
        );
        assert_eq!(tokens[7].token.value, " ");
        assert_eq!(tokens[8].token.value, "'");
        assert_eq!(tokens[8].annotation, TokenAnnotation::IsOpening(Some(10)));
        assert_eq!(tokens[9].token.value, "wörld");
        assert_eq!(
            tokens[9].annotation,
            TokenAnnotation::IsPartOfSingleQuotedString
        );
        assert_eq!(tokens[10].token.value, "'");
        assert_eq!(
            tokens[10].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 8,
                is_auto_inserted: false
            }
        );
    }

    #[test]
    fn test_double_quote_annotations() {
        let input = r#"echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(
            tokens[0].annotation,
            TokenAnnotation::IsCommandWord("echo".to_string())
        );
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "\"");
        assert_eq!(tokens[2].annotation, TokenAnnotation::IsOpening(Some(4)));
        assert_eq!(tokens[3].token.value, "wörld");
        assert_eq!(
            tokens[3].annotation,
            TokenAnnotation::IsPartOfDoubleQuotedString
        );
        assert_eq!(tokens[4].token.value, "\"");
        assert_eq!(
            tokens[4].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false
            }
        );
    }

    #[test]
    fn test_heredoc_annotations() {
        let input = "cat <<A <<-B\nline1\nA\nline2\nB\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }
        assert_eq!(tokens[0].token.value, "cat");
        assert_eq!(
            tokens[0].annotation,
            TokenAnnotation::IsCommandWord("cat".to_string())
        );
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
        assert_eq!(
            tokens[8].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false
            }
        );
        assert_eq!(tokens[9].token.value, "\n");
        assert_eq!(tokens[9].annotation, TokenAnnotation::None);
        assert_eq!(tokens[10].token.value, "line2");
        assert_eq!(tokens[10].annotation, TokenAnnotation::None);
        assert_eq!(tokens[11].token.value, "\n");
        assert_eq!(tokens[11].annotation, TokenAnnotation::None);
        assert_eq!(tokens[12].token.value, "B");
        assert_eq!(
            tokens[12].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 4,
                is_auto_inserted: false
            }
        );
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
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }
        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(
            tokens[0].annotation,
            TokenAnnotation::IsCommandWord("echo".to_string())
        );
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "'");
        assert_eq!(tokens[2].annotation, TokenAnnotation::IsOpening(Some(6)));
        assert_eq!(tokens[3].token.value, "line1");
        assert_eq!(
            tokens[3].annotation,
            TokenAnnotation::IsPartOfSingleQuotedString
        );
        assert_eq!(tokens[4].token.kind, TokenKind::Newline);
        assert_eq!(
            tokens[4].annotation,
            TokenAnnotation::IsPartOfSingleQuotedString
        );
        assert_eq!(tokens[5].token.value, "line2");
        assert_eq!(
            tokens[5].annotation,
            TokenAnnotation::IsPartOfSingleQuotedString
        );
        assert_eq!(tokens[6].token.value, "'");
        assert_eq!(
            tokens[6].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false
            }
        );
    }

    #[test]
    fn test_arith_subst_annotations() {
        // The two consecutive ) tokens that close an ArithSubst are merged into a single
        // DoubleRParen token with value "))" covering both characters.  The phantom second )
        // is removed from the token list entirely, so subsequent tokens have the correct index
        // as if the second ) never existed.
        let input = r#"echo $(( bar ))"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();

        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        // After merging: echo (0), ' ' (1), $(( (2), ' ' (3), bar (4), ' ' (5), )) (6)
        // The phantom second ) is gone; total token count is 7.
        assert_eq!(tokens.len(), 7);

        assert_eq!(tokens[2].token.kind, TokenKind::ArithSubst);
        assert_eq!(tokens[2].annotation, TokenAnnotation::IsOpening(Some(6)));

        assert_eq!(tokens[6].token.kind, TokenKind::DoubleRParen);
        assert_eq!(tokens[6].token.value, "))");
        assert_eq!(
            tokens[6].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false
            }
        );
    }

    #[test]
    fn test_env_var_annotations() {
        let input = r#"echo $HOME"#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }
        assert_eq!(tokens[0].token.value, "echo");
        assert_eq!(
            tokens[0].annotation,
            TokenAnnotation::IsCommandWord("echo".to_string())
        );
        assert_eq!(tokens[1].token.value, " ");
        assert_eq!(tokens[2].token.value, "$");
        assert_eq!(tokens[2].annotation, TokenAnnotation::None);
        assert_eq!(tokens[3].token.value, "HOME");
        assert_eq!(tokens[3].annotation, TokenAnnotation::IsEnvVar);
    }

    // ── closing_char_to_insert ───────────────────────────────────────────────
    // These tests pass a *stale* (pre-insertion) FormattedBuffer to
    // closing_char_to_insert, mirroring how App uses formatted_buffer_cache.

    #[test]
    fn closing_char_for_opening_double_quote() {
        // Stale buffer is "echo " (before the " was typed).
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            Some('"')
        );
    }

    #[test]
    fn no_closing_char_for_closing_double_quote() {
        // Stale buffer is `echo "hello` (before the closing " was typed).
        let stale = r#"echo "hello"#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            None
        );
    }

    #[test]
    fn closing_char_for_opening_single_quote() {
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            Some('\'')
        );
    }

    #[test]
    fn no_closing_char_for_closing_single_quote() {
        let stale = "echo 'hello";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '\'', just_inserted_pos),
            None
        );
    }

    #[test]
    fn closing_char_for_opening_brace() {
        // { is never ambiguous; always produces a closing }.
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '{', just_inserted_pos),
            Some('}')
        );
    }

    #[test]
    fn closing_char_for_opening_backtick() {
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '`', just_inserted_pos),
            Some('`')
        );
    }

    #[test]
    fn no_closing_char_for_closing_backtick() {
        // Stale buffer is `echo `ls` (before the closing backtick was typed).
        let stale = "echo `ls";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '`', just_inserted_pos),
            None
        );
    }

    #[test]
    fn no_closing_char_for_unrecognised_character() {
        let stale = "echo ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), 'a', just_inserted_pos),
            None
        );
    }

    #[test]
    fn closing_char_second_quote_pair_after_first_closed() {
        // `echo "a" ` – the first pair is closed; the next " opens a new pair.
        let stale = r#"echo "a" "#;
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            Some('"')
        );
    }

    #[test]
    fn closing_char_dont_insert_in_comment() {
        // `echo # comment ` – the # starts a comment, so the next " is just a literal character, not an opener.
        let stale = "echo # comment ";
        let mut parser = DParser::from(stale);
        parser.walk_to_end();
        let just_inserted_pos = stale.len();
        assert_eq!(
            DParser::closing_char_to_insert(&parser.tokens(), '"', just_inserted_pos),
            None
        );
    }

    #[test]
    fn test_strip_heredoc_delimiter_quotes() {
        assert_eq!(strip_heredoc_delimiter_quotes("EOF"), "EOF");
        assert_eq!(strip_heredoc_delimiter_quotes("'EOF'"), "EOF");
        assert_eq!(strip_heredoc_delimiter_quotes("\"EOF\""), "EOF");
        assert_eq!(strip_heredoc_delimiter_quotes("\\EOF"), "EOF");
        assert_eq!(strip_heredoc_delimiter_quotes("E'O'F"), "EOF");
        assert_eq!(strip_heredoc_delimiter_quotes("E\"O\"F"), "EOF");
        assert_eq!(strip_heredoc_delimiter_quotes("'E'O'F'"), "EOF");
        assert_eq!(strip_heredoc_delimiter_quotes("\\E\\O\\F"), "EOF");
        // Trailing backslash is kept literally.
        assert_eq!(strip_heredoc_delimiter_quotes("EOF\\"), "EOF\\");
        // Backslash inside double quotes: only special before $`"\newline.
        assert_eq!(strip_heredoc_delimiter_quotes("\"E\\\\F\""), "E\\F");
        assert_eq!(strip_heredoc_delimiter_quotes("\"E\\xF\""), "E\\xF");
    }

    #[test]
    fn test_heredoc_single_quoted_delimiter() {
        // Single-quoted delimiter: closing line is the bare word without quotes.
        let input = "cat <<'EOF'\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        // <<'EOF' token should be an opening that is matched.
        assert_eq!(tokens[2].token.value, "<<'EOF'");
        assert!(matches!(
            tokens[2].annotation,
            TokenAnnotation::IsOpening(Some(_))
        ));

        // Find the "EOF" closing token.
        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false,
            }
        );
    }

    #[test]
    fn test_heredoc_double_quoted_delimiter() {
        let input = "cat <<\"EOF\"\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        // <<"EOF" token should be matched.
        assert_eq!(tokens[2].token.value, "<<\"EOF\"");
        assert!(matches!(
            tokens[2].annotation,
            TokenAnnotation::IsOpening(Some(_))
        ));

        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false,
            }
        );
    }

    #[test]
    fn test_heredoc_backslash_quoted_delimiter() {
        let input = "cat <<\\EOF\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        // <<\EOF token should be matched.
        assert_eq!(tokens[2].token.value, "<<\\EOF");
        assert!(matches!(
            tokens[2].annotation,
            TokenAnnotation::IsOpening(Some(_))
        ));

        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false,
            }
        );
    }

    #[test]
    fn test_heredoc_mixed_quoted_delimiter() {
        // Partially-quoted delimiter: E'O'F is equivalent to EOF.
        let input = "cat <<E'O'F\nhello\nEOF\n";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        assert_eq!(tokens[2].token.value, "<<E'O'F");
        assert!(matches!(
            tokens[2].annotation,
            TokenAnnotation::IsOpening(Some(_))
        ));

        let closing_idx = tokens.iter().position(|t| t.token.value == "EOF").unwrap();
        assert_eq!(
            tokens[closing_idx].annotation,
            TokenAnnotation::IsClosing {
                opening_idx: 2,
                is_auto_inserted: false,
            }
        );
    }

    #[test]
    fn test_env_var_starting_command() {
        let input = "$HOME/bin/echo";
        let mut parser = DParser::from(input);
        parser.walk_to_end();

        let tokens = parser.tokens();
        for t in tokens {
            dbg!("{:?} - {:?}", &t.token, &t.annotation);
        }

        assert_eq!(tokens[0].token.value, "$");
        assert_eq!(
            tokens[0].annotation,
            TokenAnnotation::IsCommandWord("$".to_string())
        );
        assert_eq!(tokens[1].token.value, "HOME/bin/echo");
        assert_eq!(
            tokens[1].annotation,
            TokenAnnotation::IsCommandWord("$HOME/bin/echo".to_string())
        );
    }
}
