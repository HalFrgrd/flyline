use crate::lexer;
use flash::lexer::{Token, TokenKind};

pub fn will_bash_accept_buffer(buffer: &str) -> bool {
    // returns true iff bash won't try to get more input to complete the command
    // e.g. unclosed quotes, unclosed parens/braces/brackets, etc.
    // its ok if there are syntax errors, as long as the command is "complete"

    let tokens: Vec<Token> = lexer::safe_into_tokens(buffer);

    let mut nestings: Vec<TokenKind> = Vec::new();

    let nested_opening_satisfied = |token: &Token, current_nesting: Option<&TokenKind>| -> bool {
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
    };

    let nested_closing_satisfied =
        |token: &Token, current_nesting: Option<&TokenKind>, next_token: Option<&&Token>| {
            let current_nesting = match current_nesting {
                Some(v) => v,
                None => return false,
            };
            match (&token.kind, current_nesting) {
            (TokenKind::RParen, TokenKind::LParen) => true,
            (TokenKind::RParen, TokenKind::CmdSubst) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstIn) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstOut) => true,
            (TokenKind::RBrace, TokenKind::ParamExpansion) => true,
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
        };

    if tokens
        .iter()
        .rev()
        .skip_while(|t| t.kind == TokenKind::EOF)
        .next()
        .map_or(false, |token| {
            matches!(token.kind, TokenKind::Pipe | TokenKind::And | TokenKind::Or)
        })
    {
        // last_token_needs_more_input
        log::debug!("Last token needs more input");
        return false;
    }

    let mut toks = tokens.iter().peekable();
    loop {
        let token = match toks.next() {
            Some(t) => t,
            None => break,
        };
        // log::debug!("Current token:");
        // log::debug!("{:?}", &token.kind);

        match token.kind {
            TokenKind::LParen
            | TokenKind::LBrace
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
            | TokenKind::HereDoc
                if nested_opening_satisfied(&token, nestings.last()) =>
            {
                // dbg!("Pushing nesting:");
                // dbg!(&token.kind);
                // dbg!(&nestings);
                nestings.push(token.kind.clone());
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
                if nested_closing_satisfied(&token, nestings.last(), toks.peek()) =>
            {
                // dbg!("Popping nesting:");
                // dbg!(&token.kind);
                // dbg!(&nestings);
                let kind = nestings.pop().unwrap();
                if kind == TokenKind::ArithSubst {
                    assert!(
                        toks.peek().unwrap().kind == TokenKind::RParen,
                        "expected two RParen tokens"
                    );
                    toks.next(); // consume the extra RParen
                }
            }
            _ => {}
        }
    }

    // dbg!("Final nestings:");
    // dbg!(&nestings);

    nestings.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unclosed_quotes() {
        assert_eq!(will_bash_accept_buffer("echo 'hello"), false);
        assert_eq!(will_bash_accept_buffer("echo \"hello"), false);
        assert_eq!(will_bash_accept_buffer("echo '\nhello'"), true);
        assert_eq!(will_bash_accept_buffer("echo \"\nhello\""), true);
    }

    #[test]
    fn test_command_substitutions() {
        assert_eq!(will_bash_accept_buffer("echo $(ls"), false);
        assert_eq!(will_bash_accept_buffer("echo $(ls)"), true);
        assert_eq!(will_bash_accept_buffer("echo $((1 + 2"), false);
        assert_eq!(will_bash_accept_buffer("echo $((1 + 2)"), false);
        assert_eq!(will_bash_accept_buffer("echo $((1 + 2))"), true);
        assert_eq!(will_bash_accept_buffer("echo ${VAR}"), true);
        assert_eq!(will_bash_accept_buffer("echo ${VAR"), false);
        // test backticks
        assert_eq!(will_bash_accept_buffer("echo `ls"), false);
        assert_eq!(will_bash_accept_buffer("echo `ls`"), true);
    }

    #[test]
    fn test_here_documents() {
        assert_eq!(will_bash_accept_buffer("cat <<EOF\nhello"), false);
        assert_eq!(will_bash_accept_buffer("cat <<EOF\nhello\nEOF"), true);
    }

    #[test]
    fn test_if_then_fi() {
        assert_eq!(will_bash_accept_buffer("if true; then echo hi"), false);
        assert_eq!(will_bash_accept_buffer("if true; then echo hi; fi"), true);

        // test if-elif-else-fi
        assert_eq!(
            will_bash_accept_buffer("if true; then echo hi; elif false; then echo bye"),
            false
        );
        assert_eq!(
            will_bash_accept_buffer(
                "if true; then echo hi; elif false; then echo bye; else echo meh; fi"
            ),
            true
        );
    }

    #[test]
    fn test_for_loops() {
        assert_eq!(will_bash_accept_buffer("for i in 1 2 3; do echo $i"), false);
        assert_eq!(
            will_bash_accept_buffer("for i in 1 2 3; do echo $i; done"),
            true
        );
    }

    #[test]
    fn test_while_loops() {
        assert_eq!(will_bash_accept_buffer("while true; do echo hi"), false);
        assert_eq!(
            will_bash_accept_buffer("while true; do echo hi; done"),
            true
        );
    }

    #[test]
    fn test_case_statements() {
        assert_eq!(
            will_bash_accept_buffer("case $var in pattern) echo hi"),
            false
        );
        assert_eq!(
            will_bash_accept_buffer("case $var in pattern) echo hi ;; esac"),
            true
        );
    }

    #[test]
    fn test_nested_structures() {
        assert_eq!(will_bash_accept_buffer("echo ( ${ )"), false);
        assert_eq!(will_bash_accept_buffer("echo ( ${ } )"), true);
    }

    #[test]
    fn test_endings() {
        assert_eq!(will_bash_accept_buffer("echo hello |"), false);
        assert_eq!(will_bash_accept_buffer("echo hello | grep h"), true);

        assert_eq!(will_bash_accept_buffer("echo hello ||"), false);
        assert_eq!(will_bash_accept_buffer("echo hello || grep h"), true);

        assert_eq!(will_bash_accept_buffer("echo hello &&"), false);
        assert_eq!(will_bash_accept_buffer("echo hello && grep h"), true);
    }

    #[test]
    fn test_comments() {
        assert_eq!(
            will_bash_accept_buffer("echo hello # ' this is a comment"),
            true
        );
        assert_eq!(
            will_bash_accept_buffer("echo hello # ' this is a comment\n"),
            true
        );
    }

    #[test]
    fn test_process_substitution() {
        assert_eq!(will_bash_accept_buffer("diff <(ls) <(pwd"), false);
        assert_eq!(will_bash_accept_buffer("diff <(ls) <(pwd)"), true);
    }

    #[test]
    fn test_ext_glob() {
        assert_eq!(
            will_bash_accept_buffer("shopt -s extglob; echo @(a|b"),
            false
        );
        assert_eq!(
            will_bash_accept_buffer("shopt -s extglob; echo @(a|b)"),
            true
        );
    }

    #[test]
    fn test_function_def() {
        assert_eq!(will_bash_accept_buffer("my_func() { echo hello"), false);
        assert_eq!(will_bash_accept_buffer("my_func() { echo hello; }"), true);
    }

    #[test]
    fn test_multiple_heredocs() {
        assert_eq!(
            will_bash_accept_buffer("cat <<EOF1  <<EOF2\nhello\nEOF1\nworld\n"),
            false
        );
        assert_eq!(
            will_bash_accept_buffer("cat <<EOF1  <<EOF2\nhello\nEOF1\nworld\nEOF2"),
            true
        );
    }

    // TODO test ones that will be syntax errors but complete commands
}
