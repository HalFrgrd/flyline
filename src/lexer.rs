use flash::lexer::{Lexer as FlashLexer, Token as FlashToken, TokenKind as FlashTokenKind};
use std::collections::HashMap;

fn line_and_column_to_byte_pos(input: &str) -> HashMap<(usize, usize), usize> {
    let mut current_line = 1; // flash lexer uses 1 based indexing
    let mut current_column = 1;
    let mut line_col_map = HashMap::new();

    for (byte_index, c) in input.char_indices() {
        dbg!(byte_index, c, current_line, current_column);
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
            FlashTokenKind::CmdSubst => TokenKind::CmdSubst,
            FlashTokenKind::ArithSubst => TokenKind::ArithSubst,
            FlashTokenKind::ArithCommand => TokenKind::ArithCommand,
            FlashTokenKind::ParamExpansion => TokenKind::ParamExpansion,
            FlashTokenKind::ParamExpansionOp(op) => TokenKind::ParamExpansionOp(op),
            FlashTokenKind::ProcessSubstIn => TokenKind::ProcessSubstIn,
            FlashTokenKind::ProcessSubstOut => TokenKind::ProcessSubstOut,
            FlashTokenKind::HereDoc => TokenKind::HereDoc,
            FlashTokenKind::HereDocDash => TokenKind::HereDocDash,
            FlashTokenKind::HereDocContent(content) => TokenKind::HereDocContent(content),
            FlashTokenKind::HereString => TokenKind::HereString,
            FlashTokenKind::ExtGlob(c) => TokenKind::ExtGlob(c),
            FlashTokenKind::If => TokenKind::If,
            FlashTokenKind::Then => TokenKind::Then,
            FlashTokenKind::Elif => TokenKind::Elif,
            FlashTokenKind::Else => TokenKind::Else,
            FlashTokenKind::Fi => TokenKind::Fi,
            FlashTokenKind::Case => TokenKind::Case,
            FlashTokenKind::Esac => TokenKind::Esac,
            FlashTokenKind::Function => TokenKind::Function,
            FlashTokenKind::For => TokenKind::For,
            FlashTokenKind::While => TokenKind::While,
            FlashTokenKind::Until => TokenKind::Until,
            FlashTokenKind::Do => TokenKind::Do,
            FlashTokenKind::Done => TokenKind::Done,
            FlashTokenKind::In => TokenKind::In,
            FlashTokenKind::Break => TokenKind::Break,
            FlashTokenKind::Continue => TokenKind::Continue,
            FlashTokenKind::Return => TokenKind::Return,
            FlashTokenKind::DoubleLBracket => TokenKind::DoubleLBracket,
            FlashTokenKind::DoubleRBracket => TokenKind::DoubleRBracket,
            // For simplicity, we treat all of these as Word tokens for now
            _ => TokenKind::Word(flash_token.value.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    byte_pos: usize, // byte position in the input string
    byte_len: usize, // length of the token in bytes
}

impl Token {
    pub fn new(kind: TokenKind, byte_pos: usize, byte_len: usize) -> Self {
        Token {
            kind,
            byte_pos,
            byte_len,
        }
    }

    // meant to mimic the behavior of flash's deslash logic
    fn deslash_str(s: &str) -> String {
        let mut deslashed = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                // If we see a backslash, we want to skip it and take the next character as a literal
                if let Some(&next_char) = chars.peek() {
                    deslashed.push(next_char);
                    chars.next(); // consume the next character
                } else {
                    // If the backslash is the last character, we treat it as a literal backslash
                    deslashed.push(c);

                }
            } else {
                deslashed.push(c);
            }
        }

        deslashed
    }

    pub fn new_from_flash(
        flash_token: FlashToken,
        line_col_to_byte: &HashMap<(usize, usize), usize>,
        source: &str,
    ) -> Self {
        let mut kind = TokenKind::from(flash_token.clone());
        let byte_pos = *line_col_to_byte
            .get(&(flash_token.position.line, flash_token.position.column))
            .unwrap();

        let mut true_byte_len = flash_token.value.len();
        if let TokenKind::Word(ref mut s) = kind  {

            // flash annoyingly doesn't include backslashes when they are escaping a character
            // but we want to include them in our tokens, so we need to adjust the byte_len to include any backslashes that are escaping characters in the token
            loop {
                // TODO:  make safer
                if let Some(slice) = source.get(byte_pos..byte_pos + true_byte_len) {

                    let deslashed = Token::deslash_str(slice);
                    if deslashed == *s {
                        break;
                    }
                    true_byte_len += 1;
                } else {
                    break;
                }
            }
            true_byte_len = true_byte_len.min(source.len() - byte_pos);

            *s = source[byte_pos..byte_pos + true_byte_len].to_string();
        }

        Token::new(kind, byte_pos, true_byte_len)
    }

    pub fn new_whitespace(s: &str, byte_pos: usize) -> Self {
        if cfg!(test) {
            if s.chars().any(|c| !c.is_whitespace() || c == '\n') {
                panic!(
                    "Whitespace token contains non-whitespace characters: {:?}",
                    s
                );
            }
        }

        Token {
            kind: TokenKind::WhiteSpace(s.to_string()),
            byte_pos,
            byte_len: s.len(),
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
pub struct Lexer {
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
            println!("Got flash token: {:?} at line {}, column {} with value {:?}",
                flash_token.kind, flash_token.position.line, flash_token.position.column, flash_token.value);
            let token = Token::new_from_flash(flash_token, &line_col_to_char, input);
            if cfg!(test) {
                println!("Got token: {:?} (byte pos: {}, byte len: {})", token.kind, token.byte_pos, token.byte_len);
            }

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
                    tokens.push(Token::new_whitespace(whitespace, prev_token_end));
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
        dbg!(&tokens);
        dbg!(last_token_end);
        if last_token_end < input.len() {
            let whitespace = &input[last_token_end..];
            tokens.push(Token::new_whitespace(whitespace, last_token_end));
        }

        Lexer { tokens }
    }

    pub fn tokens(&self) -> &Vec<Token> {
        &self.tokens
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
        let tokens = Lexer::new(input).tokens;
        assert_eq!(tokens[0], Token::new(TokenKind::Word("echo".into()), 0, 4));
        assert_eq!(
            tokens[1],
            Token::new(TokenKind::WhiteSpace(" ".into()), 4, 1)
        );
        assert_eq!(tokens[2], Token::new(TokenKind::Word("foo".into()), 5, 3));
        assert_eq!(tokens[3], Token::new(TokenKind::Newline, 8, 1));
        assert_eq!(tokens[4], Token::new(TokenKind::Word("bar".into()), 9, 3));
        assert_eq!(
            tokens[5],
            Token::new(TokenKind::WhiteSpace("\t".into()), 12, 1)
        );
        assert_eq!(tokens[6], Token::new(TokenKind::Word("baz".into()), 13, 3));
    }

    #[test]
    fn test_lexer_on_just_whitespace() {
        assert_eq!(
            Lexer::new("   \t  ").tokens,
            vec![Token::new(TokenKind::WhiteSpace("   \t  ".into()), 0, 6)]
        );
    }

    #[test]
    fn test_lexer_on_whitespace_at_end() {
        assert_eq!(
            Lexer::new("echo   \t  ").tokens,
            vec![
                Token::new(TokenKind::Word("echo".into()), 0, 4),
                Token::new(TokenKind::WhiteSpace("   \t  ".into()), 4, 6),
            ]
        );
    }

    #[test]
    fn test_preservers_whitespace_type() {
        let input = "echo \t \t    foo\n";

        let tokens = Lexer::new(input).tokens;
        println!("{:#?}", tokens);
        assert_eq!(
            tokens[1],
            Token::new(TokenKind::WhiteSpace(" \t \t    ".into()), 4, 8)
        );
        assert_eq!(tokens[3], Token::new(TokenKind::Newline, 15, 1));
    }

    #[test]
    fn test_lexer_on_backslash_1() {
        let tokens = Lexer::new(r#"echo "asd\"#).tokens;
        println!("{:#?}", tokens);
        assert_eq!(tokens[2], Token::new(TokenKind::Quote, 5, 1));
        assert_eq!(tokens[3], Token::new(TokenKind::Word("asd\\".into()), 6, 4));
    }

    #[test]
    fn test_lexer_on_backslash_2() {
        let tokens = Lexer::new(r#"echo "asd\ "#).tokens;
        println!("{:#?}", tokens);
        assert_eq!(tokens[2], Token::new(TokenKind::Quote, 5, 1));
        assert_eq!(
            tokens[3],
            Token::new(TokenKind::Word("asd\\ ".into()), 6, 5)
        );
    }

    #[test]
    fn test_lexer_on_backslash_3() {
        let tokens = Lexer::new(r#"echo \""#).tokens;
        println!("{:#?}", tokens);
        assert_eq!(tokens[2], Token::new(TokenKind::Word("\\\"".into()), 5, 2));
    }

    #[test]
    fn test_lexer_on_backslash_4() {
        let tokens = Lexer::new(r#"echo asd\ foo"#).tokens;
        println!("{:#?}", tokens);
        assert_eq!(
            tokens[1],
            Token::new(TokenKind::WhiteSpace(" ".into()), 4, 1)
        );
        assert_eq!(
            tokens[2],
            Token::new(TokenKind::Word("asd\\ foo".into()), 5, 8)
        );
    }

    #[test]
    fn test_lexer_on_backslash_5() {
        let tokens = Lexer::new(r#"echo foo\"#).tokens;
        println!("{:#?}", tokens);
        assert_eq!(
            tokens[1],
            Token::new(TokenKind::WhiteSpace(" ".into()), 4, 1)
        );
        assert_eq!(tokens[2], Token::new(TokenKind::Word("foo\\".into()), 5, 4));
    }

    #[test]
    fn test_lexer_on_backslash_6() {
        let tokens = Lexer::new(r#"echo \"foo"#).tokens;
        println!("{:#?}", tokens);
        assert_eq!(
            tokens[2],
            Token::new(TokenKind::Word(r#"\"foo"#.into()), 5, 5)
        );
    }

    #[test]
    fn test_line_continuation() {
        let input = "ls \\\n-la";
        let tokens = Lexer::new(input).tokens;
        println!("{:#?}", tokens);
        assert_eq!(tokens[0], Token::new(TokenKind::Word("ls".into()), 0, 2));
        assert_eq!(
            tokens[1],
            Token::new(TokenKind::WhiteSpace(" ".into()), 2, 1)
        );
        // assert_eq!(tokens[2], Token::new(TokenKind::, 3, 2));
        assert_eq!(tokens[5], Token::new(TokenKind::Word("-la".into()), 10, 3));
        assert_eq!(tokens[6], Token::new(TokenKind::RParen, 13, 1));
    }

    // TODO: try comments. ensure # isnt skipped
}
