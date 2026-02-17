use crossterm::cursor;
use flash::lexer::{Lexer, Token, TokenKind};
use std::collections::VecDeque;

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
    fn to_inclusive(&self) -> core::ops::RangeInclusive<usize>;
}

impl ToInclusiveRange for std::ops::Range<usize> {
    fn to_inclusive(&self) -> core::ops::RangeInclusive<usize> {
        self.start..=self.end
    }
}

#[derive(Debug)]
pub struct DParser {
    tokens: Vec<Token>,
    nestings: Vec<TokenKind>,
    // Heredocs are tracked separately since they close based on FIFO order, not LIFO like the other nestings
    heredocs: VecDeque<String>,
    current_command_start: Option<usize>,
    current_command_end: Option<usize>,
}

impl DParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            nestings: Vec::new(),
            heredocs: VecDeque::new(),
            current_command_start: None,
            current_command_end: None,
        }
    }

    pub fn from(input: &str) -> Self {
        let tokens = collect_tokens_include_whitespace(input);
        Self {
            tokens,
            nestings: Vec::new(),
            heredocs: VecDeque::new(),
            current_command_start: None,
            current_command_end: None,
        }
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
        let mut current_command_start = None;
        let mut current_command_end = None;
        let mut stop_parsing_at_command_boundary = false;

        let mut command_start_stack = Vec::new();

        loop {
            let (idx, token) = match toks.next() {
                Some(t) => t,
                None => break,
            };

            if cfg!(test) {
                dbg!(idx);
                dbg!(&token);
            }

            if let Some(pos) = cursor_byte_pos
                && token.byte_range().to_inclusive().contains(&pos)
            {
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
                command_start_stack.push(current_command_start);
                current_command_start = None; // set for next word after this
                current_command_end = None;
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
                // dbg!("Popping nesting:");
                // dbg!(&token.kind);
                // dbg!(&nestings);
                let kind = self.nestings.pop().unwrap();
                if kind == TokenKind::ArithSubst {
                    assert!(
                        toks.peek().unwrap().1.kind == TokenKind::RParen,
                        "expected two RParen tokens"
                    );
                    toks.next(); // consume the extra RParen
                }

                if stop_parsing_at_command_boundary {
                    break;
                }

                // Restore command start for the command that this nesting started, if any
                if let Some(prev_command_start) = command_start_stack.pop() {
                    current_command_start = prev_command_start;
                }
            }
            _ => {
                if current_command_start.is_none() && !token.kind.is_whitespace() {
                    current_command_start = Some(idx);
                }
                if current_command_start.is_some() && !token.kind.is_whitespace()  {
                   current_command_end = Some(idx);
                   println!("Setting current_command_end to idx {}", idx);
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

        self.current_command_start = current_command_start;
        self.current_command_end = current_command_end;
    }

    pub fn needs_more_input(&self) -> bool {
        !self.nestings.is_empty() || !self.heredocs.is_empty()
    }

    pub fn get_current_command_tokens(&self) -> &[Token] {
        let start = self.current_command_start.unwrap_or(0);
        let end = self.current_command_end.unwrap_or(start);
        &self.tokens[start..end + 1]
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
}
