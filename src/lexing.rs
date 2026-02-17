use flash::lexer::{Lexer, Token, TokenKind};

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
