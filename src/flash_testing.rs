use flash::lexer::{Lexer, Position, Token, TokenKind};
use flash::parser::{Node, Parser};

pub fn tokenize(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();

    loop {
        let token = lexer.next_token();
        if token.kind == TokenKind::EOF {
            break;
        }
        tokens.push(token);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer() {
        let input = "TEST=1 grep 'patte\"sdf\"rn' file.txt > out.txt ; if asdfd; then echo hi; fi";
        let tokens = tokenize(input);
        dbg!("Tokens: {:?}", tokens);
        assert!(false);
    }

    #[test]
    fn test_parser() {
        let input = "TEST=1 grep 'patte\"sdf\"rn' file.txt > out.txt";
        let lexer = Lexer::new(input);
        let mut parser = Parser::new(lexer);
        let ast = parser.parse_script();
        dbg!("AST: {:?}", ast);
        assert!(false);
    }

    #[test]
    fn test_lexer2() {
        let input = "echo $(VAR(_sdf qwe ";
        let tokens = tokenize(input);
        dbg!("Tokens: {:?}", tokens);
        assert!(false);
    }
}
