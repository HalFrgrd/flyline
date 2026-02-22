use flash::lexer::TokenKind;

use crate::dparser::{DParser, ToInclusiveRange};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompType {
    FirstWord, // the first word under the cursor. cursor might be in the middle of it

    CommandComp {
        // "git commi asdf" with cursor just after com
        command_word: String, // "git"
    },
    EnvVariable,    // the env variable under the cursor, with the leading $
    TildeExpansion, // the tilde under the cursor, e.g. "~us|erna"
    GlobExpansion,  // the glob pattern under the cursor, e.g. "*.rs|t"
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CompletionContext<'a> {
    pub buffer: &'a str,
    pub context: &'a str,
    pub context_until_cursor: &'a str,
    pub word_under_cursor: &'a str,
    pub comp_type: CompType,
}

impl<'a> CompletionContext<'a> {
    fn classify_word_type(word: &str) -> Option<CompType> {
        if false && word.starts_with('$') {
            Some(CompType::EnvVariable)
        } else if false && word.starts_with('~') && !word.contains("/") {
            Some(CompType::TildeExpansion)
        } else if word.contains('*') || word.contains('?') || word.contains('[') {
            // TODO "*.md will match this. need some better logic here
            Some(CompType::GlobExpansion)
        } else {
            None
        }
    }

    pub fn new(
        buffer: &'a str,
        context_until_cursor: &'a str,
        context: &'a str,
        word_under_cursor: &'a str,
    ) -> Self {
        if cfg!(test) {
            dbg!(&buffer);
            dbg!(&context_until_cursor);
            dbg!(&context);
            dbg!(&word_under_cursor);
        }

        let comp_type = if context.trim().is_empty() {
            CompType::FirstWord
        } else if !context_until_cursor.chars().any(|c| c.is_whitespace()) {
            if let Some(comp_type) = Self::classify_word_type(word_under_cursor) {
                comp_type
            } else {
                CompType::FirstWord
            }
        } else {
            if let Some(comp_type) = Self::classify_word_type(&word_under_cursor) {
                comp_type
            } else {
                CompType::CommandComp {
                    command_word: context.split_whitespace().next().unwrap_or("").to_string(),
                }
            }
        };

        CompletionContext {
            buffer,
            context_until_cursor,
            context,
            word_under_cursor,
            comp_type,
        }
    }
}

pub fn get_completion_context<'a>(
    buffer: &'a str,
    cursor_byte_pos: usize,
) -> CompletionContext<'a> {
    let mut parser = DParser::from(buffer);

    parser.walk(cursor_byte_pos);

    let context_tokens = parser.get_current_command_tokens();

    if cfg!(test) {
        println!("Context tokens:");
        dbg!(cursor_byte_pos);
        for t in context_tokens.iter() {
            println!("{:?} byte_range={:?}", t, t.byte_range());
        }
    }

    // first try and find a non whitespace token that inclusivly contains the cursor.
    // if there is one, that is the word under the cursor.
    // Otherwise allow whitespace tokens to be the word under the cursor.
    // If there still isnt a node, then the word under the cursor is empty and the context is empty.
    let opt_cursor_node = match context_tokens
        .iter()
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace(_)))
        .find(|t| t.byte_range().to_inclusive().contains(&cursor_byte_pos))
    {
        Some(node) => Some(node),
        None => context_tokens
            .iter()
            .find(|t| t.byte_range().to_inclusive().contains(&cursor_byte_pos)),
    };

    let word_under_cursor_range = match opt_cursor_node {
        Some(cursor_node) if matches!(cursor_node.kind, TokenKind::Whitespace(_)) => {
            cursor_byte_pos..cursor_byte_pos
        }
        Some(cursor_node) if matches!(cursor_node.kind, TokenKind::Word(_)) => {
            // try grow to the left if there are single or double quotes
            let mut byte_range = cursor_node.byte_range();

            if byte_range.start > 0 {
                if let Some(prev_char) = buffer[..byte_range.start].chars().rev().next() {
                    if prev_char == '"' || prev_char == '\'' {
                        byte_range.start -= prev_char.len_utf8();
                    }
                }
            }

            byte_range
        }
        Some(cursor_node) => cursor_node.byte_range(),
        None if context_tokens.is_empty() => {
            return CompletionContext::new(buffer, &buffer[0..0], &buffer[0..0], &buffer[0..0]);
        }
        None => {
            todo!("Cursor is outside of all context tokens");
        }
    };

    assert!(
        word_under_cursor_range
            .to_inclusive()
            .contains(&cursor_byte_pos)
    );

    let comp_context_range = if context_tokens
        .iter()
        .all(|t| matches!(t.kind, TokenKind::Whitespace(_)))
    {
        cursor_byte_pos..cursor_byte_pos
    } else {
        context_tokens.first().unwrap().byte_range().start
            ..context_tokens.last().unwrap().byte_range().end
    };

    let context_until_cursor = &buffer[comp_context_range.start..cursor_byte_pos];
    let context = &buffer[comp_context_range];

    let word_under_cursor = &buffer[word_under_cursor_range];

    CompletionContext::new(buffer, context_until_cursor, context, word_under_cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<'a>(input: &'a str, cursor_byte_pos: usize) -> CompletionContext<'a> {
        get_completion_context(input, cursor_byte_pos)
    }

    /// Parse a test string with `█` marking the cursor position.
    /// Returns (input_without_cursor, cursor_byte_pos).
    fn run_inline(input: &str) -> CompletionContext<'static> {
        let cursor_byte_pos = input.find('█').expect("Cursor marker █ not found");
        let input_without_cursor = input.replace('█', "");
        let input_without_cursor: &'static str = Box::leak(input_without_cursor.into_boxed_str());
        run(input_without_cursor, cursor_byte_pos)
    }

    #[test]
    fn test_command_extraction() {
        let res = run_inline(r#"git com█mi café"#);

        assert_eq!(res.context_until_cursor, "git com");
        assert_eq!(res.context, "git commi café");

        match res.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "git");
                assert_eq!(res.word_under_cursor, "commi");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_command_extraction_at_end() {
        let res = run_inline(r#"cd a█ b"#);
        assert_eq!(res.context_until_cursor, "cd a");
        assert_eq!(res.context, "cd a b");

        match res.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(res.word_under_cursor, "a");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_command_extraction_at_end_2() {
        let res = run_inline(r#"cd  █"#);
        assert_eq!(res.context_until_cursor, "cd  ");
        assert_eq!(res.context, "cd  ");

        match res.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(res.word_under_cursor, "");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_with_assignment_basic() {
        let res = run_inline(r#"A=b █ls -la"#);
        assert_eq!(res.context, "ls -la");
        assert_eq!(res.context_until_cursor, "");
        match res.comp_type {
            CompType::FirstWord => {
                assert_eq!(res.word_under_cursor, "ls");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_with_assignment_before_command() {
        let res = run_inline(r#"VAR=valué ABC=qwe   █      ls -la"#);
        assert_eq!(res.context, "");
        assert_eq!(res.context_until_cursor, "");
    }

    #[test]
    fn test_empty_command() {
        let res = run_inline(r#"█"#);
        assert_eq!(res.context, "");
        assert_eq!(res.context_until_cursor, "");
        assert_eq!(res.word_under_cursor, "");
    }

    #[test]
    fn test_whitespace_command() {
        let res = run_inline(r#"   █   "#);
        assert_eq!(res.context, "");
        assert_eq!(res.context_until_cursor, "");
        assert_eq!(res.word_under_cursor, "");
    }

    #[test]
    fn test_with_assignment_at_end() {
        let res = run_inline(r#"VAR=valué ABC=qwe█ ls -la"#);
        assert_eq!(res.context, "ABC=qwe");
        assert_eq!(res.context_until_cursor, "ABC=qwe");
    }

    #[test]
    fn test_list_of_commands() {
        let res = run_inline(r#"git commit -m "Initial "; ls -la█"#);
        assert_eq!(res.context, "ls -la");
        assert_eq!(res.context_until_cursor, "ls -la");
    }

    #[test]
    fn test_cursor_at_start_of_word() {
        let res = run_inline(r#"git █commit"#);
        assert_eq!(res.context, "git commit");
        assert_eq!(res.context_until_cursor, "git ");
        assert_eq!(res.word_under_cursor, "commit");
    }

    #[test]
    fn test_dollar_sign() {
        let res = run_inline(r#"echo $█"#);
        assert_eq!(res.context, "echo $");
        assert_eq!(res.context_until_cursor, "echo $");
        assert_eq!(res.word_under_cursor, "$");
    }

    #[test]
    fn test_dollar_sign_one_letter() {
        let res = run_inline(r#"echo $A█"#);
        assert_eq!(res.context, "echo $A");
        assert_eq!(res.context_until_cursor, "echo $A");
        assert_eq!(res.word_under_cursor, "A");
    }

    #[test]
    fn test_dollar_concatenation() {
        let res = run_inline(r#"echo $A█$B"#);
        assert_eq!(res.context, "echo $A$B");
        assert_eq!(res.context_until_cursor, "echo $A");
        assert_eq!(res.word_under_cursor, "A");

        let res = run_inline(r#"echo $A$█B"#);
        assert_eq!(res.context, "echo $A$B");
        assert_eq!(res.context_until_cursor, "echo $A$");
        assert_eq!(res.word_under_cursor, "$");
    }

    #[test]
    fn test_with_pipeline() {
        let res = run_inline(r#"cat filé.txt | grep "pattern" | sort█"#);
        assert_eq!(res.context, "sort");
        assert_eq!(res.context_until_cursor, "sort");

        let res2 = run_inline(r#"echo "héllo" && echo "wörld"█"#);
        assert_eq!(res2.context, r#"echo "wörld""#);
        assert_eq!(res2.context_until_cursor, r#"echo "wörld""#);

        let res3 = run_inline(r#"false || echo "fallback 😅"█"#);
        assert_eq!(res3.context, r#"echo "fallback 😅""#);
        assert_eq!(res3.context_until_cursor, r#"echo "fallback 😅""#);
    }

    #[test]
    fn test_subshell_in_command() {
        let res = run_inline("echo $(git rev-parse HEAD) résumé█");
        assert_eq!(res.context, "echo $(git rev-parse HEAD) résumé");
        assert_eq!(
            res.context_until_cursor,
            "echo $(git rev-parse HEAD) résumé"
        );

        match res.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(res.context, "echo $(git rev-parse HEAD) résumé");
                assert_eq!(command_word, "echo");
                assert_eq!(res.word_under_cursor, "résumé");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_cursor_in_middle_of_subshell_command() {
        let res = run_inline(r#"echo $(git rev-parse HEA█D) café"#);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor, r#"git rev-parse HEA"#);
    }

    #[test]
    fn test_cursor_at_end_of_subshell_command() {
        let res = run_inline(r#"echo $(git rev-parse HEAD█) 🎉"#);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor, r#"git rev-parse HEAD"#);
    }

    #[test]
    fn test_cursor_just_outside_of_subshell_command() {
        let res = run_inline(r#"echo $(git rev-parse HEAD)█ 🎉"#);
        assert_eq!(res.context, r#"echo $(git rev-parse HEAD) 🎉"#);
        assert_eq!(res.context_until_cursor, r#"echo $(git rev-parse HEAD)"#);
    }

    #[test]
    fn test_command_at_end_of_subshell() {
        let res = run_inline(r#"echo $(ls -la)█"#);
        assert_eq!(res.context, "echo $(ls -la)");
        assert_eq!(res.context_until_cursor, "echo $(ls -la)");
    }

    #[test]
    fn test_param_expansion_in_command() {
        let res = run_inline(r#"echo ${HOME} naïve█"#);
        assert_eq!(res.context, r#"echo ${HOME} naïve"#);
        assert_eq!(res.context_until_cursor, r#"echo ${HOME} naïve"#);
    }

    #[test]
    fn test_cursor_in_middle_of_param_expansion() {
        let res = run_inline(r#"echo ${HO█ME} asdf"#);
        assert_eq!(res.context, r#"HOME"#);
        assert_eq!(res.context_until_cursor, "HO");
    }

    #[test]
    fn test_cursor_at_end_of_param_expansion() {
        let res = run_inline(r#"echo ${HOME█} asdf"#);
        assert_eq!(res.context, "HOME");
        assert_eq!(res.context_until_cursor, "HOME");
    }

    #[test]
    fn test_command_at_end_of_param_expansion() {
        let res = run_inline(r#"ls -la ${PWD}█"#);
        assert_eq!(res.context, "ls -la ${PWD}");
        assert_eq!(res.context_until_cursor, "ls -la ${PWD}");
    }

    #[test]
    fn test_complex_param_expansion() {
        let res = run_inline(r#"echo ${VAR:-dëfault} test 🎯█"#);
        assert_eq!(res.context, r#"echo ${VAR:-dëfault} test 🎯"#);
        assert_eq!(res.context_until_cursor, r#"echo ${VAR:-dëfault} test 🎯"#);
    }

    #[test]
    fn test_cursor_inside_complex_param_expansion() {
        let res = run_inline(r#"echo ${VAR:-dëf█ault} tëst"#);
        assert_eq!(res.context, "VAR:-dëfault");
        assert_eq!(res.context_until_cursor, "VAR:-dëf");
    }

    #[test]
    fn test_backtick_substitution_in_command() {
        let res = run_inline(r#"echo `git rev-parse HEAD` café█"#);
        assert_eq!(res.context, r#"echo `git rev-parse HEAD` café"#);
        assert_eq!(
            res.context_until_cursor,
            r#"echo `git rev-parse HEAD` café"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_backtick_command() {
        let res = run_inline(r#"echo `git rev-parse█ HEAD` asdf"#);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor, r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_backtick_command() {
        let res = run_inline(r#"echo `b c█`"#);
        assert_eq!(res.context, "b c");
        assert_eq!(res.context_until_cursor, "b c");
    }

    #[test]
    fn test_command_at_end_of_backtick() {
        let res = run_inline(r#"echo `ls -la`█ qwe"#);
        assert_eq!(res.context, "echo `ls -la` qwe");
        assert_eq!(res.context_until_cursor, "echo `ls -la`");
    }

    #[test]
    fn test_nested_backticks_in_command() {
        let res = run_inline(r#"echo `echo \`date\`` tëst 🎯█"#);
        assert_eq!(res.context, r#"echo `echo \`date\`` tëst 🎯"#);
        assert_eq!(res.context_until_cursor, r#"echo `echo \`date\`` tëst 🎯"#);
    }

    #[test]
    fn test_cursor_in_backtick_with_pipe() {
        let res = run_inline(r#"echo `ls | grep█ test` done"#);
        assert_eq!(res.context, r#"grep test"#);
        assert_eq!(res.context_until_cursor, r#"grep"#);
    }

    #[test]
    fn test_arith_subst_in_command() {
        let res = run_inline(r#"echo $((5 + 3)) rësult 📊█"#);
        assert_eq!(res.context, r#"echo $((5 + 3)) rësult 📊"#);
        assert_eq!(res.context_until_cursor, r#"echo $((5 + 3)) rësult 📊"#);
    }

    #[test]
    fn test_cursor_in_middle_of_arith_subst() {
        let res = run_inline(r#"echo $((5 + 3█)) result"#);
        assert_eq!(res.context, "5 + 3");
        assert_eq!(res.context_until_cursor, "5 + 3");
    }

    #[test]
    fn test_cursor_in_middle_of_arith_subst_2() {
        let res = run_inline(r#"echo $((5 + 3)█) result"#);
        assert_eq!(res.context, "echo $((5 + 3)) result");
        assert_eq!(res.context_until_cursor, "echo $((5 + 3)");
    }

    #[test]
    fn test_cursor_at_end_of_arith_subst() {
        let res = run_inline(r#"echo $((10 * 2))█ bar"#);
        assert_eq!(res.context, "echo $((10 * 2)) bar");
        assert_eq!(res.context_until_cursor, "echo $((10 * 2))");
    }

    #[test]
    fn test_command_at_mid_end_of_arith_subst() {
        let res = run_inline(r#"result=$((100 / 5)█)"#);
        assert_eq!(res.context, r#"result=$((100 / 5))"#);
        assert_eq!(res.context_until_cursor, r#"result=$((100 / 5)"#);
    }

    #[test]
    fn test_command_at_end_end_of_arith_subst() {
        let res = run_inline(r#"result=$((100 / 5))█"#);
        assert_eq!(res.context, r#"result=$((100 / 5))"#);
        assert_eq!(res.context_until_cursor, r#"result=$((100 / 5))"#);
    }

    #[test]
    fn test_complex_arith_with_variables() {
        let res = run_inline(r#"echo $(($VAR + 10)) test█"#);
        assert_eq!(res.context, r#"echo $(($VAR + 10)) test"#);
        assert_eq!(res.context_until_cursor, r#"echo $(($VAR + 10)) test"#);
    }

    #[test]
    fn test_cursor_inside_complex_arith() {
        let res = run_inline(r#"val=$((VAR * 2█ + 5))"#);
        assert_eq!(res.context, "VAR * 2 + 5");
        assert_eq!(res.context_until_cursor, "VAR * 2");
    }

    #[test]
    fn test_nested_arith_operations() {
        let res = run_inline(r#"echo $(( $(( 5 +█ 3 )) * 2 )) ënd ✅"#);
        assert_eq!(res.context, r#"5 + 3"#);
        assert_eq!(res.context_until_cursor, r#"5 +"#);
    }

    #[test]
    fn test_proc_subst_in_command() {
        let res = run_inline(r#"diff <(ls /tmp) <(ls /var) résult 🔍█"#);
        assert_eq!(res.context, r#"diff <(ls /tmp) <(ls /var) résult 🔍"#);
        assert_eq!(
            res.context_until_cursor,
            r#"diff <(ls /tmp) <(ls /var) résult 🔍"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_in() {
        let res = run_inline(r#"diff <(ls /t█mp) <(ls /var) done"#);
        assert_eq!(res.context, r#"ls /tmp"#);
        assert_eq!(res.context_until_cursor, r#"ls /t"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_in() {
        let res = run_inline(r#"diff <(ls /tmp█) <(ls /var) done"#);
        assert_eq!(res.context, r#"ls /tmp"#);
        assert_eq!(res.context_until_cursor, r#"ls /tmp"#);
    }

    #[test]
    fn test_command_at_end_of_proc_subst_in() {
        let res = run_inline(r#"cat <(echo test)█"#);
        assert_eq!(res.context, r#"cat <(echo test)"#);
        assert_eq!(res.context_until_cursor, r#"cat <(echo test)"#);
    }

    #[test]
    fn test_proc_subst_out_in_command() {
        let res = run_inline(r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉█"#);
        assert_eq!(
            res.context,
            r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#
        );
        assert_eq!(
            res.context_until_cursor,
            r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_out() {
        let res = run_inline(r#"tee >(gzip > fi█le.gz) test"#);
        assert_eq!(res.context, r#"gzip > file.gz"#);
        assert_eq!(res.context_until_cursor, r#"gzip > fi"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_out() {
        let res = run_inline(r#"tee >(cat█) done"#);
        assert_eq!(res.context, r#"cat"#);
        assert_eq!(res.context_until_cursor, r#"cat"#);
    }

    #[test]
    fn test_mixed_proc_subst_in_and_out() {
        let res = run_inline(r#"cmd <(input cmd) >(output cmd) final█"#);
        assert_eq!(res.context, r#"cmd <(input cmd) >(output cmd) final"#);
        assert_eq!(
            res.context_until_cursor,
            r#"cmd <(input cmd) >(output cmd) final"#
        );
    }

    #[test]
    // #[ignore] // Need to think more on what the expected behavior is here
    fn test_double_bracket_condition() {
        let res = run_inline(r#"if [[ -f file.txt ]]; then echo found; fi█"#);
        assert_eq!(res.context, "if [[ -f file.txt ]]; then echo found; fi");
        assert_eq!(
            res.context_until_cursor,
            "if [[ -f file.txt ]]; then echo found; fi"
        );
        assert_eq!(res.word_under_cursor, "fi");
    }

    #[test]
    fn test_cursor_inside_double_bracket() {
        let res = run_inline(r#"[[ -f filé█.txt ]] && echo yës"#);
        assert_eq!(res.context, "-f filé.txt");
        assert_eq!(res.context_until_cursor, "-f filé");
    }

    #[test]
    fn test_double_bracket_with_string_comparison() {
        let res = run_inline(r#"[[ "$var" == "café" ]] && echo match 🎯█"#);
        assert_eq!(res.context, r#"echo match 🎯"#);
        assert_eq!(res.context_until_cursor, r#"echo match 🎯"#);
    }

    #[test]
    fn test_double_bracket_with_pattern() {
        let res = run_inline(r#"[[ $file == *.txt ]█] || echo "not a text file""#);
        assert_eq!(res.context, "[[ $file == *.txt ]]");
        assert_eq!(res.context_until_cursor, "[[ $file == *.txt ]");
    }

    #[test]
    fn test_start_with_subshell() {
        let res = run_inline(r#"$(echo test)█"#);
        assert_eq!(res.context, "$(echo test)");
        assert_eq!(res.context_until_cursor, "$(echo test)");
    }

    #[test]
    fn test_double_bracket_with_regex() {
        let res = run_inline(r#"[[ $email =~ ^[a-z]+@[a-z]+$ ]]█"#);
        assert_eq!(res.context, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
        assert_eq!(res.context_until_cursor, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
    }

    #[test]
    fn test_double_bracket_logical_operators() {
        let res = run_inline(r#"[[ -f file.txt && -r file.txt ]] && cat file.txt█"#);
        assert_eq!(res.context, "cat file.txt");
        assert_eq!(res.context_until_cursor, "cat file.txt");
    }

    #[test]
    fn test_cursor_before_double_bracket() {
        let res = run_inline(r#"if [[ -d /path/caf█é ]]; then ls; fi"#);
        assert_eq!(res.context, "-d /path/café");
        assert_eq!(res.context_until_cursor, "-d /path/caf");
    }

    #[test]
    fn test_double_bracket_with_emoji() {
        let res = run_inline(r#"[[ "$msg" == "✅ done" ]] && echo success█"#);
        assert_eq!(res.context, "echo success");
        assert_eq!(res.context_until_cursor, "echo success");
    }

    // Tests for CompletionContext with various cursor positions and non-ASCII characters

    #[test]
    fn test_completion_context_cursor_at_start_of_line() {
        // Cursor at position 0 (start of line)
        let ctx = run_inline("█café --option 🎯");
        match ctx.comp_type {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor, "café");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_first_word() {
        // Cursor in the middle of first word with non-ASCII
        let ctx = run_inline("caf█é --option 🎯");
        match ctx.comp_type {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor, "café");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_after_first_word_emoji() {
        // Cursor after first word that contains emoji
        let ctx = run_inline("🚀rock█et --verbose naïve");
        match ctx.comp_type {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor, "🚀rocket");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_at_end_of_line() {
        // Cursor at end of line with non-ASCII
        let ctx = run_inline("echo 'Tëst message' résumé 📄█");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "echo 'Tëst message' résumé 📄");
                assert_eq!(command_word, "echo");
                assert_eq!(ctx.word_under_cursor, "📄");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_middle_word_with_unicode() {
        // Cursor in middle of word with unicode characters
        let ctx = run_inline("ls --sïze caf█é 日本語");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "ls --sïze café 日本語");
                assert_eq!(command_word, "ls");
                assert_eq!(ctx.word_under_cursor, "café");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_at_start_chinese_chars() {
        // Cursor at start with Chinese characters
        let ctx = run_inline("█文件 --option värde");
        match ctx.comp_type {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor, "文件");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_middle_chinese() {
        // Cursor in middle of Chinese word
        let ctx = run_inline("git 提█交 --mëssage 'hëllo'");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "git 提交 --mëssage 'hëllo'");
                assert_eq!(command_word, "git");
                assert_eq!(ctx.word_under_cursor, "提交");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_end_arabic_text() {
        // Cursor at end with Arabic text
        let ctx = run_inline("cat مرحبا --öption 🔥█");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "cat مرحبا --öption 🔥");
                assert_eq!(command_word, "cat");
                assert_eq!(ctx.word_under_cursor, "🔥");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_middle_cyrillic() {
        // Cursor in middle of Cyrillic word
        let ctx = run_inline("ls фай█л --süze привет 🎯");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "ls файл --süze привет 🎯");
                assert_eq!(command_word, "ls");
                assert_eq!(ctx.word_under_cursor, "файл");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_blank_space_mixed_scripts() {
        // Cursor on blank space with mixed scripts
        let ctx = run_inline("grep 'pättërn' █файл.txt 日本語 🚀");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "grep 'pättërn' файл.txt 日本語 🚀");
                assert_eq!(command_word, "grep");
                assert_eq!(ctx.word_under_cursor, "файл.txt");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_start_emoji_only() {
        // Cursor at start of emoji-only command
        let ctx = run_inline("█🎉 🎊 🎈 --flâg");
        match ctx.comp_type {
            CompType::FirstWord => {
                assert_eq!(ctx.word_under_cursor, "🎉");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_end_accented_characters() {
        // Cursor at end with heavily accented text
        let ctx = run_inline("find . -näme 'fîlé' -type f 🔍█");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "find . -näme 'fîlé' -type f 🔍");
                assert_eq!(command_word, "find");
                assert_eq!(ctx.word_under_cursor, "🔍");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_space_between_multibyte() {
        // Cursor on space between multibyte characters
        let ctx = run_inline("écho 'mëssagé' █文件 🎨");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "écho 'mëssagé' 文件 🎨");
                assert_eq!(command_word, "écho");
                assert_eq!(ctx.word_under_cursor, "文件");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_middle_thai_text() {
        // Cursor in middle of Thai text
        let ctx = run_inline("cat ไฟ█ล์ --öption วันนี้ 🌟");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "cat ไฟล์ --öption วันนี้ 🌟");
                assert_eq!(command_word, "cat");
                assert_eq!(ctx.word_under_cursor, "ไฟล์");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_under_cursor_with_word_after() {
        // This is the bug: when cursor is at END of word AND there's a word after,
        // word_under_cursor should be the current word, not ""
        // Example: "cd fo[cursor] bar" - word_under_cursor should be "fo", not ""
        let ctx = run_inline("cd fo█ bar");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "fo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_under_cursor_in_middle_with_word_after() {
        // Cursor in the middle of "foo" when "bar" follows
        let ctx = run_inline("cd f█oo bar");

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "foo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_double_quote_1() {
        let ctx = run_inline(r#"cd "foo█"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "\"foo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]

    fn test_word_with_double_quote_2() {
        let ctx = run_inline(r#"cd "foo   asdf█"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "\"foo   asdf");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_double_quote_3() {
        let ctx = run_inline(r#"cd "foo █"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "\"foo ");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_double_quote_4() {
        let ctx = run_inline(r#"echo && cd "foo █"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "\"foo ");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_single_quote_1() {
        let ctx = run_inline(r#"cd 'foo█"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "'foo");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_single_quote_2() {
        let ctx = run_inline(r#"cd 'foo   asdf█"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "'foo   asdf");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_single_quote_3() {
        let ctx = run_inline(r#"echo && cd 'foo   asdf█"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "'foo   asdf");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_backslash_1() {
        let ctx = run_inline(r#"echo && cd foo\█"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "foo\\");
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_word_with_backslash_2() {
        let ctx = run_inline(r#"cd foo\ █"#);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(command_word, "cd");
                assert_eq!(ctx.word_under_cursor, "foo\\ ");
            }
            _ => panic!("Expected CommandComp"),
        }
    }
}
