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

            let word_is_part_of_assignment = if token.kind.is_word() {
                previous_token.as_ref().map_or(false, |token| {
                    matches!(token.token.kind, TokenKind::Assignment)
                })
            } else {
                false
            };

            let token_inclusively_contains_cursor = cursor_byte_pos.map_or(false, |pos| {
                token.byte_range().to_inclusive().contains(&pos)
            });

            let token_strictly_contains_cursor =
                cursor_byte_pos.map_or(false, |pos| token.byte_range().contains(&pos));

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
                TokenKind::HereDoc(delim) | TokenKind::HereDocDash(delim) => {
                    annotated_token.annotation = TokenAnnotation::IsOpening(None);

                    heredocs.push_back((idx, delim.to_string()));
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

                    if token.kind.is_word() {
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
            println!("{:?} - {:?}", t.token, t.annotation);
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
            println!("{:?} - {:?}", t.token, t.annotation);
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
            println!("{:?} - {:?}", t.token, t.annotation);
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
