use crate::dparser::{DParser, collect_tokens_include_whitespace};
use flash::lexer::{Token, TokenKind};

pub fn will_bash_accept_buffer(buffer: &str) -> bool {
    // returns true iff bash won't try to get more input to complete the command
    // e.g. unclosed quotes, unclosed parens/braces/brackets, etc.
    // its ok if there are syntax errors, as long as the command is "complete"

    let tokens: Vec<Token> = collect_tokens_include_whitespace(buffer);

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

    let mut parser = DParser::new(tokens);
    parser.walk_to_end();

    !parser.needs_more_input()
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
    fn test_interleaved_heredocs_fifo() {
        // Delimiters must close in the order they appear (FIFO), not nested.
        let interleaved = "cat <<A <<-B\nline1\nB\nline2\nA\n";
        assert_eq!(will_bash_accept_buffer(interleaved), false);

        let ordered = "cat <<A <<-B\nline1\nA\nline2\nB\n";
        assert_eq!(will_bash_accept_buffer(ordered), true);
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
    fn test_unrecognised_tokens() {
        assert_eq!(will_bash_accept_buffer("echo }"), true);
        assert_eq!(will_bash_accept_buffer("echo ]"), true);

        // These are accepted by bash but are harder to analyse since they might affect
        // nesting levels. e.g this wont be accepted: function abc {
        // assert_eq!(will_bash_accept_buffer("echo {"), true);
        // assert_eq!(will_bash_accept_buffer("echo ["), true);
        // assert_eq!(will_bash_accept_buffer("echo [["), true);
        // assert_eq!(will_bash_accept_buffer("echo {{"), true);
    }

    // TODO test ones that will be syntax errors but complete commands
    #[test]
    fn test_syntax_errors() {
        assert_eq!(will_bash_accept_buffer("echo ("), true);
        assert_eq!(will_bash_accept_buffer("echo )"), true);
    }
}
