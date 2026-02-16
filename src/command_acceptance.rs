use flash::lexer::{Lexer, Token, TokenKind};
use std::collections::VecDeque;

fn collect_tokens_include_whitespace(input: &str) -> Vec<Token> {
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

pub fn will_bash_accept_buffer(buffer: &str) -> bool {
    // returns true iff bash won't try to get more input to complete the command
    // e.g. unclosed quotes, unclosed parens/braces/brackets, etc.
    // its ok if there are syntax errors, as long as the command is "complete"

    let tokens: Vec<Token> = collect_tokens_include_whitespace(buffer);

    let mut nestings: Vec<TokenKind> = Vec::new();
    let mut heredocs: VecDeque<String> = VecDeque::new();

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
        };

    if let Some(last_token) = tokens
        .iter()
        .rev()
        .skip_while(|t| matches!(t.kind, TokenKind::Whitespace(_) | TokenKind::Comment))
        .next()
    {
        match &last_token.kind {
            TokenKind::Pipe | TokenKind::And | TokenKind::Or => {
                return false;
            }
            TokenKind::Word(s)
                if s.trim().chars().rev().take_while(|c| *c == '\\').count() % 2 == 1 =>
            {
                return false;
            }
            _ => {}
        }
    }

    let mut toks = tokens.iter().peekable();
    loop {
        let token = match toks.next() {
            Some(t) => t,
            None => break,
        };

        if cfg!(test) {
            dbg!("Token: {:?}", token);
        }

        match &token.kind {
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
                if nested_opening_satisfied(&token, nestings.last()) =>
            {
                // dbg!("Pushing nesting:");
                // dbg!(&token.kind);
                // dbg!(&nestings);
                nestings.push(token.kind.clone());
            }
            TokenKind::HereDoc(delim) | TokenKind::HereDocDash(delim) => {
                heredocs.push_back(delim.to_string());
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

        if let TokenKind::Word(word) = &token.kind {
            if heredocs.front().is_some_and(|delim| delim == word) {
                heredocs.pop_front();
            }
        }
    }

    if cfg!(test) {
        dbg!("Final nestings:");
        dbg!(&nestings);
    }

    nestings.is_empty() && heredocs.is_empty()
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
        // assert_eq!(
        //     will_bash_accept_buffer("cat <<EOF1  <<EOF2\nhello\nEOF1\nworld\nEOF2"),
        //     true
        // );
    }

    #[test]
    fn test_line_continuation_basic() {
        // Basic line continuation at end of line
        assert_eq!(will_bash_accept_buffer("echo hello \\"), false);
        assert_eq!(will_bash_accept_buffer("echo hello \\\nworld"), true);

        // Line continuation with trailing whitespace (tricky!)
        assert_eq!(will_bash_accept_buffer("echo hello \\  "), false);
        assert_eq!(will_bash_accept_buffer("echo hello \\\t"), false);
    }

    #[test]
    fn test_line_continuation_in_strings() {
        // Line continuation inside double quotes - bash still expects more input
        assert_eq!(will_bash_accept_buffer("echo \"hello \\"), false);
        assert_eq!(will_bash_accept_buffer("echo \"hello \\\nworld\""), true);

        // Multiple line continuations in a complex command
        assert_eq!(
            will_bash_accept_buffer("if [ \"$var\" = \"value\" ] && \\"),
            false
        );
        assert_eq!(
            will_bash_accept_buffer(
                "if [ \"$var\" = \"value\" ] && \\\n   [ \"$other\" = \"test\" ]; then echo ok; fi"
            ),
            true
        );

        // Line continuation before pipe (very tricky edge case)
        assert_eq!(will_bash_accept_buffer("echo hello \\\n|"), false);
        assert_eq!(will_bash_accept_buffer("echo hello \\\n| grep l"), true);
    }

    #[test]
    fn test_line_continuation_edge_cases() {
        // Line continuation in command substitution
        assert_eq!(will_bash_accept_buffer("echo $(ls \\"), false);
        assert_eq!(will_bash_accept_buffer("echo $(ls \\\n-la)"), true);

        // Line continuation with heredoc (super tricky!)
        assert_eq!(will_bash_accept_buffer("cat <<EOF \\"), false);
        assert_eq!(will_bash_accept_buffer("cat <<EOF \\\nhello\nEOF"), true);

        // Multiple backslashes - only the last one matters for continuation
        assert_eq!(will_bash_accept_buffer("echo hello\\\\\\"), false);
        assert_eq!(will_bash_accept_buffer("echo hello\\\\"), true); // Even number of backslashes = no continuation

        // Line continuation in function definition
        assert_eq!(will_bash_accept_buffer("function test() { \\"), false);
        assert_eq!(
            will_bash_accept_buffer("function test() { \\\necho hi; }"),
            true
        );
    }

    #[test]
    fn test_asdf() {
        assert_eq!(
            will_bash_accept_buffer("gcm \"no history suggestion if empty\""),
            true
        );
    }

    // TODO test ones that will be syntax errors but complete commands
}
