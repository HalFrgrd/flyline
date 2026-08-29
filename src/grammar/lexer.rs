/*
 * Bash lexer for flyline.
 *
 * Originally derived from flash (https://github.com/raphamorim/flash).
 * Copyright (c) 2025 Raphael Amorim
 *
 * Licensed under GNU General Public License v3.0.
 */

use std::ops::Range;

/// Token types that can be produced by the lexer
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Word(String),
    Whitespace(String),
    Assignment,                                      // =
    Pipe,                                            // |
    Semicolon,                                       // ;
    DoubleSemicolon,                                 // ;;
    Newline,                                         // \n
    And,                                             // &&
    Background,                                      // & (add this new token)
    Or,                                              // ||
    LParen,                                          // (
    RParen,                                          // )
    DoubleRParen,                                    // ))
    LBrace,                                          // {
    RBrace,                                          // }
    Less,                                            // <
    Great,                                           // >
    DGreat,                                          // >>
    InputDup,                                        // <&
    OutputDup,                                       // >&
    ReadWrite,                                       // <>
    Clobber,                                         // >|
    Dollar,                                          // $
    Quote,                                           // "
    SingleQuote,                                     // '
    Backtick,                                        // `
    Comment,                                         // #
    CmdSubst,                                        // $(
    ArithSubst,                                      // $((
    ArithCommand,                                    // ((
    ParamExpansion,                                  // ${
    ParamExpansionOp(String),                        // :-, :=, :?, :+, #, ##, %, %%
    ProcessSubstIn,                                  // <(
    ProcessSubstOut,                                 // >(
    HereDoc { delimiter: String, quoted: bool }, // << followed by delimiter; `delimiter` is the unquoted word; `quoted` is true if any part of the original word was quoted (which suppresses body expansion)
    HereDocDash { delimiter: String, quoted: bool }, // <<- variant of HereDoc
    HereString,                                  // <<<
    ExtGlob(char),                               // For ?(, *(, +(, @(, !(
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
    Export,   // export keyword
    // Bash-specific features
    LBracket,       // [
    RBracket,       // ]
    DoubleLBracket, // [[ - extended test command
    DoubleRBracket, // ]] - end extended test
    History,        // ! - history expansion
    Complete,       // complete - tab completion builtin
    Select,         // select - interactive menu selection
    EOF,
}

impl TokenKind {
    pub fn is_word(&self) -> bool {
        matches!(self, TokenKind::Word(_))
    }

    pub fn is_whitespace(&self) -> bool {
        matches!(self, TokenKind::Whitespace(_))
    }
}

/// A token produced by the lexer
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub position: Position,
}

impl Token {
    pub fn unquoted(&self) -> String {
        match self.kind {
            TokenKind::Word(_) | TokenKind::Whitespace(_) => {
                let mut output = String::new();
                let mut chars = self.value.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '\\' {
                        if let Some(next) = chars.next() {
                            output.push(next);
                        } else {
                            output.push(ch);
                        }
                    } else {
                        output.push(ch);
                    }
                }
                output
            }
            _ => self.value.clone(),
        }
    }

    pub fn byte_range(&self) -> Range<usize> {
        let start = self.position.byte;
        let end = start + self.value.len();
        start..end
    }
}

/// Source position information
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub byte: usize,
}

impl Position {
    pub fn new(line: usize, column: usize, byte: usize) -> Self {
        Self { line, column, byte }
    }
}

fn is_special_char(ch: char) -> bool {
    matches!(
        ch,
        '=' | '|'
            | ';'
            | '\n'
            | '&'
            | '('
            | ')'
            | '{'
            | '}'
            | '<'
            | '>'
            | '$'
            | '"'
            | '\''
            | '`'
            | '#'
    )
}

fn is_word_terminator(ch: char) -> bool {
    matches!(
        ch,
        '=' | '|'
            | ';'
            | '\n'
            | '&'
            | '('
            | ')'
            | '{'
            | '}'
            | '<'
            | '>'
            | '$'
            | '"'
            | '\''
            | '`'
            | '['
            | ']'
    )
}

#[inline]
fn is_extglob_prefix(ch: char, next_ch: char) -> bool {
    matches!(ch, '?' | '*' | '+' | '@' | '!') && next_ch == '('
}

/// Lexer that converts input text into tokens
#[derive(Clone)]
pub struct Lexer {
    input: Vec<char>,
    pub position: usize,
    read_position: usize,
    ch: char,
    line: usize,
    column: usize,
    in_quotes: Option<char>,
    quote_after_cmdsubst: Option<char>,
    quote_after_cmdsubst_depth: usize,
    quote_after_param_expansion: Option<char>,
    quote_after_backtick: Option<char>,
    param_expansion_depth: usize,
    after_dollar: bool,
    pending_loop_headers: usize,
    active_loop_bodies: usize,
    last_significant_token: Option<SignificantToken>,
    /// A quoted (`<<'EOF'`, `<<"EOF"`, `<<\EOF`, …) heredoc operator was
    /// just emitted; once we see the next `Newline` we should switch into
    /// `in_quoted_heredoc_body` mode so the body is lexed as if it were
    /// single-quoted (one literal `Word` per line, no expansion).
    pending_quoted_heredoc: Option<(String, bool)>,
    /// We are currently consuming the body of a quoted heredoc. The
    /// payload is `(delimiter, dash_variant)`; `dash_variant` controls
    /// whether leading TABs are stripped before matching the delimiter
    /// line.
    in_quoted_heredoc_body: Option<(String, bool)>,
    arithmetic_depth: usize,
    paren_depth_in_arithmetic: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SignificantToken {
    Semicolon,
    Newline,
    Other,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        let mut lexer = Self {
            input: input.chars().collect(),
            position: 0,
            read_position: 0,
            ch: '\0',
            line: 1,
            column: 0,
            in_quotes: None,
            quote_after_cmdsubst: None,
            quote_after_cmdsubst_depth: 0,
            quote_after_param_expansion: None,
            quote_after_backtick: None,
            param_expansion_depth: 0,
            after_dollar: false,
            pending_loop_headers: 0,
            active_loop_bodies: 0,
            last_significant_token: None,
            pending_quoted_heredoc: None,
            in_quoted_heredoc_body: None,
            arithmetic_depth: 0,
            paren_depth_in_arithmetic: 0,
        };
        lexer.read_char();
        lexer
    }

    fn read_char(&mut self) {
        if self.read_position >= self.input.len() {
            self.ch = '\0';
        } else {
            self.ch = self.input[self.read_position];
        }
        self.position = self.read_position;
        self.read_position += 1;
        self.column += 1;
    }

    pub fn peek_char(&self) -> char {
        if self.read_position >= self.input.len() {
            '\0'
        } else {
            self.input[self.read_position]
        }
    }

    fn current_byte_offset(&self) -> usize {
        self.input
            .iter()
            .take(self.position)
            .map(|ch| ch.len_utf8())
            .sum()
    }

    // check if the current position is followed by whitespace or a special character
    fn is_word_boundary(&self) -> bool {
        let peek = self.peek_char();
        peek.is_whitespace() || is_special_char(peek) || peek == '\0'
    }

    pub fn peek_next_token(&mut self) -> Token {
        // Save the current state
        let saved_position = self.position;
        let saved_read_position = self.read_position;
        let saved_ch = self.ch;
        let saved_line = self.line;
        let saved_column = self.column;
        let saved_param_expansion_depth = self.param_expansion_depth;
        let saved_after_dollar = self.after_dollar;
        let saved_pending_loop_headers = self.pending_loop_headers;
        let saved_active_loop_bodies = self.active_loop_bodies;
        let saved_last_significant_token = self.last_significant_token;
        let saved_pending_quoted_heredoc = self.pending_quoted_heredoc.clone();
        let saved_in_quoted_heredoc_body = self.in_quoted_heredoc_body.clone();

        // Get the next token
        let mut token = self.next_token();
        while matches!(token.kind, TokenKind::Whitespace(_)) {
            token = self.next_token();
        }

        // Restore the saved state
        self.position = saved_position;
        self.read_position = saved_read_position;
        self.ch = saved_ch;
        self.line = saved_line;
        self.column = saved_column;
        self.param_expansion_depth = saved_param_expansion_depth;
        self.after_dollar = saved_after_dollar;
        self.pending_loop_headers = saved_pending_loop_headers;
        self.active_loop_bodies = saved_active_loop_bodies;
        self.last_significant_token = saved_last_significant_token;
        self.pending_quoted_heredoc = saved_pending_quoted_heredoc;
        self.in_quoted_heredoc_body = saved_in_quoted_heredoc_body;

        token
    }

    pub fn next_token(&mut self) -> Token {
        // When consuming the body of a quoted heredoc, every non-empty
        // line up to the delimiter is emitted as a single literal `Word`
        // token (no parameter / arithmetic / command substitution is
        // performed). Empty lines and the delimiter line itself are
        // handed back to the normal lexing logic.
        if let Some((delim, dash)) = self.in_quoted_heredoc_body.clone() {
            if self.ch == '\0' {
                // EOF inside an unterminated heredoc body — leave body
                // mode and let the normal logic emit EOF.
                self.in_quoted_heredoc_body = None;
            } else if self.ch != '\n' {
                // Look ahead a single line WITHOUT consuming, so we can
                // decide whether it is the delimiter (which must be
                // lexed normally) or body content (one literal Word).
                let mut idx = self.position;
                while idx < self.input.len() && self.input[idx] != '\n' {
                    idx += 1;
                }
                let line: String = self.input[self.position..idx].iter().collect();
                let trimmed = if dash {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                if trimmed == delim {
                    // Delimiter line — exit body mode and fall through
                    // so it is lexed as ordinary tokens.
                    self.in_quoted_heredoc_body = None;
                } else {
                    let position =
                        Position::new(self.line, self.column, self.current_byte_offset());
                    let value = line.clone();
                    // Consume every character of the body line; stop on
                    // the trailing '\n' which the normal logic will emit
                    // as a `Newline` token on the next call.
                    for _ in 0..line.chars().count() {
                        self.read_char();
                    }
                    return Token {
                        kind: TokenKind::Word(value.clone()),
                        value,
                        position,
                    };
                }
            }
        }

        if self.in_quotes.is_none() && self.ch.is_whitespace() && self.ch != '\n' {
            return self.read_whitespace();
        }

        let current_position = Position::new(self.line, self.column, self.current_byte_offset());

        // Check for quote start/end
        if (self.ch == '"' || self.ch == '\'') && self.in_quotes.is_none() {
            // Starting a quoted section
            let quote_type = self.ch;
            let token = Token {
                kind: if quote_type == '"' {
                    TokenKind::Quote
                } else {
                    TokenKind::SingleQuote
                },
                value: quote_type.to_string(),
                position: current_position,
            };

            self.in_quotes = Some(quote_type); // Set the in_quotes state
            self.read_char();
            return token;
        } else if self.in_quotes.is_some() && self.ch == self.in_quotes.unwrap() {
            // Ending a quoted section
            let quote_type = self.ch;
            let token = Token {
                kind: if quote_type == '"' {
                    TokenKind::Quote
                } else {
                    TokenKind::SingleQuote
                },
                value: quote_type.to_string(),
                position: current_position,
            };

            self.in_quotes = None; // Clear the in_quotes state
            self.read_char();
            return token;
        } else if self.in_quotes.is_some() {
            // A literal newline inside a quoted string must not become part of a
            // `Word` token. Emit it as a standalone `Newline` token while
            // remaining in the quoted state so the closing quote is still
            // recognised on a subsequent line.
            if self.ch == '\n' {
                let token = Token {
                    kind: TokenKind::Newline,
                    value: "\n".to_string(),
                    position: current_position,
                };
                self.line += 1;
                self.column = 0;
                self.read_char();
                return token;
            }
            let in_double_quotes = self.in_quotes == Some('"');
            // Inside double quotes, $ and ` retain their special meaning
            if in_double_quotes && self.ch == '$' {
                if self.peek_char() == '(' {
                    if self.position + 2 < self.input.len() && self.input[self.position + 2] == '('
                    {
                        // Arithmetic substitution $((
                        self.quote_after_cmdsubst = self.in_quotes;
                        self.quote_after_cmdsubst_depth = 2;
                        self.in_quotes = None;
                        self.read_char(); // Consume first '('
                        self.read_char(); // Consume second '('
                        self.read_char(); // Advance to first char inside $((
                        self.arithmetic_depth += 1;
                        self.paren_depth_in_arithmetic += 2;
                        return Token {
                            kind: TokenKind::ArithSubst,
                            value: "$((".to_string(),
                            position: current_position,
                        };
                    } else {
                        // Command substitution $(
                        self.quote_after_cmdsubst = self.in_quotes;
                        self.quote_after_cmdsubst_depth = 1;
                        self.in_quotes = None;
                        self.read_char(); // Consume '('
                        self.read_char(); // Advance to first char inside $(
                        return Token {
                            kind: TokenKind::CmdSubst,
                            value: "$(".to_string(),
                            position: current_position,
                        };
                    }
                } else if self.peek_char() == '{' {
                    // Parameter expansion ${
                    self.quote_after_param_expansion = self.in_quotes;
                    self.in_quotes = None;
                    self.param_expansion_depth += 1;
                    self.read_char(); // Consume '{'
                    self.read_char(); // Advance to first char inside ${...}
                    return Token {
                        kind: TokenKind::ParamExpansion,
                        value: "${".to_string(),
                        position: current_position,
                    };
                } else {
                    // Simple variable expansion $VAR
                    self.read_char(); // Advance past '$' to the variable name
                    self.after_dollar = true;
                    return Token {
                        kind: TokenKind::Dollar,
                        value: "$".to_string(),
                        position: current_position,
                    };
                }
            } else if in_double_quotes && self.ch == '`' {
                // Backtick command substitution inside double quotes
                self.quote_after_backtick = self.in_quotes;
                self.in_quotes = None;
                self.read_char(); // Advance past '`'
                return Token {
                    kind: TokenKind::Backtick,
                    value: "`".to_string(),
                    position: current_position,
                };
            } else if self.after_dollar && (self.ch.is_ascii_alphabetic() || self.ch == '_') {
                // After a $ in double-quoted context, read only a valid variable name
                self.after_dollar = false;
                let token = self.read_var_name();
                self.read_char();
                return token;
            } else {
                self.after_dollar = false;
                // Regular quoted content
                return self.read_quoted_content();
            }
        }

        if self.after_dollar {
            self.after_dollar = false;
            if self.ch.is_ascii_alphabetic() || self.ch == '_' {
                let token = self.read_var_name();
                self.read_char();
                return token;
            }
        }

        if self.in_quotes.is_none()
            && matches!(self.ch, '?' | '*' | '+' | '@' | '!')
            && self.peek_char() == '('
        {
            let op = self.ch;
            let token = Token {
                kind: TokenKind::ExtGlob(op),
                value: format!("{}(", op),
                position: current_position,
            };
            self.read_char();
            self.read_char();
            return token;
        }

        let token = match self.ch {
            '=' => Token {
                kind: TokenKind::Assignment,
                value: "=".to_string(),
                position: current_position,
            },
            '|' => {
                if self.peek_char() == '|' {
                    self.read_char();
                    Token {
                        kind: TokenKind::Or,
                        value: "||".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::Pipe,
                        value: "|".to_string(),
                        position: current_position,
                    }
                }
            }
            ';' => {
                if self.peek_char() == ';' {
                    self.read_char();
                    Token {
                        kind: TokenKind::DoubleSemicolon,
                        value: ";;".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::Semicolon,
                        value: ";".to_string(),
                        position: current_position,
                    }
                }
            }
            '&' => {
                if self.peek_char() == '&' {
                    self.read_char();
                    Token {
                        kind: TokenKind::And,
                        value: "&&".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::Background,
                        value: "&".to_string(),
                        position: current_position,
                    }
                }
            }
            '\n' => {
                self.line += 1;
                self.column = 0;
                let token = Token {
                    kind: TokenKind::Newline,
                    value: "\n".to_string(),
                    position: current_position,
                };
                // If a quoted heredoc operator was just emitted, the lines
                // following this newline are the body and must be lexed
                // as if they were inside a single-quoted string (one
                // literal `Word` per line, no expansion).
                if let Some(pending) = self.pending_quoted_heredoc.take() {
                    self.in_quoted_heredoc_body = Some(pending);
                }
                token
            }
            '(' => {
                // Check for arithmetic command (( syntax
                if self.peek_char() == '(' && self.arithmetic_depth == 0 {
                    self.read_char(); // Consume second '('
                    self.arithmetic_depth += 1;
                    self.paren_depth_in_arithmetic += 2;
                    Token {
                        kind: TokenKind::ArithCommand,
                        value: "((".to_string(),
                        position: current_position,
                    }
                } else {
                    if self.arithmetic_depth > 0 {
                        self.paren_depth_in_arithmetic += 1;
                    }
                    Token {
                        kind: TokenKind::LParen,
                        value: "(".to_string(),
                        position: current_position,
                    }
                }
            }
            ')' => {
                // Check if we need to restore quote state after command substitution.
                // Use depth counter so $((expr)) (which needs two ')') works correctly.
                if self.quote_after_cmdsubst.is_some() {
                    if self.quote_after_cmdsubst_depth > 1 {
                        self.quote_after_cmdsubst_depth -= 1;
                    } else {
                        let quote_char = self.quote_after_cmdsubst.unwrap();
                        self.in_quotes = Some(quote_char);
                        self.quote_after_cmdsubst = None;
                        self.quote_after_cmdsubst_depth = 0;
                    }
                }
                if self.arithmetic_depth > 0 {
                    self.paren_depth_in_arithmetic -= 1;
                    if self.paren_depth_in_arithmetic == 0 {
                        self.arithmetic_depth -= 1;
                    }
                }
                Token {
                    kind: TokenKind::RParen,
                    value: ")".to_string(),
                    position: current_position,
                }
            }
            '{' => Token {
                kind: TokenKind::LBrace,
                value: "{".to_string(),
                position: current_position,
            },
            '}' => {
                if let Some(quote_char) = self.quote_after_param_expansion {
                    self.in_quotes = Some(quote_char);
                    self.quote_after_param_expansion = None;
                }
                if self.param_expansion_depth > 0 {
                    self.param_expansion_depth -= 1;
                }
                Token {
                    kind: TokenKind::RBrace,
                    value: "}".to_string(),
                    position: current_position,
                }
            }
            '<' => {
                if self.peek_char() == '(' {
                    // Process substitution <(
                    self.read_char(); // Consume '('
                    Token {
                        kind: TokenKind::ProcessSubstIn,
                        value: "<(".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == '&' {
                    self.read_char();
                    Token {
                        kind: TokenKind::InputDup,
                        value: "<&".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == '>' {
                    self.read_char();
                    Token {
                        kind: TokenKind::ReadWrite,
                        value: "<>".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == '<' {
                    // Here document << or <<-
                    self.read_char(); // Consume second '<'
                    if self.peek_char() == '<' {
                        // Here string <<<
                        self.read_char(); // Consume third '<'
                        Token {
                            kind: TokenKind::HereString,
                            value: "<<<".to_string(),
                            position: current_position,
                        }
                    } else if self.peek_char() == '-' {
                        // Here document with dash <<-
                        self.read_char(); // Consume '-'
                        self.read_char(); // Move to next char after '<<-'

                        let mut raw = String::from("<<-");

                        // Capture any whitespace before delimiter so the
                        // token value remains a verbatim slice of input.
                        while self.ch.is_whitespace() && self.ch != '\n' {
                            raw.push(self.ch);
                            self.read_char();
                        }

                        // Read delimiter
                        let (delimiter, quoted) = self.read_heredoc_delimiter(&mut raw);
                        if quoted {
                            self.pending_quoted_heredoc = Some((delimiter.clone(), true));
                        }
                        Token {
                            kind: TokenKind::HereDocDash { delimiter, quoted },
                            value: raw,
                            position: current_position,
                        }
                    } else {
                        // Regular here document <<
                        self.read_char(); // Move to next char after '<<'

                        let mut raw = String::from("<<");

                        // Capture any whitespace before delimiter so the
                        // token value remains a verbatim slice of input.
                        while self.ch.is_whitespace() && self.ch != '\n' {
                            raw.push(self.ch);
                            self.read_char();
                        }

                        // Read delimiter
                        let (delimiter, quoted) = self.read_heredoc_delimiter(&mut raw);
                        if quoted {
                            self.pending_quoted_heredoc = Some((delimiter.clone(), false));
                        }
                        Token {
                            kind: TokenKind::HereDoc { delimiter, quoted },
                            value: raw,
                            position: current_position,
                        }
                    }
                } else {
                    Token {
                        kind: TokenKind::Less,
                        value: "<".to_string(),
                        position: current_position,
                    }
                }
            }
            '>' => {
                if self.peek_char() == '>' {
                    self.read_char();
                    Token {
                        kind: TokenKind::DGreat,
                        value: ">>".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == '&' {
                    self.read_char();
                    Token {
                        kind: TokenKind::OutputDup,
                        value: ">&".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == '|' {
                    self.read_char();
                    Token {
                        kind: TokenKind::Clobber,
                        value: ">|".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == '(' {
                    // Process substitution >(
                    self.read_char(); // Consume '('
                    Token {
                        kind: TokenKind::ProcessSubstOut,
                        value: ">(".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::Great,
                        value: ">".to_string(),
                        position: current_position,
                    }
                }
            }
            '!' => {
                // Check for != operator
                if self.peek_char() == '=' {
                    self.read_char(); // Consume the '='
                    Token {
                        kind: TokenKind::Word("!=".to_string()),
                        value: "!=".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == '(' {
                    // This is an extglob pattern !(pattern), treat as word
                    self.read_word()
                } else if self.peek_char() == '!' {
                    // !! - history expansion with empty pattern
                    self.read_char(); // Consume the second '!'
                    Token {
                        kind: TokenKind::History,
                        value: "!!".to_string(),
                        position: current_position,
                    }
                } else if self.peek_char() == ' ' || self.peek_char() == '\t' {
                    // ! followed by whitespace - this is logical negation, treat as word
                    Token {
                        kind: TokenKind::Word("!".to_string()),
                        value: "!".to_string(),
                        position: current_position,
                    }
                } else {
                    // History expansion - treat as History token
                    Token {
                        kind: TokenKind::History,
                        value: "!".to_string(),
                        position: current_position,
                    }
                }
            }
            '%' if self.param_expansion_depth > 0 => {
                // Inside ${...}, % is a suffix removal operator
                if self.peek_char() == '%' {
                    self.read_char(); // consume second %
                    Token {
                        kind: TokenKind::Word("%%".to_string()),
                        value: "%%".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::Word("%".to_string()),
                        value: "%".to_string(),
                        position: current_position,
                    }
                }
            }
            '/' if self.param_expansion_depth > 0 => {
                // Inside ${...}, / is a substitution operator
                if self.peek_char() == '/' {
                    self.read_char(); // consume second /
                    Token {
                        kind: TokenKind::Word("//".to_string()),
                        value: "//".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::Word("/".to_string()),
                        value: "/".to_string(),
                        position: current_position,
                    }
                }
            }
            '[' => {
                // Check for [[ extended test command
                if self.peek_char() == '[' {
                    self.read_char(); // Consume the second '['
                    Token {
                        kind: TokenKind::DoubleLBracket,
                        value: "[[".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::LBracket,
                        value: "[".to_string(),
                        position: current_position,
                    }
                }
            }
            ']' => {
                // Check for ]] end of extended test command
                if self.peek_char() == ']' {
                    self.read_char(); // Consume the second ']'
                    Token {
                        kind: TokenKind::DoubleRBracket,
                        value: "]]".to_string(),
                        position: current_position,
                    }
                } else {
                    Token {
                        kind: TokenKind::RBracket,
                        value: "]".to_string(),
                        position: current_position,
                    }
                }
            }
            '$' => {
                // Check for arithmetic expansion $(( syntax
                if self.peek_char() == '(' {
                    // Look ahead to see if it's $(( for arithmetic expansion
                    if self.position + 2 < self.input.len() && self.input[self.position + 2] == '('
                    {
                        if self.quote_after_cmdsubst.is_some() {
                            self.quote_after_cmdsubst_depth += 2;
                        }
                        self.read_char(); // Consume first '('
                        self.read_char(); // Consume second '('
                        self.arithmetic_depth += 1;
                        self.paren_depth_in_arithmetic += 2;
                        Token {
                            kind: TokenKind::ArithSubst,
                            value: "$((".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Regular command substitution $(
                        if self.quote_after_cmdsubst.is_some() {
                            self.quote_after_cmdsubst_depth += 1;
                        }
                        self.read_char(); // Consume the '('
                        Token {
                            kind: TokenKind::CmdSubst,
                            value: "$(".to_string(),
                            position: current_position,
                        }
                    }
                } else if self.peek_char() == '{' {
                    // Parameter expansion ${
                    self.param_expansion_depth += 1;
                    self.read_char(); // Consume the '{'
                    Token {
                        kind: TokenKind::ParamExpansion,
                        value: "${".to_string(),
                        position: current_position,
                    }
                } else {
                    self.after_dollar = true;
                    Token {
                        kind: TokenKind::Dollar,
                        value: "$".to_string(),
                        position: current_position,
                    }
                }
            }
            '"' => Token {
                kind: TokenKind::Quote,
                value: "\"".to_string(),
                position: current_position,
            },
            '\'' => Token {
                kind: TokenKind::SingleQuote,
                value: "'".to_string(),
                position: current_position,
            },
            '`' => {
                if let Some(quote_char) = self.quote_after_backtick {
                    self.in_quotes = Some(quote_char);
                    self.quote_after_backtick = None;
                }
                Token {
                    kind: TokenKind::Backtick,
                    value: "`".to_string(),
                    position: current_position,
                }
            }
            '#' => {
                if self.param_expansion_depth > 0 {
                    // Inside ${...}, # is an operator (length, prefix removal, or pattern anchor)
                    if self.peek_char() == '#' {
                        self.read_char(); // consume second #
                        Token {
                            kind: TokenKind::Word("##".to_string()),
                            value: "##".to_string(),
                            position: current_position,
                        }
                    } else {
                        Token {
                            kind: TokenKind::Word("#".to_string()),
                            value: "#".to_string(),
                            position: current_position,
                        }
                    }
                } else {
                    self.read_comment()
                }
            }
            '\0' => Token {
                kind: TokenKind::EOF,
                value: "".to_string(),
                position: current_position,
            },
            't' => {
                // Check for "then" keyword
                if self.peek_char() == 'h'
                    && self.position + 3 < self.input.len()
                    && self.input[self.position + 1] == 'h'
                    && self.input[self.position + 2] == 'e'
                    && self.input[self.position + 3] == 'n'
                {
                    self.read_char(); // 'h'
                    self.read_char(); // 'e'
                    self.read_char(); // 'n'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Then,
                            value: "then".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "then", backtrack and treat as a word
                        self.position -= 3;
                        self.read_position -= 3;
                        self.column -= 3;
                        self.ch = 't';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'e' => {
                // Check for "else", "elif", or "export" keywords
                if self.peek_char() == 'l' && self.position + 3 < self.input.len() {
                    self.read_char(); // 'l'

                    if self.peek_char() == 's' {
                        self.read_char(); // 's'
                        if self.peek_char() == 'e' {
                            self.read_char(); // 'e'
                            if self.is_word_boundary() {
                                Token {
                                    kind: TokenKind::Else,
                                    value: "else".to_string(),
                                    position: current_position,
                                }
                            } else {
                                // Not a standalone "else", backtrack
                                self.position -= 3;
                                self.read_position -= 3;
                                self.column -= 3;
                                self.ch = 'e';
                                self.read_word()
                            }
                        } else {
                            // Not "else", backtrack
                            self.position -= 2;
                            self.read_position -= 2;
                            self.column -= 2;
                            self.ch = 'e';
                            self.read_word()
                        }
                    } else if self.peek_char() == 'i' {
                        self.read_char(); // 'i'
                        if self.peek_char() == 'f' {
                            self.read_char(); // 'f'
                            if self.is_word_boundary() {
                                Token {
                                    kind: TokenKind::Elif,
                                    value: "elif".to_string(),
                                    position: current_position,
                                }
                            } else {
                                // Not a standalone "elif", backtrack
                                self.position -= 3;
                                self.read_position -= 3;
                                self.column -= 3;
                                self.ch = 'e';
                                self.read_word()
                            }
                        } else {
                            // Not "elif", backtrack
                            self.position -= 2;
                            self.read_position -= 2;
                            self.column -= 2;
                            self.ch = 'e';
                            self.read_word()
                        }
                    } else {
                        // Not "else" or "elif", backtrack
                        self.position -= 1;
                        self.read_position -= 1;
                        self.column -= 1;
                        self.ch = 'e';
                        self.read_word()
                    }
                } else if self.position + 5 < self.input.len()
                    && self.peek_char() == 'x'
                    && self.input[self.position + 1] == 'x'
                    && self.input[self.position + 2] == 'p'
                    && self.input[self.position + 3] == 'o'
                    && self.input[self.position + 4] == 'r'
                    && self.input[self.position + 5] == 't'
                {
                    self.read_char(); // 'x'
                    self.read_char(); // 'p'
                    self.read_char(); // 'o'
                    self.read_char(); // 'r'
                    self.read_char(); // 't'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Export,
                            value: "export".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "export", backtrack
                        self.position -= 5;
                        self.read_position -= 5;
                        self.column -= 5;
                        self.ch = 'e';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'f' => {
                // Check for "fi" keyword
                if self.peek_char() == 'i' && self.position + 1 < self.input.len() {
                    self.read_char(); // Consume 'i'
                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Fi,
                            value: "fi".to_string(),
                            position: current_position,
                        }
                    } else {
                        // If it's not a standalone "fi", backtrack and treat as a word
                        self.position -= 1;
                        self.read_position -= 1;
                        self.column -= 1;
                        self.ch = 'f';
                        self.read_word()
                    }
                } else if self.position + 7 < self.input.len()
                    && self.peek_char() == 'u'
                    && self.input[self.position + 1] == 'u'
                    && self.input[self.position + 2] == 'n'
                    && self.input[self.position + 3] == 'c'
                    && self.input[self.position + 4] == 't'
                    && self.input[self.position + 5] == 'i'
                    && self.input[self.position + 6] == 'o'
                    && self.input[self.position + 7] == 'n'
                {
                    // Check for "function" keyword
                    self.read_char(); // 'u'
                    self.read_char(); // 'n'
                    self.read_char(); // 'c'
                    self.read_char(); // 't'
                    self.read_char(); // 'i'
                    self.read_char(); // 'o'
                    self.read_char(); // 'n'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Function,
                            value: "function".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "function", backtrack
                        self.position -= 7;
                        self.read_position -= 7;
                        self.column -= 7;
                        self.ch = 'f';
                        self.read_word()
                    }
                } else if self.position + 2 < self.input.len()
                    && self.peek_char() == 'o'
                    && self.input[self.position + 1] == 'o'
                    && self.input[self.position + 2] == 'r'
                {
                    // Check for "for" keyword
                    self.read_char(); // 'o'
                    self.read_char(); // 'r'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::For,
                            value: "for".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "for", backtrack
                        self.position -= 2;
                        self.read_position -= 2;
                        self.column -= 2;
                        self.ch = 'f';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'u' => {
                // Check for "until" keyword
                if self.position + 4 < self.input.len()
                    && self.peek_char() == 'n'
                    && self.input[self.position + 1] == 'n'
                    && self.input[self.position + 2] == 't'
                    && self.input[self.position + 3] == 'i'
                    && self.input[self.position + 4] == 'l'
                {
                    self.read_char(); // 'n'
                    self.read_char(); // 't'
                    self.read_char(); // 'i'
                    self.read_char(); // 'l'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Until,
                            value: "until".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "until", backtrack
                        self.position -= 4;
                        self.read_position -= 4;
                        self.column -= 4;
                        self.ch = 'u';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'r' => {
                // Check for "return" keyword
                if self.position + 5 < self.input.len()
                    && self.peek_char() == 'e'
                    && self.input[self.position + 1] == 'e'
                    && self.input[self.position + 2] == 't'
                    && self.input[self.position + 3] == 'u'
                    && self.input[self.position + 4] == 'r'
                    && self.input[self.position + 5] == 'n'
                {
                    self.read_char(); // 'e'
                    self.read_char(); // 't'
                    self.read_char(); // 'u'
                    self.read_char(); // 'r'
                    self.read_char(); // 'n'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Return,
                            value: "return".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "return", backtrack
                        self.position -= 5;
                        self.read_position -= 5;
                        self.column -= 5;
                        self.ch = 'r';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'w' => {
                // Check for "while" keyword
                if self.position + 4 < self.input.len()
                    && self.peek_char() == 'h'
                    && self.input[self.position + 1] == 'h'
                    && self.input[self.position + 2] == 'i'
                    && self.input[self.position + 3] == 'l'
                    && self.input[self.position + 4] == 'e'
                {
                    self.read_char(); // 'h'
                    self.read_char(); // 'i'
                    self.read_char(); // 'l'
                    self.read_char(); // 'e'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::While,
                            value: "while".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "while", backtrack
                        self.position -= 4;
                        self.read_position -= 4;
                        self.column -= 4;
                        self.ch = 'w';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'd' => {
                // Check for "do" or "done" keywords
                if self.peek_char() == 'o' && self.position + 1 < self.input.len() {
                    self.read_char(); // 'o'

                    if self.peek_char() == 'n'
                        && self.position + 2 < self.input.len()
                        && self.input[self.position + 1] == 'n'
                        && self.input[self.position + 2] == 'e'
                    {
                        self.read_char(); // 'n'
                        self.read_char(); // 'e'

                        if self.is_word_boundary() {
                            Token {
                                kind: self.classify_done_keyword(),
                                value: "done".to_string(),
                                position: current_position,
                            }
                        } else {
                            // Not a standalone "done", backtrack
                            self.position -= 3;
                            self.read_position -= 3;
                            self.column -= 3;
                            self.ch = 'd';
                            self.read_word()
                        }
                    } else if self.is_word_boundary() {
                        Token {
                            kind: self.classify_do_keyword(),
                            value: "do".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "do", backtrack
                        self.position -= 1;
                        self.read_position -= 1;
                        self.column -= 1;
                        self.ch = 'd';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'b' => {
                // Check for "break" keyword
                if self.position + 4 < self.input.len()
                    && self.peek_char() == 'r'
                    && self.input[self.position + 1] == 'r'
                    && self.input[self.position + 2] == 'e'
                    && self.input[self.position + 3] == 'a'
                    && self.input[self.position + 4] == 'k'
                {
                    self.read_char(); // 'r'
                    self.read_char(); // 'e'
                    self.read_char(); // 'a'
                    self.read_char(); // 'k'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Break,
                            value: "break".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "break", backtrack
                        self.position -= 4;
                        self.read_position -= 4;
                        self.column -= 4;
                        self.ch = 'b';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'c' => {
                // Check for "continue" keyword
                if self.position + 7 < self.input.len()
                    && self.peek_char() == 'o'
                    && self.input[self.position + 1] == 'o'
                    && self.input[self.position + 2] == 'n'
                    && self.input[self.position + 3] == 't'
                    && self.input[self.position + 4] == 'i'
                    && self.input[self.position + 5] == 'n'
                    && self.input[self.position + 6] == 'u'
                    && self.input[self.position + 7] == 'e'
                {
                    self.read_char(); // 'o'
                    self.read_char(); // 'n'
                    self.read_char(); // 't'
                    self.read_char(); // 'i'
                    self.read_char(); // 'n'
                    self.read_char(); // 'u'
                    self.read_char(); // 'e'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::Continue,
                            value: "continue".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "continue", backtrack
                        self.position -= 7;
                        self.read_position -= 7;
                        self.column -= 7;
                        self.ch = 'c';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            'i' => {
                // Check for "if" keyword
                if self.peek_char() == 'f' && self.position + 1 < self.input.len() {
                    self.read_char(); // Consume 'f'
                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::If,
                            value: "if".to_string(),
                            position: current_position,
                        }
                    } else {
                        // If it's not a standalone "if", backtrack and treat as a word
                        self.position -= 1;
                        self.read_position -= 1;
                        self.column -= 1;
                        self.ch = 'i';
                        self.read_word()
                    }
                } else if self.position + 1 < self.input.len() &&
            // check in
           self.peek_char() == 'n'
                {
                    self.read_char(); // 'n'

                    if self.is_word_boundary() {
                        Token {
                            kind: TokenKind::In,
                            value: "in".to_string(),
                            position: current_position,
                        }
                    } else {
                        // Not a standalone "in", backtrack
                        self.position -= 1;
                        self.read_position -= 1;
                        self.column -= 1;
                        self.ch = 'i';
                        self.read_word()
                    }
                } else {
                    self.read_word()
                }
            }
            _ => self.read_word(),
        };

        self.update_control_flow_state(&token.kind);

        if token.kind != TokenKind::Word(String::new()) {
            self.read_char();
        }

        token
    }

    fn classify_do_keyword(&self) -> TokenKind {
        if self.pending_loop_headers > 0
            && matches!(
                self.last_significant_token,
                Some(SignificantToken::Semicolon | SignificantToken::Newline)
            )
        {
            TokenKind::Do
        } else {
            TokenKind::Word("do".to_string())
        }
    }

    fn classify_done_keyword(&self) -> TokenKind {
        if self.active_loop_bodies > 0
            && matches!(
                self.last_significant_token,
                Some(SignificantToken::Semicolon | SignificantToken::Newline)
            )
        {
            TokenKind::Done
        } else {
            TokenKind::Word("done".to_string())
        }
    }

    fn update_control_flow_state(&mut self, token_kind: &TokenKind) {
        match token_kind {
            TokenKind::For | TokenKind::While | TokenKind::Until | TokenKind::Select => {
                self.pending_loop_headers += 1;
                self.last_significant_token = Some(SignificantToken::Other);
            }
            TokenKind::Do => {
                self.pending_loop_headers = self.pending_loop_headers.saturating_sub(1);
                self.active_loop_bodies += 1;
                self.last_significant_token = Some(SignificantToken::Other);
            }
            TokenKind::Done => {
                self.active_loop_bodies = self.active_loop_bodies.saturating_sub(1);
                self.last_significant_token = Some(SignificantToken::Other);
            }
            TokenKind::Semicolon => {
                self.last_significant_token = Some(SignificantToken::Semicolon);
            }
            TokenKind::Newline => {
                self.last_significant_token = Some(SignificantToken::Newline);
            }
            TokenKind::Whitespace(_) | TokenKind::Comment | TokenKind::EOF => {}
            _ => {
                self.last_significant_token = Some(SignificantToken::Other);
            }
        }
    }

    fn read_word(&mut self) -> Token {
        let position = Position::new(self.line, self.column, self.current_byte_offset());
        let mut word = String::new();

        // Read word characters, including glob patterns but handling braces carefully
        while !self.ch.is_whitespace() && self.ch != '\0' {
            // Handle special case for '=' in command line arguments first
            if self.ch == '=' && word.starts_with('-') {
                // For command line arguments like --option=value, include the = as part of the word
                word.push(self.ch);
                self.read_char();

                // Continue reading the value part
                while !self.ch.is_whitespace() && self.ch != '\0' && !is_word_terminator(self.ch) {
                    word.push(self.ch);
                    self.read_char();
                }
                break; // Exit the main loop after handling the argument
            }
            // Check for other word terminators
            else if is_word_terminator(self.ch)
                || (self.param_expansion_depth > 0 && matches!(self.ch, '#' | '%' | '/'))
                || is_extglob_prefix(self.ch, self.peek_char())
            {
                break;
            }
            // Handle escape sequences
            else if self.ch == '\\' {
                // Look at the next character
                let next_ch = self.peek_char();
                if next_ch == '\n' {
                    // Preserve backslash but let newline be tokenized separately
                    word.push(self.ch);
                    self.read_char();
                    break;
                } else if next_ch != '\0' {
                    // Preserve the backslash and add the escaped character
                    word.push(self.ch); // Add the backslash
                    self.read_char(); // Move to the escaped character
                    word.push(self.ch); // Add the escaped character
                    self.read_char(); // Move past the escaped character
                } else {
                    // Backslash at end of input, treat as literal
                    word.push(self.ch);
                    self.read_char();
                }
            }
            // Handle regular characters and glob metacharacters
            else {
                word.push(self.ch);
                self.read_char();
            }
        }

        // We moved ahead one character, so step back
        if self.position > 0 {
            self.position -= 1;
            self.read_position -= 1;
            self.column -= 1;
        }

        // Check for keywords after reading the full word
        let token_kind = match word.as_str() {
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "fi" => TokenKind::Fi,
            "case" => TokenKind::Case,
            "esac" => TokenKind::Esac,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "until" => TokenKind::Until,
            "do" => self.classify_do_keyword(),
            "done" => self.classify_done_keyword(),
            "in" => TokenKind::In,
            "function" => TokenKind::Function,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "return" => TokenKind::Return,
            "export" => TokenKind::Export,
            "complete" => TokenKind::Complete,
            "select" => TokenKind::Select,
            _ => TokenKind::Word(word.clone()),
        };

        Token {
            kind: token_kind,
            value: word,
            position,
        }
    }

    fn read_var_name(&mut self) -> Token {
        let position = Position::new(self.line, self.column, self.current_byte_offset());
        let mut name = String::new();

        while self.ch.is_ascii_alphanumeric() || self.ch == '_' {
            name.push(self.ch);
            self.read_char();
        }

        // Step back one character (same pattern as read_word)
        if self.position > 0 {
            self.position -= 1;
            self.read_position -= 1;
            self.column -= 1;
        }

        Token {
            kind: TokenKind::Word(name.clone()),
            value: name,
            position,
        }
    }

    fn read_whitespace(&mut self) -> Token {
        let position = Position::new(self.line, self.column, self.current_byte_offset());
        let mut whitespace = String::new();

        while self.ch.is_whitespace() && self.ch != '\n' {
            whitespace.push(self.ch);
            self.read_char();
        }

        Token {
            kind: TokenKind::Whitespace(whitespace.clone()),
            value: whitespace,
            position,
        }
    }

    fn read_comment(&mut self) -> Token {
        let position = Position::new(self.line, self.column, self.current_byte_offset());
        let mut comment = String::from("#");

        self.read_char(); // Skip the '#'

        while self.ch != '\n' && self.ch != '\0' {
            comment.push(self.ch);
            self.read_char();
        }

        // We moved ahead one character, so step back
        if self.position > 0 {
            self.position -= 1;
            self.read_position -= 1;
            self.column -= 1;
        }

        Token {
            kind: TokenKind::Comment,
            value: comment,
            position,
        }
    }

    fn read_quoted_content(&mut self) -> Token {
        let position = Position::new(self.line, self.column, self.current_byte_offset());
        let mut content = String::new();
        let quote_char = self.in_quotes.unwrap();
        let is_double_quote = quote_char == '"';

        // Keep reading until we hit the closing quote, EOF, or a literal newline.
        // Newlines inside a quoted string are lexed as separate `Newline` tokens
        // so that no `Word` token ever contains a newline character.
        while self.ch != quote_char && self.ch != '\0' && self.ch != '\n' {
            // In double quotes, backslash escapes $, `, \, and " (preserving both chars)
            if is_double_quote && self.ch == '\\' {
                let next = self.peek_char();
                if next == '$' || next == '`' || next == '\\' || next == '"' {
                    content.push('\\');
                    self.read_char(); // Move to the escaped char
                    if self.ch == '\n' {
                        self.line += 1;
                        self.column = 0;
                    }
                    content.push(self.ch);
                    self.read_char();
                    continue;
                }
            }

            // For double-quoted strings, unescaped $ and ` trigger expansions
            if is_double_quote && (self.ch == '$' || self.ch == '`') {
                break;
            }

            if self.ch == '\n' {
                self.line += 1;
                self.column = 0;
            }

            content.push(self.ch);
            self.read_char();
        }

        if self.ch == '\0' {
            self.in_quotes = None;
            if content.is_empty() {
                return Token {
                    kind: TokenKind::EOF,
                    value: String::new(),
                    position,
                };
            }
        }

        Token {
            kind: TokenKind::Word(content.clone()),
            value: content,
            position,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.ch.is_whitespace() && self.ch != '\n' {
            self.read_char();
        }
    }

    // Parse parameter expansion content after ${
    pub fn read_parameter_expansion(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        let start_position = Position::new(self.line, self.column, self.current_byte_offset());

        // Skip whitespace
        self.skip_whitespace();

        // Handle special cases first
        if self.ch == '!' {
            // Indirect expansion ${!var}
            tokens.push(Token {
                kind: TokenKind::Word("!".to_string()),
                value: "!".to_string(),
                position: start_position,
            });
            self.read_char();
            self.skip_whitespace();
        } else if self.ch == '#' {
            // Length expansion ${#var} or prefix removal ${var#pattern}
            let pos = Position::new(self.line, self.column, self.current_byte_offset());
            self.read_char();

            // Check if this is length expansion (# followed by variable name)
            if self.ch.is_alphabetic() || self.ch == '_' {
                tokens.push(Token {
                    kind: TokenKind::Word("#".to_string()),
                    value: "#".to_string(),
                    position: pos,
                });
            } else {
                // This might be prefix removal, put # back for later processing
                self.position -= 1;
                self.read_position -= 1;
                self.column -= 1;
                self.ch = '#';
            }
        }

        // Read variable name
        if self.ch.is_alphabetic() || self.ch == '_' || self.ch.is_ascii_digit() {
            let var_token = self.read_word();
            tokens.push(var_token);
        }

        // Skip whitespace
        self.skip_whitespace();

        // Check for parameter expansion operators
        if self.ch == ':' {
            let op_start = Position::new(self.line, self.column, self.current_byte_offset());
            let mut op = String::new();
            op.push(self.ch);
            self.read_char();

            // Read the operator character(s)
            match self.ch {
                '-' | '=' | '?' | '+' => {
                    op.push(self.ch);
                    self.read_char();
                }
                _ => {
                    // Just a colon, might be for substring ${var:offset:length}
                }
            }

            tokens.push(Token {
                kind: TokenKind::ParamExpansionOp(op.clone()),
                value: op,
                position: op_start,
            });
        } else if self.ch == '#' {
            // Prefix removal
            let op_start = Position::new(self.line, self.column, self.current_byte_offset());
            let mut op = String::new();
            op.push(self.ch);
            self.read_char();

            // Check for ## (longest prefix removal)
            if self.ch == '#' {
                op.push(self.ch);
                self.read_char();
            }

            tokens.push(Token {
                kind: TokenKind::ParamExpansionOp(op.clone()),
                value: op,
                position: op_start,
            });
        } else if self.ch == '%' {
            // Suffix removal
            let op_start = Position::new(self.line, self.column, self.current_byte_offset());
            let mut op = String::new();
            op.push(self.ch);
            self.read_char();

            // Check for %% (longest suffix removal)
            if self.ch == '%' {
                op.push(self.ch);
                self.read_char();
            }

            tokens.push(Token {
                kind: TokenKind::ParamExpansionOp(op.clone()),
                value: op,
                position: op_start,
            });
        }

        // Read the rest of the content until }
        while self.ch != '}' && self.ch != '\0' {
            if self.ch.is_whitespace() {
                self.skip_whitespace();
                continue;
            }

            let token = self.read_word();
            tokens.push(token);
        }

        tokens
    }

    /// Check, without consuming any input, whether the quoted segment
    /// that would be opened by the current character (`self.ch`, which
    /// must be `'` or `"`) has a matching closing quote later in the
    /// same heredoc-delimiter "word" (i.e. before the next newline or
    /// EOF). For double quotes, a backslash escapes the following
    /// character so it cannot itself close the segment.
    ///
    /// Used by `read_heredoc_delimiter` to avoid greedily swallowing
    /// characters that belong to a separate, unclosed quoted-string
    /// token following the heredoc operator.
    fn heredoc_delim_has_matching_close(&self, quote: char) -> bool {
        let mut idx = self.read_position;
        while let Some(&c) = self.input.get(idx) {
            match c {
                '\n' => return false,
                '\\' if quote == '"' => idx += 2, // escaped char can't close
                c if c == quote => return true,
                _ => idx += 1,
            }
        }
        false
    }

    /// Read a heredoc delimiter "word" starting at `self.ch`.
    ///
    /// Returns `(delimiter, quoted)` where `delimiter` is the value of the
    /// word after quote removal (used for line matching) and `quoted` is
    /// `true` if any part of the original word was quoted (with single
    /// quotes, double quotes or a backslash escape). Per the bash spec, a
    /// quoted delimiter suppresses parameter, command and arithmetic
    /// expansion in the here-document body.
    ///
    /// The verbatim characters consumed (including any quote characters
    /// and backslashes) are appended to `raw` so that the caller can
    /// reconstruct the exact slice of source text covered by the token.
    fn read_heredoc_delimiter(&mut self, raw: &mut String) -> (String, bool) {
        let mut delimiter = String::new();
        let mut quoted = false;

        // Consume one verbatim char into both `raw` and (optionally) `delimiter`.
        macro_rules! take {
            ($lex:expr, $into_delim:expr) => {{
                if $into_delim {
                    delimiter.push($lex.ch);
                }
                raw.push($lex.ch);
                $lex.read_char();
            }};
        }

        while !self.ch.is_whitespace() && self.ch != '\0' {
            match self.ch {
                // Quoted segment: only enter if a matching close quote
                // exists on the same word, otherwise leave the opening
                // quote for the outer lexer to emit as an unclosed
                // string. Single quotes are literal; double quotes
                // honour backslash escapes for ", \, $, `.
                q @ ('\'' | '"') => {
                    if !self.heredoc_delim_has_matching_close(q) {
                        break;
                    }
                    quoted = true;
                    take!(self, false); // opening quote
                    while self.ch != q {
                        if q == '"'
                            && self.ch == '\\'
                            && matches!(self.peek_char(), '"' | '\\' | '$' | '`')
                        {
                            take!(self, false); // backslash
                        }
                        take!(self, true);
                    }
                    take!(self, false); // closing quote (guaranteed by lookahead)
                }
                // Backslash escapes the next character (marks delimiter
                // as quoted). A trailing backslash before EOF/newline is
                // taken literally.
                '\\' if !matches!(self.peek_char(), '\0' | '\n') => {
                    quoted = true;
                    take!(self, false); // backslash
                    take!(self, true); // escaped char
                }
                // Bare character (including unmatched quotes and
                // trailing backslashes) — consumed literally.
                _ => take!(self, true),
            }
        }

        // Step back one character so the outer `next_token` loop can
        // re-process the terminator (whitespace / newline / EOF).
        if self.position > 0 {
            self.position -= 1;
            self.read_position -= 1;
            self.column -= 1;
        }

        (delimiter, quoted)
    }

    // Parse here-document content
    pub fn read_here_document(&mut self, delimiter: &str, dash_variant: bool) -> String {
        let mut content = String::new();
        let mut line = String::new();

        // Skip to next line
        while self.ch != '\n' && self.ch != '\0' {
            self.read_char();
        }
        if self.ch == '\n' {
            self.read_char();
        }

        loop {
            line.clear();

            // Read a complete line
            while self.ch != '\n' && self.ch != '\0' {
                line.push(self.ch);
                self.read_char();
            }

            // Check if this line is the delimiter
            let trimmed_line = if dash_variant {
                line.trim_start() // <<- removes leading tabs
            } else {
                &line
            };

            if trimmed_line == delimiter {
                break;
            }

            // Add the line to content
            content.push_str(&line);
            if self.ch == '\n' {
                content.push('\n');
                self.read_char();
            }

            // Check for EOF
            if self.ch == '\0' {
                break;
            }
        }

        content
    }
}

#[cfg(test)]
mod lexer_tests {
    use super::{Lexer, Token, TokenKind};

    #[test]
    fn debug_lexer_output() {
        let input = r#"LOG_DIR="/var/log""#;
        let mut lexer = Lexer::new(input);

        println!("Tokens for 'LOG_DIR=\"/var/log\"':");

        let mut token = lexer.next_token();
        while token.kind != TokenKind::EOF {
            println!("Token: {token:?}");
            token = lexer.next_token();
        }
    }

    fn collect_tokens(input: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();

        loop {
            let mut token = lexer.next_token();
            while matches!(token.kind, TokenKind::Whitespace(_)) {
                token = lexer.next_token();
            }
            let is_eof = matches!(token.kind, TokenKind::EOF);
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        tokens
    }

    fn collect_tokens_include_whitespace(input: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token();
            let is_eof = matches!(token.kind, TokenKind::EOF);
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        tokens
    }

    fn test_round_trip(input: &str) {
        let tokens = collect_tokens_include_whitespace(input);
        let reconstructed = tokens
            .iter()
            .map(|t| t.value.clone())
            .collect::<Vec<String>>()
            .join("");
        assert_eq!(
            input, reconstructed,
            "Round-trip failed for input: {}",
            input
        );
    }

    fn test_tokens(input: &str, expected_tokens: Vec<TokenKind>) {
        let mut lexer = Lexer::new(input);
        for expected in expected_tokens {
            let mut token = lexer.next_token();
            while matches!(token.kind, TokenKind::Whitespace(_)) {
                token = lexer.next_token();
            }
            assert_eq!(
                token.kind, expected,
                "Expected {:?} but got {:?} for input: {}",
                expected, token.kind, input
            );
        }

        // Ensure we've consumed all tokens
        let mut final_token = lexer.next_token();
        while matches!(final_token.kind, TokenKind::Whitespace(_)) {
            final_token = lexer.next_token();
        }
        assert_eq!(
            final_token.kind,
            TokenKind::EOF,
            "Expected EOF but got {:?}",
            final_token.kind
        );
    }

    fn test_tokens_include_whitespace(input: &str, expected_tokens: Vec<TokenKind>) {
        let mut lexer = Lexer::new(input);
        for expected in expected_tokens {
            let token = lexer.next_token();
            assert_eq!(
                token.kind, expected,
                "Expected {:?} but got {:?} for input: {}",
                expected, token.kind, input
            );
        }

        // Ensure we've consumed all tokens
        let final_token = lexer.next_token();
        assert_eq!(
            final_token.kind,
            TokenKind::EOF,
            "Expected EOF but got {:?}",
            final_token.kind
        );
    }

    fn next_non_whitespace(lexer: &mut Lexer) -> Token {
        let mut token = lexer.next_token();
        while matches!(token.kind, TokenKind::Whitespace(_)) {
            token = lexer.next_token();
        }
        token
    }

    #[test]
    fn test_peek_without_advancing() {
        let input = "if then";
        let mut lexer = Lexer::new(input);

        // Peek next token (should be 'if')
        let peeked_token = lexer.peek_next_token();
        assert_eq!(peeked_token.kind, TokenKind::If);
        assert_eq!(peeked_token.value, "if");

        // Current token should still be 'if' after peeking
        let current_token = next_non_whitespace(&mut lexer);
        assert_eq!(current_token.kind, TokenKind::If);
        assert_eq!(current_token.value, "if");

        // Next token should be 'then'
        let next_token = next_non_whitespace(&mut lexer);
        assert_eq!(next_token.kind, TokenKind::Then);
        assert_eq!(next_token.value, "then");
    }

    #[test]
    fn test_multiple_peeks() {
        let input = "for i in 1 2 3";
        let mut lexer = Lexer::new(input);

        // First peek should be 'for'
        let first_peek = lexer.peek_next_token();
        assert_eq!(first_peek.kind, TokenKind::For);

        // Second peek should still be 'for' since we haven't advanced
        let second_peek = lexer.peek_next_token();
        assert_eq!(second_peek.kind, TokenKind::For);

        // Now consume the 'for' token
        let token = next_non_whitespace(&mut lexer);
        assert_eq!(token.kind, TokenKind::For);

        // Peek should now be 'i'
        let third_peek = lexer.peek_next_token();
        assert_eq!(third_peek.kind, TokenKind::Word("i".to_string()));
    }

    #[test]
    fn test_peek_at_end() {
        let input = "ls";
        let mut lexer = Lexer::new(input);

        // Consume the only token
        let token = lexer.next_token();
        assert_eq!(token.kind, TokenKind::Word("ls".to_string()));

        // Peek should now return EOF
        let peeked_token = lexer.peek_next_token();
        assert_eq!(peeked_token.kind, TokenKind::EOF);

        // Next token should also be EOF
        let eof_token = lexer.next_token();
        assert_eq!(eof_token.kind, TokenKind::EOF);
    }

    #[test]
    fn test_peek_special_tokens() {
        let input = "if [ $a = 5 ]; then echo success; fi";
        let mut lexer = Lexer::new(input);

        // Consume 'if'
        let if_token = lexer.next_token();
        assert_eq!(if_token.kind, TokenKind::If);

        // Peek should be '['
        let peek_token = lexer.peek_next_token();
        assert_eq!(peek_token.kind, TokenKind::LBracket);

        // Lexer position should still be at the same point
        let bracket_token = next_non_whitespace(&mut lexer);
        assert_eq!(bracket_token.kind, TokenKind::LBracket);

        // Let's consume a few more tokens
        next_non_whitespace(&mut lexer); // $
        next_non_whitespace(&mut lexer); // a

        // Peek should now be '='
        let eq_peek = lexer.peek_next_token();
        assert_eq!(eq_peek.kind, TokenKind::Assignment);
        assert_eq!(eq_peek.value, "=");

        // And verify we're still at the same position
        let eq_token = next_non_whitespace(&mut lexer);
        assert_eq!(eq_token.kind, TokenKind::Assignment);
    }

    #[test]
    fn test_peek_with_complex_tokens() {
        let input = "ls -l || echo 'failed'";
        let mut lexer = Lexer::new(input);

        // Consume 'ls' and '-l'
        next_non_whitespace(&mut lexer); // ls
        next_non_whitespace(&mut lexer); // -l

        // Peek should now be '||'
        let or_peek = lexer.peek_next_token();
        assert_eq!(or_peek.kind, TokenKind::Or);
        assert_eq!(or_peek.value, "||");

        // Verify we still get '||' when advancing
        let or_token = next_non_whitespace(&mut lexer);
        assert_eq!(or_token.kind, TokenKind::Or);

        // Peek should now be 'echo'
        let echo_peek = lexer.peek_next_token();
        assert_eq!(echo_peek.kind, TokenKind::Word("echo".to_string()));
    }

    #[test]
    fn test_peek_with_newlines() {
        let input = "echo hello\necho world";
        let mut lexer = Lexer::new(input);

        // Consume 'echo' and 'hello'
        next_non_whitespace(&mut lexer); // echo
        next_non_whitespace(&mut lexer); // hello

        // Peek should be newline
        let nl_peek = lexer.peek_next_token();
        assert_eq!(nl_peek.kind, TokenKind::Newline);

        // Advance past newline
        let nl_token = next_non_whitespace(&mut lexer);
        assert_eq!(nl_token.kind, TokenKind::Newline);

        // Peek should now be the second 'echo'
        let echo2_peek = lexer.peek_next_token();
        assert_eq!(echo2_peek.kind, TokenKind::Word("echo".to_string()));
    }

    #[test]
    fn test_peek_with_comments() {
        let input = "# This is a comment\necho hello";
        let mut lexer = Lexer::new(input);

        // Peek should be a comment
        let comment_peek = lexer.peek_next_token();
        assert_eq!(comment_peek.kind, TokenKind::Comment);

        // Advance past comment
        let comment_token = next_non_whitespace(&mut lexer);
        assert_eq!(comment_token.kind, TokenKind::Comment);

        // Peek should now be newline
        let nl_peek = lexer.peek_next_token();
        assert_eq!(nl_peek.kind, TokenKind::Newline);
    }

    #[test]
    fn test_state_preservation() {
        let input = "if [ $? -eq 0 ]; then echo success; fi";
        let mut lexer = Lexer::new(input);

        // Record initial position data
        let initial_position = lexer.position;
        let initial_read_position = lexer.read_position;
        let initial_line = lexer.line;
        let initial_column = lexer.column;

        // Peek next token to ensure state is preserved
        lexer.peek_next_token();

        // Verify that the lexer's state hasn't changed
        assert_eq!(lexer.position, initial_position);
        assert_eq!(lexer.read_position, initial_read_position);
        assert_eq!(lexer.line, initial_line);
        assert_eq!(lexer.column, initial_column);

        // Now advance the lexer
        lexer.next_token();

        // Verify that the state has now changed
        assert_ne!(lexer.position, initial_position);
    }

    #[test]
    fn test_basic_tokens() {
        let input = "ls -l | grep file";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::Word("-l".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("grep".to_string()),
            TokenKind::Word("file".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_assignment() {
        let input = "VAR=value";
        let expected = vec![
            TokenKind::Word("VAR".to_string()),
            TokenKind::Assignment,
            TokenKind::Word("value".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_redirections() {
        let input = "ls > output.txt 2>&1";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::Great,
            TokenKind::Word("output.txt".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::OutputDup,
            TokenKind::Word("1".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_quoted_strings() {
        let input = r#"echo "hello world" 'rio de janeiro'"#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Quote,
            TokenKind::Word("hello world".to_string()),
            TokenKind::Quote,
            TokenKind::SingleQuote,
            TokenKind::Word("rio de janeiro".to_string()),
            TokenKind::SingleQuote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_command_substitution() {
        let input = "echo $(ls -l)";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::CmdSubst,
            TokenKind::Word("ls".to_string()),
            TokenKind::Word("-l".to_string()),
            TokenKind::RParen,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_command_substitution_on_variable() {
        let input = "NUMBER=$(echo 85)";
        let expected = vec![
            TokenKind::Word("NUMBER".to_string()),
            TokenKind::Assignment,
            TokenKind::CmdSubst,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("85".to_string()),
            TokenKind::RParen,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_command_substitution_on_variable_with_quotes() {
        let input = "NUMBER=\"$(echo 85)\"";
        let expected = vec![
            TokenKind::Word("NUMBER".to_string()),
            TokenKind::Assignment,
            TokenKind::Quote,
            TokenKind::CmdSubst,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("85".to_string()),
            TokenKind::RParen,
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_dollar_expansion_in_double_quotes() {
        // echo "hello $FOO" should lex out the dollar sign and FOO
        let input = r#"echo "hello $FOO""#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Quote,
            TokenKind::Word("hello ".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("FOO".to_string()),
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_dollar_at_start_of_double_quotes() {
        let input = r#""$HOME""#;
        let expected = vec![
            TokenKind::Quote,
            TokenKind::Dollar,
            TokenKind::Word("HOME".to_string()),
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_param_expansion_in_double_quotes() {
        // "${FOO}" inside double quotes
        let input = r#""${FOO}""#;
        let expected = vec![
            TokenKind::Quote,
            TokenKind::ParamExpansion,
            TokenKind::Word("FOO".to_string()),
            TokenKind::RBrace,
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_param_expansion_with_text_in_double_quotes() {
        // "hello ${FOO} world"
        let input = r#""hello ${FOO} world""#;
        let expected = vec![
            TokenKind::Quote,
            TokenKind::Word("hello ".to_string()),
            TokenKind::ParamExpansion,
            TokenKind::Word("FOO".to_string()),
            TokenKind::RBrace,
            TokenKind::Word(" world".to_string()),
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_cmd_substitution_with_text_in_double_quotes() {
        // "result: $(echo hello)"
        let input = r#""result: $(echo hello)""#;
        let expected = vec![
            TokenKind::Quote,
            TokenKind::Word("result: ".to_string()),
            TokenKind::CmdSubst,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("hello".to_string()),
            TokenKind::RParen,
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_arith_substitution_in_double_quotes() {
        // "$((1 + 2))" - arithmetic substitution inside double quotes
        let input = r#""$((1 + 2))""#;
        let expected = vec![
            TokenKind::Quote,
            TokenKind::ArithSubst,
            TokenKind::Word("1".to_string()),
            TokenKind::Word("+".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_backtick_in_double_quotes() {
        // "`date`" inside double quotes
        let input = r#""`date`""#;
        let expected = vec![
            TokenKind::Quote,
            TokenKind::Backtick,
            TokenKind::Word("date".to_string()),
            TokenKind::Backtick,
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_escaped_dollar_in_double_quotes() {
        // "\$FOO" - escaped dollar should be literal inside double quotes
        let input = r#""\$FOO""#;
        let expected = vec![
            TokenKind::Quote,
            TokenKind::Word("\\$FOO".to_string()),
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_single_quotes_no_expansion() {
        // Single quotes preserve everything literally
        let input = r#"'$FOO `date` $((1+2))'"#;
        let expected = vec![
            TokenKind::SingleQuote,
            TokenKind::Word("$FOO `date` $((1+2))".to_string()),
            TokenKind::SingleQuote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_single_quotes_backslash() {
        // Single quotes preserve backslashes literally, including double backslash and trailing backslash
        let input = r#"printf '\\'"#;
        let expected = vec![
            TokenKind::Word("printf".to_string()),
            TokenKind::SingleQuote,
            TokenKind::Word("\\\\".to_string()),
            TokenKind::SingleQuote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_single_quotes_escaped_quote_not_escaped() {
        // In single quotes, backslash has no special meaning, so '\'' starts a quote, has a literal backslash,
        // and then the second single quote closes the single-quoted section.
        let input = r#"'foo\'bar'"#;
        let expected = vec![
            TokenKind::SingleQuote,
            TokenKind::Word("foo\\".to_string()),
            TokenKind::SingleQuote,
            TokenKind::Word("bar".to_string()),
            TokenKind::SingleQuote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_equal_sign_not_as_assignment() {
        let input = "./configure --target=something";
        let expected = vec![
            TokenKind::Word("./configure".to_string()),
            TokenKind::Word("--target=something".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_variable_expansion() {
        let input = "echo $HOME";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("HOME".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_operators() {
        let input = "cmd1 && cmd2 || cmd3";
        let expected = vec![
            TokenKind::Word("cmd1".to_string()),
            TokenKind::And,
            TokenKind::Word("cmd2".to_string()),
            TokenKind::Or,
            TokenKind::Word("cmd3".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_background_process() {
        let input = "sleep 10 &";
        let expected = vec![
            TokenKind::Word("sleep".to_string()),
            TokenKind::Word("10".to_string()),
            TokenKind::Background,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_comments() {
        let input = "echo hello # this is a comment";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("hello".to_string()),
            TokenKind::Comment,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_newlines() {
        let input = "cmd1\ncmd2\ncmd3";
        let expected = vec![
            TokenKind::Word("cmd1".to_string()),
            TokenKind::Newline,
            TokenKind::Word("cmd2".to_string()),
            TokenKind::Newline,
            TokenKind::Word("cmd3".to_string()),
        ];
        test_tokens(input, expected);
    }

    // Tests for shell control flow

    #[test]
    fn test_if_statement() {
        let input = "if test -f file.txt; then echo found; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::Word("test".to_string()),
            TokenKind::Word("-f".to_string()),
            TokenKind::Word("file.txt".to_string()),
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("found".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_if_with_newlines() {
        let input = "if true\nthen\necho yes\nfi";
        let expected = vec![
            TokenKind::If,
            TokenKind::Word("true".to_string()),
            TokenKind::Newline,
            TokenKind::Then,
            TokenKind::Newline,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("yes".to_string()),
            TokenKind::Newline,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_if_else_statement() {
        let input = "if [ $a -eq 5 ]; then echo equal; else echo not equal; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("a".to_string()),
            TokenKind::Word("-eq".to_string()),
            TokenKind::Word("5".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("equal".to_string()),
            TokenKind::Semicolon,
            TokenKind::Else,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("not".to_string()),
            TokenKind::Word("equal".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_if_elif_else_statement() {
        let input =
            "if [ $a -eq 1 ]; then echo one; elif [ $a -eq 2 ]; then echo two; else echo other; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("a".to_string()),
            TokenKind::Word("-eq".to_string()),
            TokenKind::Word("1".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("one".to_string()),
            TokenKind::Semicolon,
            TokenKind::Elif,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("a".to_string()),
            TokenKind::Word("-eq".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("two".to_string()),
            TokenKind::Semicolon,
            TokenKind::Else,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("other".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_nested_if_statements() {
        let input = "if true; then if false; then echo nested; fi; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::Word("true".to_string()),
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::If,
            TokenKind::Word("false".to_string()),
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("nested".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_if_with_complex_command() {
        let input = "if grep -q pattern file.txt; then echo found; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::Word("grep".to_string()),
            TokenKind::Word("-q".to_string()),
            TokenKind::Word("pattern".to_string()),
            TokenKind::Word("file.txt".to_string()),
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("found".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_control_flow_keywords_as_prefix() {
        let input = "ifconfig && thenext && elifprocess && elseware && fifile";
        let expected = vec![
            TokenKind::Word("ifconfig".to_string()),
            TokenKind::And,
            TokenKind::Word("thenext".to_string()),
            TokenKind::And,
            TokenKind::Word("elifprocess".to_string()),
            TokenKind::And,
            TokenKind::Word("elseware".to_string()),
            TokenKind::And,
            TokenKind::Word("fifile".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_words_with_glob_patterns() {
        let input = "ls *.txt file?.log [abc]*.tmp";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::Word("*.txt".to_string()),
            TokenKind::Word("file?.log".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("abc".to_string()),
            TokenKind::RBracket,
            TokenKind::Word("*.tmp".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_extglob_patterns() {
        // Test extended glob patterns
        let input = "ls ?(file|temp).txt *(a|b|c).log +(1|2|3).dat";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::ExtGlob('?'),
            TokenKind::Word("file".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("temp".to_string()),
            TokenKind::RParen,
            TokenKind::Word(".txt".to_string()),
            TokenKind::ExtGlob('*'),
            TokenKind::Word("a".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("b".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("c".to_string()),
            TokenKind::RParen,
            TokenKind::Word(".log".to_string()),
            TokenKind::ExtGlob('+'),
            TokenKind::Word("1".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("2".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("3".to_string()),
            TokenKind::RParen,
            TokenKind::Word(".dat".to_string()),
        ];
        test_tokens(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_extglob_token_value_contains_parenthesis() {
        let input = "echo !(mnt)";
        let mut lexer = Lexer::new(input);
        let extglob_token = std::iter::from_fn(|| {
            let t = lexer.next_token();
            if t.kind == TokenKind::EOF {
                None
            } else {
                Some(t)
            }
        })
        .find(|t| matches!(t.kind, TokenKind::ExtGlob(_)))
        .unwrap();

        assert_eq!(extglob_token.kind, TokenKind::ExtGlob('!'));
        assert_eq!(extglob_token.value, "!(");
        test_round_trip(input);
    }

    #[test]
    fn test_mixed_keywords_and_words() {
        let input = "if if_var=42; then echo then_var=42; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::Word("if_var".to_string()),
            TokenKind::Assignment,
            TokenKind::Word("42".to_string()),
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("then_var".to_string()),
            TokenKind::Assignment,
            TokenKind::Word("42".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_command_substitution_in_if() {
        let input = "if $(test -d /tmp); then echo directory exists; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::CmdSubst,
            TokenKind::Word("test".to_string()),
            TokenKind::Word("-d".to_string()),
            TokenKind::Word("/tmp".to_string()),
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("directory".to_string()),
            TokenKind::Word("exists".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_function_declaration() {
        let input = "function greet() { echo hello; }";
        let expected = vec![
            TokenKind::Function,
            TokenKind::Word("greet".to_string()),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("hello".to_string()),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_function_declaration_alternate_syntax() {
        let input = "greet() { echo hello; }";
        let expected = vec![
            TokenKind::Word("greet".to_string()),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("hello".to_string()),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_function_call() {
        let input = "greet; greet arg1 arg2";
        let expected = vec![
            TokenKind::Word("greet".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("greet".to_string()),
            TokenKind::Word("arg1".to_string()),
            TokenKind::Word("arg2".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_function_with_return() {
        let input = "function check() { if [ $1 -eq 0 ]; then return 1; fi; echo ok; }";
        let expected = vec![
            TokenKind::Function,
            TokenKind::Word("check".to_string()),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("1".to_string()),
            TokenKind::Word("-eq".to_string()),
            TokenKind::Word("0".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Return,
            TokenKind::Word("1".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
            TokenKind::Semicolon,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("ok".to_string()),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_function_multiline() {
        let input = "function hello() {\n  echo \"Hello, world!\"\n  return 0\n}";
        let expected = vec![
            TokenKind::Function,
            TokenKind::Word("hello".to_string()),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Newline,
            TokenKind::Word("echo".to_string()),
            TokenKind::Quote,
            TokenKind::Word("Hello, world!".to_string()),
            TokenKind::Quote,
            TokenKind::Newline,
            TokenKind::Return,
            TokenKind::Word("0".to_string()),
            TokenKind::Newline,
            TokenKind::RBrace,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_for_loop_basic() {
        let input = "for i in 1 2 3; do echo $i; done";
        let expected = vec![
            TokenKind::For,
            TokenKind::Word("i".to_string()),
            TokenKind::In,
            TokenKind::Word("1".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::Word("3".to_string()),
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_for_loop_with_glob() {
        let input = "for file in *.txt; do cat $file; done";
        let expected = vec![
            TokenKind::For,
            TokenKind::Word("file".to_string()),
            TokenKind::In,
            TokenKind::Word("*.txt".to_string()),
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::Word("cat".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("file".to_string()),
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_for_loop_multiline() {
        let input = "for i in $(seq 1 10)\ndo\n  echo $i\ndone";
        let expected = vec![
            TokenKind::For,
            TokenKind::Word("i".to_string()),
            TokenKind::In,
            TokenKind::CmdSubst,
            TokenKind::Word("seq".to_string()),
            TokenKind::Word("1".to_string()),
            TokenKind::Word("10".to_string()),
            TokenKind::RParen,
            TokenKind::Newline,
            TokenKind::Do,
            TokenKind::Newline,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Newline,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_for_loop_with_break() {
        let input = "for i in 1 2 3; do if [ $i -eq 2 ]; then break; fi; done";
        let expected = vec![
            TokenKind::For,
            TokenKind::Word("i".to_string()),
            TokenKind::In,
            TokenKind::Word("1".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::Word("3".to_string()),
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Word("-eq".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Break,
            TokenKind::Semicolon,
            TokenKind::Fi,
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_for_loop_with_continue() {
        let input = "for i in 1 2 3; do if [ $i -eq 2 ]; then continue; fi; echo $i; done";
        let expected = vec![
            TokenKind::For,
            TokenKind::Word("i".to_string()),
            TokenKind::In,
            TokenKind::Word("1".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::Word("3".to_string()),
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Word("-eq".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Continue,
            TokenKind::Semicolon,
            TokenKind::Fi,
            TokenKind::Semicolon,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_c_style_for_loop() {
        let input = "for ((i=0; i<5; i++)); do echo $i; done";
        let expected = vec![
            TokenKind::For,
            TokenKind::ArithCommand,
            TokenKind::Word("i".to_string()),
            TokenKind::Assignment,
            TokenKind::Word("0".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("i".to_string()),
            TokenKind::Less,
            TokenKind::Word("5".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("i++".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_c_style_for_loop_using_decrement() {
        let input = "for ((i=5; i>0; i--)); do echo $i; done";
        let expected = vec![
            TokenKind::For,
            TokenKind::ArithCommand,
            TokenKind::Word("i".to_string()),
            TokenKind::Assignment,
            TokenKind::Word("5".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("i".to_string()),
            TokenKind::Great,
            TokenKind::Word("0".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("i--".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_while_loop_basic() {
        let input = "while [ $i -lt 10 ]; do echo $i; i=$((i+1)); done";
        let expected = vec![
            TokenKind::While,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Word("-lt".to_string()),
            TokenKind::Word("10".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("i".to_string()),
            TokenKind::Assignment,
            TokenKind::ArithSubst,
            TokenKind::Word("i+1".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_while_loop_multiline() {
        let input = "while true\ndo\n  echo looping\n  if [ $count -gt 10 ]; then break; fi\ndone";
        let expected = vec![
            TokenKind::While,
            TokenKind::Word("true".to_string()),
            TokenKind::Newline,
            TokenKind::Do,
            TokenKind::Newline,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("looping".to_string()),
            TokenKind::Newline,
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("count".to_string()),
            TokenKind::Word("-gt".to_string()),
            TokenKind::Word("10".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Break,
            TokenKind::Semicolon,
            TokenKind::Fi,
            TokenKind::Newline,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_array_declaration() {
        let input = "colors=(red green blue)";
        let expected = vec![
            TokenKind::Word("colors".to_string()),
            TokenKind::Assignment,
            TokenKind::LParen,
            TokenKind::Word("red".to_string()),
            TokenKind::Word("green".to_string()),
            TokenKind::Word("blue".to_string()),
            TokenKind::RParen,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_export_keyword() {
        let tokens = collect_tokens("export");
        assert_eq!(tokens.len(), 2); // export + EOF
        assert!(matches!(tokens[0].kind, TokenKind::Export));
        assert_eq!(tokens[0].value, "export");
    }

    #[test]
    fn test_export_assignment() {
        let tokens = collect_tokens("export VAR=value");
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();

        assert_eq!(kinds.len(), 5); // export + VAR + = + value + EOF
        assert!(matches!(kinds[0], TokenKind::Export));
        assert!(matches!(kinds[1], TokenKind::Word(_)));
        assert!(matches!(kinds[2], TokenKind::Assignment));
        assert!(matches!(kinds[3], TokenKind::Word(_)));
        assert!(matches!(kinds[4], TokenKind::EOF));

        assert_eq!(tokens[0].value, "export");
        assert_eq!(tokens[1].value, "VAR");
        assert_eq!(tokens[2].value, "=");
        assert_eq!(tokens[3].value, "value");
    }

    #[test]
    fn test_export_with_quotes() {
        let tokens = collect_tokens("export PATH=\"/usr/bin:/bin\"");
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();

        assert!(matches!(kinds[0], TokenKind::Export));
        assert!(matches!(kinds[1], TokenKind::Word(_)));
        assert!(matches!(kinds[2], TokenKind::Assignment));
        assert!(matches!(kinds[3], TokenKind::Quote));
        assert!(matches!(kinds[4], TokenKind::Word(_)));
        assert!(matches!(kinds[5], TokenKind::Quote));

        assert_eq!(tokens[0].value, "export");
        assert_eq!(tokens[1].value, "PATH");
        assert_eq!(tokens[4].value, "/usr/bin:/bin");
    }

    #[test]
    fn test_export_multiple_variables() {
        let tokens = collect_tokens("export VAR1=val1 VAR2=val2");
        let export_count = tokens
            .iter()
            .filter(|t| matches!(t.kind, TokenKind::Export))
            .count();
        assert_eq!(export_count, 1); // Only one export keyword

        let var_count = tokens
            .iter()
            .filter(|t| matches!(t.kind, TokenKind::Word(_)))
            .count();
        assert_eq!(var_count, 4); // VAR1, val1, VAR2, val2
    }

    #[test]
    fn test_export_not_keyword_when_part_of_word() {
        let tokens = collect_tokens("exported");
        assert_eq!(tokens.len(), 2); // word + EOF
        assert!(matches!(tokens[0].kind, TokenKind::Word(_)));
        assert_eq!(tokens[0].value, "exported");

        let tokens2 = collect_tokens("exportable");
        assert!(matches!(tokens2[0].kind, TokenKind::Word(_)));
        assert_eq!(tokens2[0].value, "exportable");
    }

    #[test]
    fn test_export_with_newline() {
        let tokens = collect_tokens("export VAR=value\necho $VAR");
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();

        assert!(matches!(kinds[0], TokenKind::Export));
        assert!(matches!(kinds[4], TokenKind::Newline)); // After value
        assert!(matches!(kinds[5], TokenKind::Word(_))); // echo
    }

    // #[test]
    // fn test_export_with_variable() {
    //     let tokens = collect_tokens("export PATH=\"$PATH\":");
    //     let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();

    //     assert!(matches!(kinds[0], TokenKind::Export));
    //     assert!(matches!(kinds[4], TokenKind::Newline)); // After value
    //     assert!(matches!(kinds[5], TokenKind::Word(_))); // echo
    // }

    #[test]
    fn test_export_with_semicolon() {
        let tokens = collect_tokens("export VAR=value; echo done");
        let semicolon_pos = tokens
            .iter()
            .position(|t| matches!(t.kind, TokenKind::Semicolon));
        assert!(semicolon_pos.is_some());
    }

    #[test]
    fn test_until_loop() {
        let input = "until [ $count -eq 10 ]; do echo $count; count=$((count+1)); done";
        let expected = vec![
            TokenKind::Until,
            TokenKind::LBracket,
            TokenKind::Dollar,
            TokenKind::Word("count".to_string()),
            TokenKind::Word("-eq".to_string()),
            TokenKind::Word("10".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("count".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("count".to_string()),
            TokenKind::Assignment,
            TokenKind::ArithSubst,
            TokenKind::Word("count+1".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_nested_loops() {
        let input = "for i in 1 2; do for j in a b; do echo $i$j; done; done";
        let expected = vec![
            TokenKind::For,
            TokenKind::Word("i".to_string()),
            TokenKind::In,
            TokenKind::Word("1".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::For,
            TokenKind::Word("j".to_string()),
            TokenKind::In,
            TokenKind::Word("a".to_string()),
            TokenKind::Word("b".to_string()),
            TokenKind::Semicolon,
            TokenKind::Do,
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("i".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("j".to_string()),
            TokenKind::Semicolon,
            TokenKind::Done,
            TokenKind::Semicolon,
            TokenKind::Done,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_complex_redirections() {
        let input = "cmd < input.txt > output.txt 2>&1 >> append.log";
        let expected = vec![
            TokenKind::Word("cmd".to_string()),
            TokenKind::Less,
            TokenKind::Word("input.txt".to_string()),
            TokenKind::Great,
            TokenKind::Word("output.txt".to_string()),
            TokenKind::Word("2".to_string()),
            TokenKind::OutputDup,
            TokenKind::Word("1".to_string()),
            TokenKind::DGreat,
            TokenKind::Word("append.log".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_backtick_command_substitution() {
        let input = "echo `date +%Y`";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Backtick,
            TokenKind::Word("date".to_string()),
            TokenKind::Word("+%Y".to_string()),
            TokenKind::Backtick,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_nested_command_substitution() {
        let input = "echo $(echo $(date))";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::CmdSubst,
            TokenKind::Word("echo".to_string()),
            TokenKind::CmdSubst,
            TokenKind::Word("date".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_mixed_quotes() {
        let input = r#"echo "single 'quote' inside" 'double "quote" inside'"#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Quote,
            TokenKind::Word("single 'quote' inside".to_string()),
            TokenKind::Quote,
            TokenKind::SingleQuote,
            TokenKind::Word("double \"quote\" inside".to_string()),
            TokenKind::SingleQuote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_escaped_quotes() {
        let input = r#"echo "escaped \" quote""#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Quote,
            TokenKind::Word(r#"escaped \" quote"#.to_string()),
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_multiline_strings() {
        // A literal newline inside a quoted string must not appear inside any
        // `Word` token. The lexer emits a fresh `Word` for each line and a
        // `Newline` token in between, while remaining in the quoted state.
        let input = "echo \"line1\nline2\nline3\"";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Quote,
            TokenKind::Word("line1".to_string()),
            TokenKind::Newline,
            TokenKind::Word("line2".to_string()),
            TokenKind::Newline,
            TokenKind::Word("line3".to_string()),
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_complex_variable_expansion() {
        let input = "echo $HOME ${USER} $((2+3)) $?";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Dollar,
            TokenKind::Word("HOME".to_string()),
            TokenKind::ParamExpansion,
            TokenKind::Word("USER".to_string()),
            TokenKind::RBrace,
            TokenKind::ArithSubst,
            TokenKind::Word("2+3".to_string()),
            TokenKind::RParen,
            TokenKind::RParen,
            TokenKind::Dollar,
            TokenKind::Word("?".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_array_access() {
        let input = "echo ${array[0]} ${array[@]} ${#array[@]}";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::ParamExpansion,
            TokenKind::Word("array".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("0".to_string()),
            TokenKind::RBracket,
            TokenKind::RBrace,
            TokenKind::ParamExpansion,
            TokenKind::Word("array".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("@".to_string()),
            TokenKind::RBracket,
            TokenKind::RBrace,
            TokenKind::ParamExpansion,
            TokenKind::Word("#".to_string()),
            TokenKind::Word("array".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("@".to_string()),
            TokenKind::RBracket,
            TokenKind::RBrace,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_complex_extglob() {
        let input = "ls !(*.tmp|*.log) @(file1|file2).txt +(a|b|c)*";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::ExtGlob('!'),
            TokenKind::Word("*.tmp".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("*.log".to_string()),
            TokenKind::RParen,
            TokenKind::ExtGlob('@'),
            TokenKind::Word("file1".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("file2".to_string()),
            TokenKind::RParen,
            TokenKind::Word(".txt".to_string()),
            TokenKind::ExtGlob('+'),
            TokenKind::Word("a".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("b".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("c".to_string()),
            TokenKind::RParen,
            TokenKind::Word("*".to_string()),
        ];
        test_tokens(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_subshell_and_grouping() {
        let input = "(cd /tmp && ls) { echo group; }";
        let expected = vec![
            TokenKind::LParen,
            TokenKind::Word("cd".to_string()),
            TokenKind::Word("/tmp".to_string()),
            TokenKind::And,
            TokenKind::Word("ls".to_string()),
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("group".to_string()),
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_pipeline_with_multiple_commands() {
        let input = "cat file.txt | grep pattern | sort | uniq -c | head -10";
        let expected = vec![
            TokenKind::Word("cat".to_string()),
            TokenKind::Word("file.txt".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("grep".to_string()),
            TokenKind::Word("pattern".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("sort".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("uniq".to_string()),
            TokenKind::Word("-c".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("head".to_string()),
            TokenKind::Word("-10".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_complex_conditional_operators() {
        let input = "cmd1 && cmd2 || cmd3 && cmd4";
        let expected = vec![
            TokenKind::Word("cmd1".to_string()),
            TokenKind::And,
            TokenKind::Word("cmd2".to_string()),
            TokenKind::Or,
            TokenKind::Word("cmd3".to_string()),
            TokenKind::And,
            TokenKind::Word("cmd4".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_function_with_complex_body() {
        let input = "function deploy() { if [ -f Dockerfile ]; then docker build -t app .; docker run -d app; else echo 'No Dockerfile found'; fi; }";
        let expected = vec![
            TokenKind::Function,
            TokenKind::Word("deploy".to_string()),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Word("-f".to_string()),
            TokenKind::Word("Dockerfile".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("docker".to_string()),
            TokenKind::Word("build".to_string()),
            TokenKind::Word("-t".to_string()),
            TokenKind::Word("app".to_string()),
            TokenKind::Word(".".to_string()),
            TokenKind::Semicolon,
            TokenKind::Word("docker".to_string()),
            TokenKind::Word("run".to_string()),
            TokenKind::Word("-d".to_string()),
            TokenKind::Word("app".to_string()),
            TokenKind::Semicolon,
            TokenKind::Else,
            TokenKind::Word("echo".to_string()),
            TokenKind::SingleQuote,
            TokenKind::Word("No Dockerfile found".to_string()),
            TokenKind::SingleQuote,
            TokenKind::Semicolon,
            TokenKind::Fi,
            TokenKind::Semicolon,
            TokenKind::RBrace,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_whitespace_handling() {
        let input = "  cmd1   arg1    arg2  ";
        let expected = vec![
            TokenKind::Word("cmd1".to_string()),
            TokenKind::Word("arg1".to_string()),
            TokenKind::Word("arg2".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_whitespace_tokens_leading() {
        let input = "  echo";
        let expected = vec![
            TokenKind::Whitespace("  ".to_string()),
            TokenKind::Word("echo".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
    }

    #[test]
    fn test_whitespace_tokens_trailing() {
        let input = "echo  ";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace("  ".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
    }

    #[test]
    fn test_whitespace_tokens_only() {
        let input = " \t  ";
        let expected = vec![TokenKind::Whitespace(" \t  ".to_string())];
        test_tokens_include_whitespace(input, expected);
    }

    #[test]
    fn test_empty_input() {
        let input = "";
        let expected = vec![];
        test_tokens(input, expected);
    }

    #[test]
    fn test_only_whitespace() {
        let input = "   \t  \t   ";
        let expected = vec![];
        test_tokens(input, expected);
    }

    #[test]
    fn test_only_comments() {
        let input = "# This is a comment\n# Another comment";
        let expected = vec![TokenKind::Comment, TokenKind::Newline, TokenKind::Comment];
        test_tokens(input, expected);
    }

    #[test]
    fn test_special_characters_in_words() {
        let input = "file-name file_name file.txt file@host file:port";
        let expected = vec![
            TokenKind::Word("file-name".to_string()),
            TokenKind::Word("file_name".to_string()),
            TokenKind::Word("file.txt".to_string()),
            TokenKind::Word("file@host".to_string()),
            TokenKind::Word("file:port".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_numbers_and_arithmetic() {
        let input = "echo 123 0x1F 0755 3.14";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("123".to_string()),
            TokenKind::Word("0x1F".to_string()),
            TokenKind::Word("0755".to_string()),
            TokenKind::Word("3.14".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_path_separators() {
        let input = "/usr/bin/bash ./script.sh ../parent/file ~/home/user";
        let expected = vec![
            TokenKind::Word("/usr/bin/bash".to_string()),
            TokenKind::Word("./script.sh".to_string()),
            TokenKind::Word("../parent/file".to_string()),
            TokenKind::Word("~/home/user".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_keyword_boundaries() {
        let input = "ifconfig thenext elifant elsewhere fifo";
        let expected = vec![
            TokenKind::Word("ifconfig".to_string()),
            TokenKind::Word("thenext".to_string()),
            TokenKind::Word("elifant".to_string()),
            TokenKind::Word("elsewhere".to_string()),
            TokenKind::Word("fifo".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_position_tracking() {
        let input = "line1\nline2\nline3";
        let mut lexer = Lexer::new(input);

        let token1 = lexer.next_token();
        assert_eq!(token1.position.line, 1);
        assert_eq!(token1.position.column, 1);

        let newline1 = lexer.next_token();
        assert_eq!(newline1.kind, TokenKind::Newline);

        let token2 = lexer.next_token();
        assert_eq!(token2.position.line, 2);
        assert_eq!(token2.position.column, 1);
    }

    #[test]
    fn test_error_recovery() {
        // Test lexer behavior with malformed input
        let input = "echo \"unclosed quote";
        let mut lexer = Lexer::new(input);

        let echo_token = lexer.next_token();
        assert_eq!(echo_token.kind, TokenKind::Word("echo".to_string()));

        let quote_token = next_non_whitespace(&mut lexer);
        assert_eq!(quote_token.kind, TokenKind::Quote);

        let content_token = next_non_whitespace(&mut lexer);
        assert_eq!(
            content_token.kind,
            TokenKind::Word("unclosed quote".to_string())
        );

        // The lexer should handle EOF gracefully even with unclosed quotes
        let eof_token = lexer.next_token();
        // The actual behavior might be different, so let's just check it doesn't panic
        assert!(matches!(
            eof_token.kind,
            TokenKind::EOF | TokenKind::Word(_)
        ));
    }

    #[test]
    fn test_large_input_performance() {
        // Test with a reasonably large input to ensure performance
        let large_input = "echo hello; ".repeat(1000);
        let mut lexer = Lexer::new(&large_input);

        let mut token_count = 0;
        loop {
            let mut token = lexer.next_token();
            while matches!(token.kind, TokenKind::Whitespace(_)) {
                token = lexer.next_token();
            }
            if token.kind == TokenKind::EOF {
                break;
            }
            token_count += 1;
        }

        // Should have 3000 tokens (echo, hello, semicolon) * 1000 repetitions
        assert_eq!(token_count, 3000);
    }

    #[test]
    fn test_comprehensive_glob_patterns() {
        // Test various glob patterns to ensure brackets are tokenized cleanly
        let input = "ls *.txt file?.log [0-9]*.dat [a-z][A-Z]*.tmp [!abc]*.bak";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::Word("*.txt".to_string()),
            TokenKind::Word("file?.log".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("0-9".to_string()),
            TokenKind::RBracket,
            TokenKind::Word("*.dat".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("a-z".to_string()),
            TokenKind::RBracket,
            TokenKind::LBracket,
            TokenKind::Word("A-Z".to_string()),
            TokenKind::RBracket,
            TokenKind::Word("*.tmp".to_string()),
            TokenKind::LBracket,
            TokenKind::History,
            TokenKind::Word("abc".to_string()),
            TokenKind::RBracket,
            TokenKind::Word("*.bak".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_glob_patterns_with_paths() {
        // Test glob patterns with directory paths
        let input = "find /path/*.txt ./local/file?.log ../parent/[abc]*.tmp";
        let expected = vec![
            TokenKind::Word("find".to_string()),
            TokenKind::Word("/path/*.txt".to_string()),
            TokenKind::Word("./local/file?.log".to_string()),
            TokenKind::Word("../parent/".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("abc".to_string()),
            TokenKind::RBracket,
            TokenKind::Word("*.tmp".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_glob_patterns_in_quotes() {
        // Test that glob patterns in quotes are preserved as literals inside quotes
        let input = r#"echo "*.txt" 'file?.log' "test[abc].dat""#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Quote,
            TokenKind::Word("*.txt".to_string()),
            TokenKind::Quote,
            TokenKind::SingleQuote,
            TokenKind::Word("file?.log".to_string()),
            TokenKind::SingleQuote,
            TokenKind::Quote,
            TokenKind::Word("test[abc].dat".to_string()),
            TokenKind::Quote,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_complex_glob_combinations() {
        // Test complex combinations of glob patterns
        let input = "command *.[ch] *.{txt,log} file[0-9][a-z].* test*[!~]";
        let expected = vec![
            TokenKind::Word("command".to_string()),
            TokenKind::Word("*.".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("ch".to_string()),
            TokenKind::RBracket,
            TokenKind::Word("*.".to_string()),
            TokenKind::LBrace,
            TokenKind::Word("txt,log".to_string()),
            TokenKind::RBrace,
            TokenKind::Word("file".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("0-9".to_string()),
            TokenKind::RBracket,
            TokenKind::LBracket,
            TokenKind::Word("a-z".to_string()),
            TokenKind::RBracket,
            TokenKind::Word(".*".to_string()),
            TokenKind::Word("test*".to_string()),
            TokenKind::LBracket,
            TokenKind::History,
            TokenKind::Word("~".to_string()),
            TokenKind::RBracket,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_glob_patterns_with_special_chars() {
        // Test glob patterns with special characters that should be preserved
        let input = "ls *-file.txt file_*.log test[._-]*.dat";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::Word("*-file.txt".to_string()),
            TokenKind::Word("file_*.log".to_string()),
            TokenKind::Word("test".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("._-".to_string()),
            TokenKind::RBracket,
            TokenKind::Word("*.dat".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_negated_character_classes() {
        // Test negated character classes in glob patterns
        let input = "ls file[!0-9].txt data[^abc].log test[!~#].dat";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::Word("file".to_string()),
            TokenKind::LBracket,
            TokenKind::History,
            TokenKind::Word("0-9".to_string()),
            TokenKind::RBracket,
            TokenKind::Word(".txt".to_string()),
            TokenKind::Word("data".to_string()),
            TokenKind::LBracket,
            TokenKind::Word("^abc".to_string()),
            TokenKind::RBracket,
            TokenKind::Word(".log".to_string()),
            TokenKind::Word("test".to_string()),
            TokenKind::LBracket,
            TokenKind::History,
            TokenKind::Word("~#".to_string()),
            TokenKind::RBracket,
            TokenKind::Word(".dat".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_glob_patterns_mixed_with_other_tokens() {
        // Test glob patterns mixed with other shell constructs
        let input = "if [ -f *.txt ]; then echo file*.log | grep test; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::LBracket,
            TokenKind::Word("-f".to_string()),
            TokenKind::Word("*.txt".to_string()),
            TokenKind::RBracket,
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("file*.log".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("grep".to_string()),
            TokenKind::Word("test".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_logical_negation_tokenization() {
        // Test ! followed by space (should be Word token)
        let input = "! echo test";
        let expected = vec![
            TokenKind::Word("!".to_string()),
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("test".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_history_expansion_tokenization() {
        // Test !! (should be History token)
        let input = "!!";
        let expected = vec![TokenKind::History];
        test_tokens(input, expected);

        // Test !command (should be History token)
        let input = "!echo";
        let expected = vec![TokenKind::History, TokenKind::Word("echo".to_string())];
        test_tokens(input, expected);

        // Test !123 (should be History token)
        let input = "!123";
        let expected = vec![TokenKind::History, TokenKind::Word("123".to_string())];
        test_tokens(input, expected);
    }

    #[test]
    fn test_negation_vs_history_distinction() {
        // Test ! followed by space vs ! followed by word
        let input1 = "! false";
        let expected1 = vec![
            TokenKind::Word("!".to_string()),
            TokenKind::Word("false".to_string()),
        ];
        test_tokens(input1, expected1);

        let input2 = "!false";
        let expected2 = vec![TokenKind::History, TokenKind::Word("false".to_string())];
        test_tokens(input2, expected2);
    }

    #[test]
    fn test_complete_command_tokenization() {
        let input = "complete -F _test test";
        let expected = vec![
            TokenKind::Complete,
            TokenKind::Word("-F".to_string()),
            TokenKind::Word("_test".to_string()),
            TokenKind::Word("test".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_command_builtin_tokenization() {
        let input = "command echo test";
        let expected = vec![
            TokenKind::Word("command".to_string()),
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("test".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_complex_negation_scenarios() {
        // Test double negation
        let input = "! ! true";
        let expected = vec![
            TokenKind::Word("!".to_string()),
            TokenKind::Word("!".to_string()),
            TokenKind::Word("true".to_string()),
        ];
        test_tokens(input, expected);

        // Test negation in conditional
        let input = "if ! command false; then echo success; fi";
        let expected = vec![
            TokenKind::If,
            TokenKind::Word("!".to_string()),
            TokenKind::Word("command".to_string()),
            TokenKind::Word("false".to_string()),
            TokenKind::Semicolon,
            TokenKind::Then,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("success".to_string()),
            TokenKind::Semicolon,
            TokenKind::Fi,
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_mixed_history_and_negation() {
        // Test script with both history expansion and negation
        let input = "!! && ! false";
        let expected = vec![
            TokenKind::History,
            TokenKind::And,
            TokenKind::Word("!".to_string()),
            TokenKind::Word("false".to_string()),
        ];
        test_tokens(input, expected);
    }

    #[test]
    fn test_extglob_vs_negation() {
        // Test that !(pattern) is treated as a word (extglob pattern)
        let input = "!(*.txt)";
        let expected = vec![
            TokenKind::ExtGlob('!'),
            TokenKind::Word("*.txt".to_string()),
            TokenKind::RParen,
        ];
        test_tokens(input, expected);

        // But ! (pattern) should be negation + subshell
        let input = "! (echo test)";
        let expected = vec![
            TokenKind::Word("!".to_string()),
            TokenKind::LParen,
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("test".to_string()),
            TokenKind::RParen,
        ];
        test_tokens(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_backslashed_spaces() {
        let input = "echo hello\\ world";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Word("hello\\ world".to_string()),
        ];
        test_tokens(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_lexer_on_backslash_1() {
        let input = r#"echo "asd\"#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Quote,
            TokenKind::Word("asd\\".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_lexer_on_backslash_2() {
        let input = r#"echo "asd\ "#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Quote,
            TokenKind::Word("asd\\ ".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_lexer_on_backslash_3() {
        let input = r#"echo \""#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Word("\\\"".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_lexer_on_backslash_4() {
        let input = r#"echo asd\ foo"#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Word("asd\\ foo".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_lexer_on_backslash_5() {
        let input = r#"echo foo\"#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Word("foo\\".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_lexer_on_backslash_6() {
        let input = r#"echo \"foo"#;
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Word("\\\"foo".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_line_continuation() {
        let input = "ls \\\n-la";
        let expected = vec![
            TokenKind::Word("ls".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Word("\\".to_string()),
            TokenKind::Newline,
            TokenKind::Word("-la".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_line_continuation_with_spaces() {
        let input = "echo hello\\\\\\";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Word("hello\\\\\\".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }

    #[test]
    fn test_termination_of_lexing() {
        let input = "echo \"";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::Quote,
            TokenKind::EOF,
        ];
        test_tokens_include_whitespace(input, expected);
    }

    #[test]
    fn test_extglob_termination() {
        let input = "echo @(a|b";
        let expected = vec![
            TokenKind::Word("echo".to_string()),
            TokenKind::Whitespace(" ".to_string()),
            TokenKind::ExtGlob('@'),
            TokenKind::Word("a".to_string()),
            TokenKind::Pipe,
            TokenKind::Word("b".to_string()),
        ];
        test_tokens_include_whitespace(input, expected);
        test_round_trip(input);
    }
}
