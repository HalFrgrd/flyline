use super::dparser::{DParser, collect_tokens_include_whitespace};
use flash::lexer::{Token, TokenKind};

pub fn will_bash_accept_buffer(buffer: &str) -> bool {
    // returns true iff bash won't try to get more input to complete the command
    // e.g. unclosed quotes, unclosed parens/braces/brackets, etc.
    // its ok if there are syntax errors, as long as the command is "complete"

    let tokens: Vec<Token> = collect_tokens_include_whitespace(buffer);

    if cfg!(test) {
        println!("Tokens:");
        for token in &tokens {
            println!("{:?}", token);
        }
    }

    if is_function_header_without_body(&tokens) {
        return false;
    }

    if let Some(last_token) = tokens.iter().rev().find(|t| {
        !matches!(
            t.kind,
            TokenKind::Whitespace(_) | TokenKind::Comment | TokenKind::Newline
        )
    }) {
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

fn is_function_header_without_body(tokens: &[Token]) -> bool {
    let non_trivia: Vec<&Token> = tokens
        .iter()
        .filter(|t| {
            !matches!(
                t.kind,
                TokenKind::Whitespace(_) | TokenKind::Comment | TokenKind::Newline
            )
        })
        .collect();

    if non_trivia.is_empty() {
        return false;
    }

    let is_function_keyword = |t: &Token| {
        matches!(t.kind, TokenKind::Function)
            || matches!(&t.kind, TokenKind::Word(w) if w == "function")
    };

    let mut hdr_end_idx = None;
    let n = non_trivia.len();

    // Check Pattern 1: `function name ()` or `function name()`
    if n >= 4
        && is_function_keyword(non_trivia[0])
        && matches!(non_trivia[1].kind, TokenKind::Word(_))
        && matches!(non_trivia[2].kind, TokenKind::LParen)
        && matches!(non_trivia[3].kind, TokenKind::RParen)
    {
        hdr_end_idx = Some(3);
    }
    // Check Pattern 2: `function name`
    else if n >= 2
        && is_function_keyword(non_trivia[0])
        && matches!(non_trivia[1].kind, TokenKind::Word(_))
    {
        if n >= 4
            && matches!(non_trivia[2].kind, TokenKind::LParen)
            && matches!(non_trivia[3].kind, TokenKind::RParen)
        {
            hdr_end_idx = Some(3);
        } else {
            hdr_end_idx = Some(1);
        }
    }
    // Check Pattern 3: `name()`
    else if n >= 3
        && matches!(non_trivia[0].kind, TokenKind::Word(_))
        && matches!(non_trivia[1].kind, TokenKind::LParen)
        && matches!(non_trivia[2].kind, TokenKind::RParen)
    {
        let is_assignment =
            matches!(non_trivia[0].kind, TokenKind::Word(ref name) if name.contains('='));
        if !is_assignment {
            hdr_end_idx = Some(2);
        }
    }

    if let Some(end_idx) = hdr_end_idx {
        // If nothing follows the function header, it lacks a body!
        if end_idx == n - 1 {
            return true;
        }

        // Check token immediately after header
        let next_token = non_trivia[end_idx + 1];
        if matches!(next_token.kind, TokenKind::LParen) {
            // Function body is a subshell `( ... )`.
            // Check if there is a matching RParen closing this subshell.
            let mut depth = 0;
            for t in &non_trivia[end_idx + 1..] {
                if matches!(t.kind, TokenKind::LParen) {
                    depth += 1;
                } else if matches!(t.kind, TokenKind::RParen) {
                    depth -= 1;
                }
            }
            if depth > 0 {
                return true;
            }
        } else if matches!(next_token.kind, TokenKind::LBrace) {
            // Function body is a brace block `{ ... }`.
            // Check if there is a matching RBrace closing this function body.
            let mut depth = 0;
            for t in &non_trivia[end_idx + 1..] {
                if matches!(t.kind, TokenKind::LBrace) {
                    depth += 1;
                } else if matches!(t.kind, TokenKind::RBrace) {
                    depth -= 1;
                }
            }
            if depth > 0 {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unclosed_quotes() {
        assert!(!will_bash_accept_buffer("echo 'hello"));
        assert!(!will_bash_accept_buffer("echo \"hello"));
        assert!(will_bash_accept_buffer("echo '\nhello'"));
        assert!(will_bash_accept_buffer("echo \"\nhello\""));
    }

    #[test]
    fn test_command_substitutions() {
        assert!(!will_bash_accept_buffer("echo $(ls"));
        assert!(will_bash_accept_buffer("echo $(ls)"));
        assert!(!will_bash_accept_buffer("echo $((1 + 2"));
        assert!(!will_bash_accept_buffer("echo $((1 + 2)"));
        assert!(will_bash_accept_buffer("echo $((1 + 2))"));
        assert!(will_bash_accept_buffer("echo $(( ((2) + 2) ))"));
        assert!(will_bash_accept_buffer("(( ((2) + 2) ))"));
        assert!(will_bash_accept_buffer("case $x in (1) echo ;; esac"));
        assert!(will_bash_accept_buffer("echo ${VAR}"));
        assert!(!will_bash_accept_buffer("echo ${VAR"));
        // test backticks
        assert!(!will_bash_accept_buffer("echo `ls"));
        assert!(will_bash_accept_buffer("echo `ls`"));
        // parameter expansion with pattern replacement containing escaped special chars
        assert!(will_bash_accept_buffer(r#"printf "${PWD/#$HOME/\~}""#));
    }

    #[test]
    fn test_here_documents() {
        assert!(!will_bash_accept_buffer("cat <<EOF\nhello"));
        assert!(will_bash_accept_buffer("cat <<EOF\nhello\nEOF"));
        assert!(!will_bash_accept_buffer("cat <<eof\nfoo\neof | bar"));
        assert!(will_bash_accept_buffer("cat <<eof\nfoo\neof | bar\neof"));
    }

    #[test]
    fn test_here_document_variations() {
        // Delimiters with trailing operators/words on same line do not close heredoc
        assert!(!will_bash_accept_buffer("cat <<EOF\nfoo\nEOF && echo ok"));
        assert!(!will_bash_accept_buffer("cat <<EOF\nfoo\nEOF; echo ok"));
        assert!(!will_bash_accept_buffer("cat <<EOF\nfoo\nEOF word"));

        // Leading spaces vs tabs for << vs <<-
        assert!(!will_bash_accept_buffer("cat <<EOF\nfoo\n  EOF"));
        assert!(!will_bash_accept_buffer("cat <<EOF\nfoo\n\tEOF"));
        assert!(will_bash_accept_buffer("cat <<-EOF\nfoo\n\t\tEOF"));
        assert!(!will_bash_accept_buffer("cat <<-EOF\nfoo\n  EOF"));

        // Trailing comments on delimiter line are allowed
        assert!(will_bash_accept_buffer("cat <<EOF\nfoo\nEOF # comment"));

        // Empty heredoc body
        assert!(will_bash_accept_buffer("cat <<EOF\nEOF"));

        // Piped heredoc on header line
        assert!(will_bash_accept_buffer("cat <<EOF | grep foo\nbar\nEOF"));
    }

    #[test]
    fn test_here_documents_quoted_delimiter() {
        // Single-quoted delimiter: closing line is the bare word.
        assert!(!will_bash_accept_buffer("cat <<'EOF'\nhello"));
        assert!(will_bash_accept_buffer("cat <<'EOF'\nhello\nEOF"));

        // Double-quoted delimiter: closing line is the bare word.
        assert!(!will_bash_accept_buffer("cat <<\"EOF\"\nhello"));
        assert!(will_bash_accept_buffer("cat <<\"EOF\"\nhello\nEOF"));

        // Backslash-escaped delimiter: closing line is the bare word.
        assert!(!will_bash_accept_buffer("cat <<\\EOF\nhello"));
        assert!(will_bash_accept_buffer("cat <<\\EOF\nhello\nEOF"));

        // Partially-quoted delimiter: E'O'F closes with EOF.
        assert!(!will_bash_accept_buffer("cat <<E'O'F\nhello"));
        assert!(will_bash_accept_buffer("cat <<E'O'F\nhello\nEOF"));

        // Heredoc-dash with quoted delimiter.
        assert!(!will_bash_accept_buffer("cat <<-'EOF'\nhello"));
        assert!(will_bash_accept_buffer("cat <<-'EOF'\nhello\nEOF"));

        // Heredoc followed by a single quote opener
        assert!(!will_bash_accept_buffer("cat <<<'EOF''\nhello\nEOF"));
        // You need to first close the single quote before you can close the heredoc
        assert!(will_bash_accept_buffer(
            "cat <<<'EOF''\nhello\nEOF'\nfoo\nEOF"
        ));
    }

    #[test]
    fn test_interleaved_heredocs_fifo() {
        // Delimiters must close in the order they appear (FIFO), not nested.
        let interleaved = "cat <<A <<-B\nline1\nB\nline2\nA\n";
        assert!(!will_bash_accept_buffer(interleaved));

        let ordered = "cat <<A <<-B\nline1\nA\nline2\nB\n";
        assert!(will_bash_accept_buffer(ordered));
    }

    #[test]
    fn test_if_then_fi() {
        assert!(!will_bash_accept_buffer("if true; then echo hi"));
        assert!(will_bash_accept_buffer("if true; then echo hi; fi"));

        // test if-elif-else-fi
        assert!(!will_bash_accept_buffer(
            "if true; then echo hi; elif false; then echo bye"
        ));
        assert!(will_bash_accept_buffer(
            "if true; then echo hi; elif false; then echo bye; else echo meh; fi"
        ));
    }

    #[test]
    fn test_for_loops() {
        assert!(!will_bash_accept_buffer("for i in 1 2 3; do echo $i"));
        assert!(will_bash_accept_buffer("for i in 1 2 3; do echo $i; done"));
    }

    #[test]
    fn test_while_loops() {
        assert!(!will_bash_accept_buffer("while true; do echo hi"));
        assert!(will_bash_accept_buffer("while true; do echo hi; done"));
    }

    #[test]
    fn test_case_statements() {
        assert!(!will_bash_accept_buffer("case $var in pattern) echo hi"));
        assert!(will_bash_accept_buffer(
            "case $var in pattern) echo hi ;; esac"
        ));
    }

    #[test]
    fn test_nested_structures() {
        assert!(!will_bash_accept_buffer("echo ( ${ )"));
        assert!(will_bash_accept_buffer("echo ( ${ } )"));
    }

    #[test]
    fn test_endings() {
        assert!(!will_bash_accept_buffer("echo hello |"));
        assert!(will_bash_accept_buffer("echo hello | grep h"));

        assert!(!will_bash_accept_buffer("echo hello ||"));
        assert!(will_bash_accept_buffer("echo hello || grep h"));

        assert!(!will_bash_accept_buffer("echo hello &&"));
        assert!(will_bash_accept_buffer("echo hello && grep h"));
    }

    #[test]
    fn test_comments() {
        assert!(will_bash_accept_buffer("echo hello # ' this is a comment"));
        assert!(will_bash_accept_buffer(
            "echo hello # ' this is a comment\n"
        ));
        assert!(!will_bash_accept_buffer("clear# test '"));
    }

    #[test]
    fn test_process_substitution() {
        assert!(!will_bash_accept_buffer("diff <(ls) <(pwd"));
        assert!(will_bash_accept_buffer("diff <(ls) <(pwd)"));
    }

    #[test]
    fn test_ext_glob() {
        assert!(!will_bash_accept_buffer("shopt -s extglob; echo @(a|b"));
        assert!(will_bash_accept_buffer("shopt -s extglob; echo @(a|b)"));
    }

    #[test]
    fn test_function_def() {
        assert!(!will_bash_accept_buffer("my_func() { echo hello"));
        assert!(will_bash_accept_buffer("my_func() { echo hello; }"));

        // Function definition without body expects more input
        assert!(!will_bash_accept_buffer("my_func()"));
        assert!(!will_bash_accept_buffer("my_func() "));
        assert!(!will_bash_accept_buffer("x() ("));
        assert!(!will_bash_accept_buffer("function my_func"));
        assert!(!will_bash_accept_buffer("function my_func()"));
        assert!(!will_bash_accept_buffer("function my_func ()"));

        // Function definitions with comments after header
        assert!(!will_bash_accept_buffer("my_func() # comment\n"));
        assert!(!will_bash_accept_buffer("function my_func # comment\n"));

        // One-line subshell function bodies
        assert!(will_bash_accept_buffer("my_func() ( echo hello )"));
        assert!(will_bash_accept_buffer("function my_func ( echo hello )"));

        // Multiline function definitions with complete body are accepted
        assert!(will_bash_accept_buffer("my_func() {\n  echo hello\n}"));
        assert!(will_bash_accept_buffer("x() (\n  echo hello\n)"));
        assert!(will_bash_accept_buffer(
            "function my_func {\n  echo hello\n}"
        ));
        assert!(will_bash_accept_buffer(
            "function my_func() {\n  echo hello\n}"
        ));
        assert!(will_bash_accept_buffer(
            "function my_func () {\n  echo hello\n}"
        ));

        // Array assignments must remain complete and not be confused with functions
        assert!(will_bash_accept_buffer("arr=()"));
        assert!(will_bash_accept_buffer("arr=( 1 2 3 )"));

        // Multiline function definitions with incomplete body expect more input
        assert!(!will_bash_accept_buffer("my_func() {\n  echo hello"));
        assert!(!will_bash_accept_buffer("function my_func {\n  echo hello"));
    }

    #[test]
    fn test_multiple_heredocs() {
        assert!(!will_bash_accept_buffer(
            "cat <<EOF1  <<EOF2\nhello\nEOF1\nworld\n"
        ));
        assert!(will_bash_accept_buffer(
            "cat <<EOF1  <<EOF2\nhello\nEOF1\nworld\nEOF2"
        ));
    }

    #[test]
    fn test_line_continuation_basic() {
        // Basic line continuation at end of line
        assert!(!will_bash_accept_buffer("echo hello \\"));
        assert!(will_bash_accept_buffer("echo hello \\\nworld"));

        // Line continuation with trailing whitespace (tricky!)
        assert!(!will_bash_accept_buffer("echo hello \\  "));
        assert!(!will_bash_accept_buffer("echo hello \\\t"));

        assert!(will_bash_accept_buffer("printf '\\\\'"));
    }

    #[test]
    fn test_line_continuation_in_strings() {
        // Line continuation inside double quotes - bash still expects more input
        assert!(!will_bash_accept_buffer("echo \"hello \\"));
        assert!(will_bash_accept_buffer("echo \"hello \\\nworld\""));

        // Multiple line continuations in a complex command
        assert!(!will_bash_accept_buffer(
            "if [ \"$var\" = \"value\" ] && \\"
        ));
        assert!(will_bash_accept_buffer(
            "if [ \"$var\" = \"value\" ] && \\\n   [ \"$other\" = \"test\" ]; then echo ok; fi"
        ));

        // Line continuation before pipe (very tricky edge case)
        assert!(!will_bash_accept_buffer("echo hello \\\n|"));
        assert!(will_bash_accept_buffer("echo hello \\\n| grep l"));
    }

    #[test]
    fn test_line_continuation_edge_cases() {
        // Line continuation in command substitution
        assert!(!will_bash_accept_buffer("echo $(ls \\"));
        assert!(will_bash_accept_buffer("echo $(ls \\\n-la)"));

        // Line continuation with heredoc (super tricky!)
        assert!(!will_bash_accept_buffer("cat <<EOF \\"));
        assert!(will_bash_accept_buffer("cat <<EOF \\\nhello\nEOF"));

        // Multiple backslashes - only the last one matters for continuation
        assert!(!will_bash_accept_buffer("echo hello\\\\\\"));
        assert!(will_bash_accept_buffer("echo hello\\\\")); // Even number of backslashes = no continuation

        // Line continuation in function definition
        assert!(!will_bash_accept_buffer("function test() { \\"));
        assert!(will_bash_accept_buffer("function test() { \\\necho hi; }"));
    }

    #[test]
    fn test_unrecognised_tokens() {
        assert!(will_bash_accept_buffer("echo }"));
        assert!(will_bash_accept_buffer("echo ]"));

        assert!(will_bash_accept_buffer("echo {"));
        assert!(will_bash_accept_buffer("echo ./foo/{"));
        assert!(will_bash_accept_buffer("echo {a,b"));
        assert!(will_bash_accept_buffer("echo ["));
        assert!(will_bash_accept_buffer("echo [x"));
    }

    #[test]
    fn test_group_commands() {
        assert!(!will_bash_accept_buffer("{ echo hello;"));
        assert!(will_bash_accept_buffer("{ echo hello; }"));
        assert!(!will_bash_accept_buffer("echo a; { echo b;"));
        assert!(will_bash_accept_buffer("echo a; { echo b; }"));
        assert!(!will_bash_accept_buffer("echo a && { echo b;"));
        assert!(will_bash_accept_buffer("echo a && { echo b; }"));
    }

    // TODO test ones that will be syntax errors but complete commands
    #[test]
    fn test_syntax_errors() {
        assert!(will_bash_accept_buffer("echo ("));
        assert!(will_bash_accept_buffer("echo )"));
        assert!(will_bash_accept_buffer("echo [("));
    }

    #[test]
    fn test_single_bracket_test_command() {
        // `[ foo` is a syntactically complete command (the `[` builtin will run
        // and complain at runtime, but bash does not ask for more input).
        // `[` must therefore not introduce a nesting that needs `]` to close.
        assert!(will_bash_accept_buffer("[ foo"));
        assert!(will_bash_accept_buffer("[ -f file ]"));
    }

    #[test]
    fn test_double_bracket_needs_closing() {
        // `[[ ... ]]` is a real conditional expression and must be closed.
        assert!(!will_bash_accept_buffer("[[ 1 == 1"));
        assert!(will_bash_accept_buffer("[[ 1 == 1 ]]"));
    }

    #[test]
    fn test_array_and_argument_brackets() {
        // `[x]` as an argument or array index should be accepted by bash immediately
        assert!(will_bash_accept_buffer("echo [x]"));
        assert!(will_bash_accept_buffer("echo [x"));
        assert!(will_bash_accept_buffer("echo ${arr[0]}"));
        assert!(will_bash_accept_buffer("echo $[1+1]"));
    }

    #[test]
    fn test_quote_start_mid_word() {
        assert!(!will_bash_accept_buffer(r#"a ['"#));
        assert!(!will_bash_accept_buffer(r#"a [""#));
    }

    #[test]
    fn test_multiline_ands() {
        assert!(!will_bash_accept_buffer("echo && \n"));
    }
}
