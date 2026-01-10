use flash::lexer::{Lexer, Token, TokenKind};
use flash::parser::{Node, Parser};

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn parse(input: &str) -> Node {
    let lexer = Lexer::new(input);
    let mut parser = Parser::new(lexer);
    parser.parse_script()
}

#[cfg(test)]
mod tests {
    // This is to understand how the lexer and parser work

    use super::*;

    // #[test]
    // fn test_lexer() {
    //     let tokens = tokenize(
    //         "TEST=1 grep 'patte\"sdf\"rn' file.txt > out.txt ; if asdfd; then echo hi; fi",
    //     );
    //     dbg!("Tokens: {:?}", tokens);
    //     assert!(false);
    // }

    // #[test]
    // fn test_parser() {
    //     let ast = parse("TEST=1 grep 'patte\"sdf\"rn' file.txt > out.txt");
    //     dbg!("AST: {:?}", ast);
    //     assert!(false);
    // }

    // #[test]
    // fn test_lexer2() {
    //     let tokens = tokenize("echo $(VAR(_sdf qwe ");
    //     dbg!("Tokens: {:?}", tokens);
    //     assert!(false);
    // }
}
