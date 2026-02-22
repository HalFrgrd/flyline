use flash::lexer::{Lexer, Position, Token, TokenKind};
use itertools::Itertools;
use std::collections::VecDeque;
use std::ops::{Range, RangeInclusive};

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

#[derive(Debug, Clone)]
pub enum TokenAnnotation {
    None,
    HasOpeningQuote,
    IsOpening(Option<usize>), // index of the closing token in the tokens vector
    IsClosing(Option<usize>), // index of the opening token in the tokens vector
    IsCommandWord, // the first word of a command. e.g.`git commit -m "message"` -> `git` would be annotated with this
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
    nestings: Vec<TokenKind>,
    // Heredocs are tracked separately since they close based on FIFO order, not LIFO like the other nestings
    heredocs: VecDeque<String>,
    current_command_range: Option<RangeInclusive<usize>>,
}

impl DParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens: tokens.into_iter().map(AnnotatedToken::new).collect(),
            nestings: Vec::new(),
            heredocs: VecDeque::new(),
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

    fn nested_opening_satisfied(token: &Token, current_nesting: Option<&TokenKind>, is_command_extraction: bool) -> bool {
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

        let mut annotated_tokens = self.tokens.iter().enumerate().peekable();
        let mut stop_parsing_at_command_boundary = false;

        let mut command_start_stack = Vec::new();

        loop {
            let (mut idx, mut annotated_token) = match annotated_tokens.next() {
                Some(t) => t,
                None => break,
            };
            let mut token = &annotated_token.token;

            let word_is_part_of_assignment = if let TokenKind::Word(_) = token.kind {
                idx > 0
                    && self
                        .tokens
                        .get(idx - 1)
                        .map_or(false, |t| matches!(t.token.kind, TokenKind::Assignment))
            } else {
                false
            };

            let token_inclusively_contains_cursor = cursor_byte_pos
                .map_or(false, |pos| token.byte_range().to_inclusive().contains(&pos));

            let token_strictly_contains_cursor = cursor_byte_pos
                .map_or(false, |pos| token.byte_range().contains(&pos));

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
                | TokenKind::ExtGlob(_)
                | TokenKind::If
                | TokenKind::Case
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Until
                    if Self::nested_opening_satisfied(&token, self.nestings.last(), cursor_byte_pos.is_some()) =>
                {
                    if self.current_command_range.is_none() {
                        self.current_command_range = Some(idx..=idx);
                    }
                    self.nestings.push(token.kind.clone());
                    command_start_stack.push(self.current_command_range.clone());
                    self.current_command_range = None; // set for next word after this
                }
                TokenKind::HereDoc(delim) | TokenKind::HereDocDash(delim) => {
                    self.heredocs.push_back(delim.to_string());
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
                        self.nestings.last(),
                        annotated_tokens.peek().map(|(_, t)| &t.token).as_ref(),
                    ) =>
                {
                    let kind = self.nestings.pop().unwrap();
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
                        println!("Stopping parsing at command boundary");
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
                TokenKind::And | TokenKind::Or | TokenKind::Pipe | TokenKind::Semicolon => {
                    if stop_parsing_at_command_boundary {
                        break;
                    }
                    self.current_command_range = None;
                }
                TokenKind::Whitespace(_) => {
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
                    if self.current_command_range.is_none() {
                        self.current_command_range = Some(idx..=idx);
                    } else if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
                }
            }

            if let TokenKind::Word(word) = &token.kind {
                if self.heredocs.front().is_some_and(|delim| delim == word) {
                    self.heredocs.pop_front();
                }
            }
        }

        if cfg!(test) {
            dbg!("Final nestings:");
            dbg!(&self.nestings);
        }
    }

    pub fn needs_more_input(&self) -> bool {
        !self.nestings.is_empty() || !self.heredocs.is_empty()
    }

    pub fn get_current_command_tokens(&self) -> Vec<&Token> {
        match &self.current_command_range {
            Some(range) => {
                return self.tokens[range.clone()]
                    .iter()
                    .map(|t| &t.token)
                    .collect::<Vec<_>>();
            }
            None => return Vec::new(),
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
        assert!(parser.nestings.is_empty());
        assert!(parser.heredocs.is_empty());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, input.trim_start());
    }

    #[test]
    fn test_in_nested_command() {
        let input = r#"echo $(ls $(   echo nest    "#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert_eq!(
            parser.nestings,
            vec![TokenKind::CmdSubst, TokenKind::CmdSubst]
        );
        assert!(parser.heredocs.is_empty());

        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, "echo nest    ");
    }

    #[test]
    fn test_pipeline() {
        let input = r#"echo "héllo" && echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_cursor(input.len());
        assert!(parser.nestings.is_empty());
        assert!(parser.heredocs.is_empty());
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
}
