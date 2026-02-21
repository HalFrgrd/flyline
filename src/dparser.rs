use flash::lexer::{Lexer, Token, TokenKind};
use std::collections::VecDeque;
use std::ops::{Range, RangeInclusive};

pub fn collect_tokens_include_whitespace(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        let is_eof = matches!(token.kind, TokenKind::EOF);
        if is_eof {
            break;
        }
        tokens.push(token);
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

#[derive(Debug)]
pub struct DParser {
    tokens: Vec<Token>,
    nestings: Vec<TokenKind>,
    // Heredocs are tracked separately since they close based on FIFO order, not LIFO like the other nestings
    heredocs: VecDeque<String>,
    current_command_range: Option<RangeInclusive<usize>>,
}

impl DParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            nestings: Vec::new(),
            heredocs: VecDeque::new(),
            current_command_range: None,
        }
    }

    pub fn from(input: &str) -> Self {
        let tokens = collect_tokens_include_whitespace(input);
        Self {
            tokens,
            nestings: Vec::new(),
            heredocs: VecDeque::new(),
            current_command_range: None,
        }
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    fn nested_opening_satisfied(token: &Token, current_nesting: Option<&TokenKind>) -> bool {
        match token.kind {
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

    pub fn walk(&mut self, cursor_byte_pos: Option<usize>) {
        // Walk through the tokens until we reach the end or the cursor position, updating nestings and heredocs along the way

        // echo $(( grep 1 + 2      # command is grep
        // echo $(( grep 1 + 2 )    # command is grep
        // echo $(( grep 1 + 2 ))   # command is echo, since the cursor is after the closing ))

        let mut toks = self.tokens.iter().enumerate().peekable();
        let mut stop_parsing_at_command_boundary = false;

        let mut command_start_stack = Vec::new();

        loop {
            let (mut idx, mut token) = match toks.next() {
                Some(t) => t,
                None => break,
            };

            // if cfg!(test) {
            //     dbg!(idx);
            //     dbg!(&token);
            // }

            let word_is_part_of_assignment = if let TokenKind::Word(_) = token.kind {
                idx > 0
                    && self
                        .tokens
                        .get(idx - 1)
                        .map_or(false, |t| matches!(t.kind, TokenKind::Assignment))
            } else {
                false
            };

            let token_inclusively_contains_cursor = cursor_byte_pos
                .map(|pos| token.byte_range().to_inclusive().contains(&pos))
                .unwrap_or(false);

            let token_strictly_contains_cursor = cursor_byte_pos
                .map(|pos| token.byte_range().contains(&pos))
                .unwrap_or(false);

            if token_inclusively_contains_cursor {
                // Stop parsing
                stop_parsing_at_command_boundary = true;
            }

            match &token.kind {
            TokenKind::LBrace
            // | TokenKind::LParen
            | TokenKind::DoubleLBracket
            | TokenKind::Quote
            | TokenKind::SingleQuote
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
                if Self::nested_opening_satisfied(&token, self.nestings.last()) =>
            {
                // dbg!("Pushing nesting:");
                // dbg!(&token.kind);
                // dbg!(&nestings);
                self.nestings.push(token.kind.clone());
                command_start_stack.push(self.current_command_range.clone());
                self.current_command_range = None; // set for next word after this
            }
            TokenKind::HereDoc(delim) | TokenKind::HereDocDash(delim) => {
                self.heredocs.push_back(delim.to_string());
            }
            TokenKind::RParen
            | TokenKind::RBrace
            | TokenKind::Backtick
            | TokenKind::DoubleRBracket
            | TokenKind::Quote
            | TokenKind::SingleQuote
            | TokenKind::Esac
            | TokenKind::Done
            | TokenKind::Fi
                if Self::nested_closing_satisfied(&token, self.nestings.last(), toks.peek().map(|(_, t)| t)) =>
            {
                println!("Popping nesting:");
                dbg!(&token.kind);
                dbg!(&self.nestings);
                let kind = self.nestings.pop().unwrap();
                if kind == TokenKind::ArithSubst {
                    assert!(
                        toks.peek().unwrap().1.kind == TokenKind::RParen,
                        "expected two RParen tokens"
                    );
                    (idx, token) = toks.next().unwrap(); // consume the extra RParen
                }


                let should_pop = !stop_parsing_at_command_boundary || token_strictly_contains_cursor;
                // Restore command start for the command that this nesting started, if any
                if should_pop && let Some(prev_command_range) = command_start_stack.pop() {
                    println!("Restoring command range to:");
                    dbg!(&prev_command_range);
                    self.current_command_range = prev_command_range;
                    if let Some(range) = &mut self.current_command_range {
                        *range = *range.start()..=idx;
                    }
                }

                if stop_parsing_at_command_boundary {
                    println!("Stopping parsing at command boundary");
                    break;
                }
            }
            TokenKind::Word(_) if word_is_part_of_assignment => {
                if stop_parsing_at_command_boundary {
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


                if token_strictly_contains_cursor && stop_parsing_at_command_boundary && self.current_command_range.is_none() {
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

    pub fn get_current_command_tokens(&self) -> &[Token] {
        match &self.current_command_range {
            Some(range) => {
                return &self.tokens[range.clone()];
            }
            None => return &[],
        }
    }

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
        parser.walk_to_end();
        assert!(parser.nestings.is_empty());
        assert!(parser.heredocs.is_empty());

        let command_tokens = parser.get_current_command_tokens();
        let command_str = command_tokens
            .iter()
            .map(|t| t.value.to_string())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(command_str, input.trim());
    }

    #[test]
    fn test_in_nested_command() {
        let input = r#"echo $(ls $(   echo nest    "#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        assert_eq!(
            parser.nestings,
            vec![TokenKind::CmdSubst, TokenKind::CmdSubst]
        );
        assert!(parser.heredocs.is_empty());

        let command_tokens = parser.get_current_command_tokens();
        let command_str = command_tokens
            .iter()
            .map(|t| t.value.to_string())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(command_str, "echo nest");
    }

    #[test]
    fn test_pipeline() {
        let input = r#"echo "héllo" && echo "wörld""#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        assert!(parser.nestings.is_empty());
        assert!(parser.heredocs.is_empty());
        let command_str = parser.get_current_command_str();
        assert_eq!(command_str, r#"echo "wörld""#);
    }

    #[test]
    fn test_pipeline_with_nesting_1() {
        let input = r#"echo "héllo" && echo $(( bar "#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        assert_eq!(parser.get_current_command_str(), r#"bar"#);
    }

    #[test]
    fn test_pipeline_with_nesting_2() {
        let input = r#"echo "héllo" && echo $(( bar ) "#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        assert_eq!(parser.get_current_command_str(), r#"bar )"#);
    }

    #[test]
    fn test_pipeline_with_nesting_3() {
        let input = r#"echo "héllo" && echo $(( bar )) "#;
        let mut parser = DParser::from(input);
        parser.walk_to_end();
        assert_eq!(parser.get_current_command_str(), r#"echo $(( bar ))"#);
    }
}
