use flash::lexer::{Lexer as FlashLexer, Token as FlashToken, TokenKind as FlashTokenKind};
use std::collections::HashMap;

fn line_and_column_to_byte_pos(input: &str) -> HashMap<(usize, usize), usize> {
    let mut current_line = 1; // flash lexer uses 1 based indexing
    let mut current_column = 1;
    let mut line_col_map = HashMap::new();

    for (byte_index, c) in input.char_indices() {
        line_col_map.insert((current_line, current_column), byte_index);

        if c == '\n' {
            current_line += 1;
            current_column = 1;
        } else {
            current_column += 1;
        }
    }

    line_col_map
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Word(String),
    Assignment,               // =
    Pipe,                     // |
    Semicolon,                // ;
    DoubleSemicolon,          // ;;
    Newline,                  // \n
    And,                      // &&
    Background,               // & (add this new token)
    Or,                       // ||
    LParen,                   // (
    RParen,                   // )
    LBrace,                   // {
    RBrace,                   // }
    Less,                     // <
    Great,                    // >
    DGreat,                   // >>
    Dollar,                   // $
    Quote,                    // "
    SingleQuote,              // '
    Backtick,                 // `
    Comment,                  // #
    CmdSubst,                 // $(
    ArithSubst,               // $((
    ArithCommand,             // ((
    ParamExpansion,           // ${
    ParamExpansionOp(String), // :-, :=, :?, :+, #, ##, %, %%
    ProcessSubstIn,           // <(
    ProcessSubstOut,          // >(
    HereDoc,     // << followed by delimiter      // TODO make delimiter part of the token
    HereDocDash, // <<- followed by delimiter
    HereDocContent(String), // Content of here-document
    HereString,  // <<<
    ExtGlob(char), // For ?(, *(, +(, @(, !(
    // Shell control flow keywords
    If,   // if keyword
    Then, // then keyword
    Elif, // elif keyword
    Else, // else keyword
    Fi,   // fi keyword
    Case, // case keyword
    Esac, // esac keyword
    // Function declaration keyword
    Function, // function keyword
    // Loop keywords
    For,   // for keyword
    While, // while keyword
    Until, // until keyword
    Do,    // do keyword
    Done,  // done keyword
    In,    // in keyword (used in for loops)
    // Break and continue for loops
    Break,    // break keyword
    Continue, // continue keyword
    Return,   // return keyword (for functions)
    // Bash-specific features
    DoubleLBracket,     // [[ - extended test command
    DoubleRBracket,     // ]] - end extended test
    WhiteSpace(String), // Basically any part of input that flash doesn't consider part of a token, but we want to preserve
}

impl From<FlashToken> for TokenKind {
    fn from(flash_token: FlashToken) -> Self {
        match flash_token.kind {
            FlashTokenKind::Word(s) => TokenKind::Word(s),
            FlashTokenKind::Assignment => TokenKind::Assignment,
            FlashTokenKind::Pipe => TokenKind::Pipe,
            FlashTokenKind::Semicolon => TokenKind::Semicolon,
            FlashTokenKind::DoubleSemicolon => TokenKind::DoubleSemicolon,
            FlashTokenKind::Newline => TokenKind::Newline,
            FlashTokenKind::And => TokenKind::And,
            FlashTokenKind::Or => TokenKind::Or,
            FlashTokenKind::Background => TokenKind::Background,
            FlashTokenKind::LParen => TokenKind::LParen,
            FlashTokenKind::RParen => TokenKind::RParen,
            FlashTokenKind::LBrace => TokenKind::LBrace,
            FlashTokenKind::RBrace => TokenKind::RBrace,
            FlashTokenKind::Less => TokenKind::Less,
            FlashTokenKind::Great => TokenKind::Great,
            FlashTokenKind::DGreat => TokenKind::DGreat,
            FlashTokenKind::Dollar => TokenKind::Dollar,
            FlashTokenKind::Quote => TokenKind::Quote,
            FlashTokenKind::SingleQuote => TokenKind::SingleQuote,
            FlashTokenKind::Backtick => TokenKind::Backtick,
            FlashTokenKind::Comment => TokenKind::Comment,
            // For simplicity, we treat all of these as Word tokens for now
            _ => TokenKind::Word(flash_token.value.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    kind: TokenKind,
    byte_pos: usize, // byte position in the input string
    byte_len: usize, // length of the token in bytes
}

impl Token {
    pub fn new(flash_token: FlashToken, line_col_to_byte: &HashMap<(usize, usize), usize>) -> Self {
        let kind = TokenKind::from(flash_token.clone());
        let byte_pos = *line_col_to_byte
            .get(&(flash_token.position.line, flash_token.position.column))
            .unwrap_or(&0);
        let byte_len = flash_token.value.len();
        Token {
            kind,
            byte_pos,
            byte_len,
        }
    }

    pub fn start_byte_pos(&self) -> usize {
        self.byte_pos
    }

    pub fn end_byte_pos(&self) -> usize {
        self.byte_pos + self.byte_len
    }
}

#[derive(Debug)]
struct Lexer {
    tokens: Vec<Token>,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        let mut lexer = FlashLexer::new(input);

        let mut tokens: Vec<Token> = Vec::new();

        let line_col_to_char = line_and_column_to_byte_pos(input);

        loop {
            let flash_token = lexer.next_token();
            if flash_token.kind == flash::lexer::TokenKind::EOF {
                break;
            }
            let token = Token::new(flash_token, &line_col_to_char);

            if let Some(prev_token) = tokens.last() {
                // prevent infinite loops on malformed input
                if token == *prev_token {
                    log::warn!("Lexer stuck on token: {:?}", token);
                    break;
                }

                // See if we have any whitespace between the end of the previous token and the start of the current token, and if so, add a WhiteSpace token
                let prev_token_end = prev_token.end_byte_pos();
                let token_start = token.start_byte_pos();
                if token_start > prev_token_end {
                    let whitespace = &input[prev_token_end..token_start];
                    tokens.push(Token {
                        kind: TokenKind::WhiteSpace(whitespace.to_string()),
                        byte_pos: prev_token_end,
                        byte_len: whitespace.len(),
                    });
                }
            }

            tokens.push(token);
        }

        // Remove the final token if it has zero length
        if let Some(last_token) = tokens.last() {
            if last_token.byte_len == 0 {
                tokens.pop();
            }
        }

        // Append any final whitespace at the end of the input as a WhiteSpace token
        let last_token_end = tokens.last().map_or(0, |t| t.end_byte_pos());
        if last_token_end < input.len() {
            let whitespace = &input[last_token_end..];
            tokens.push(Token {
                kind: TokenKind::WhiteSpace(whitespace.to_string()),
                byte_pos: last_token_end,
                byte_len: whitespace.len(),
            });
        }

        Lexer { tokens }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_on_empty() {
        assert_eq!(Lexer::new("").tokens, vec![]);
    }

    #[test]
    fn test_lexer_with_newlines() {
        let input = "echo foo\nbar\tbaz";
        assert_eq!(
            Lexer::new(input).tokens,
            vec![
                Token {
                    kind: TokenKind::Word("echo".into()),
                    byte_pos: 0,
                    byte_len: 4
                },
                Token {
                    kind: TokenKind::WhiteSpace(" ".into()),
                    byte_pos: 4,
                    byte_len: 1
                },
                Token {
                    kind: TokenKind::Word("foo".into()),
                    byte_pos: 5,
                    byte_len: 3
                },
                Token {
                    kind: TokenKind::Newline,
                    byte_pos: 8,
                    byte_len: 1
                },
                Token {
                    kind: TokenKind::Word("bar".into()),
                    byte_pos: 9,
                    byte_len: 3
                },
                Token {
                    kind: TokenKind::WhiteSpace("\t".into()),
                    byte_pos: 12,
                    byte_len: 1
                },
                Token {
                    kind: TokenKind::Word("baz".into()),
                    byte_pos: 13,
                    byte_len: 3
                },
            ]
        );
    }

    #[test]
    fn test_lexer_on_just_whitespace() {
        assert_eq!(
            Lexer::new("   \t  ").tokens,
            vec![Token {
                kind: TokenKind::WhiteSpace("   \t  ".into()),
                byte_pos: 0,
                byte_len: 6
            },]
        );
    }

    #[test]
    fn test_lexer_on_whitespace_at_end() {
        assert_eq!(
            Lexer::new("echo   \t  ").tokens,
            vec![
                Token {
                    kind: TokenKind::Word("echo".into()),
                    byte_pos: 0,
                    byte_len: 4
                },
                Token {
                    kind: TokenKind::WhiteSpace("   \t  ".into()),
                    byte_pos: 4,
                    byte_len: 6
                },
            ]
        );
    }

    #[test]
    fn test_preservers_whitespace_type() {
        let input = "echo \t \t    foo";

        let token = Lexer::new(input).tokens[1].clone(); // The whitespace token
        if let TokenKind::WhiteSpace(s) = &token.kind {
            assert_eq!(s, " \t \t    ");
        } else {
            panic!("Expected a WhiteSpace token");
        }
    }
}
