
use std::collections::HashMap;

use flash::lexer::{Lexer, Token, TokenKind};


#[allow(unused_imports)]
use crate::bash_funcs;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompletionContext {
    FirstWord(
        String, // left part of the word under cursor
    ),
    CommandComp {
        full_command: String, // "git commi asdf" with cursor just after com
        command_word: String,  // "git"
        word_under_cursor: String, // "commi"
        cursor_byte_pos: usize, // 7 since cursor is after "com" in "git com|mi asdf"
        word_under_cursor_byte_end: usize, // 9 since we want the end of "commi"
    },
}

pub fn get_completion_context(buffer: &str, cursor_char_pos: usize) -> Option<CompletionContext> {
    // probably not perfect but good enough

    let extractor = CommandExtractor::new(buffer, cursor_char_pos);
    let extracted = extractor.extract_command();
    if extracted.command.trim().is_empty() {
        return None;
    }
    if extracted.command_until_cursor.split_whitespace().count() == 1 {
        return Some(CompletionContext::FirstWord(
            extracted.command_until_cursor,
        ));
    }

    let command_word = extracted
        .command
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();

   // git commi asdf
   // git com|mi asdf
   let command_until_cursor = extracted.command_until_cursor;
   let cursor_byte_pos = command_until_cursor.len();
    let word_start_pos = command_until_cursor
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(idx, _)| idx + 1);

    let word_end_char_pos = extracted.command
        .char_indices()
        .skip(command_until_cursor.chars().count())
        .find(|(_, c)| c.is_whitespace())
        .map_or(extracted.command.chars().count(), |(idx, _)| idx);
    let word_under_cursor_byte_end = extracted.command
        .char_indices()
        .nth(word_end_char_pos)
        .map_or(extracted.command.len(), |(byte_idx, _)| byte_idx);

    let word_under_cursor: String = extracted
        .command
        .chars()
        .skip(word_start_pos)
        .take(command_until_cursor.chars().count() - word_start_pos)
        .collect();


   Some(CompletionContext::CommandComp {
       full_command: extracted.command,
       command_word,
       word_under_cursor,
       cursor_byte_pos,
       word_under_cursor_byte_end,
   })
}


struct ExtractedCommand {
    command_until_cursor: String,
    command: String,
}

impl ExtractedCommand {
    
}

struct CommandExtractor<'a> {
    input: &'a str,
    tokens: Vec<(Token, usize)>,
    cursor_char: usize,
}



impl<'a> CommandExtractor<'a> {
    fn new(input: &'a str, cursor_char: usize) -> Self {
        let mut i = 0;
        let mut lexer = Lexer::new(input);
        let mut tokens: Vec<(Token, usize)> = Vec::new();

        let line_col_to_char = Self::line_column_to_char_pos(input);

        loop {
            let token = lexer.next_token();
            if token.kind == TokenKind::EOF {
                break;
            }
            let num_chars = input.chars().count();
            let char_pos = *line_col_to_char
                .get(&(token.position.line, token.position.column))
                .unwrap_or(&num_chars);

            tokens.push((token.clone(), char_pos));

            i += 1;
            if i > 99999 {
                panic!("Infinite loop detected in lexer during command extraction");
            }
        }

        Self {
            input,
            tokens,
            cursor_char,
        }
    }

    fn line_column_to_char_pos(input: &str) -> HashMap<(usize, usize), usize> {
        let mut current_line = 1; // flash lexer uses 1 based indexing
        let mut current_column = 1;
        let mut char_pos = 0;
        let mut line_col_map = HashMap::new();

        for c in input.chars() {
            line_col_map.insert((current_line, current_column), char_pos);

            if c == '\n' {
                current_line += 1;
                current_column = 1;
            } else {
                current_column += 1;
            }
            char_pos += 1;
        }

        line_col_map
    }

    fn get_next_token_start(
        &self,
        toks: &mut std::iter::Peekable<std::slice::Iter<(Token, usize)>>,
    ) -> usize {
        toks.peek()
            .map_or(self.input.chars().count(), |(t, pos)| *pos)
    }

    fn nested_opening_satisfied(
        token: &Token,
        current_nesting: Option<&(usize, TokenKind)>,
    ) -> bool {
        if token.kind == TokenKind::Backtick {
            match current_nesting {
                Some((_, TokenKind::Backtick)) => false, // It's a closing backtick
                _ => true,
            }
        } else {
            true
        }
    }

    fn nested_closing_satisfied(
        token: &Token,
        current_nesting: Option<&(usize, TokenKind)>,
        next_token: Option<&&(Token, usize)>,
    ) -> bool {
        let (_, current_nesting) = match current_nesting {
            Some(v) => v,
            None => return false,
        };
        match (&token.kind, current_nesting) {
            (TokenKind::RParen, TokenKind::CmdSubst) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstIn) => true,
            (TokenKind::RParen, TokenKind::ProcessSubstOut) => true,
            (TokenKind::RBrace, TokenKind::ParamExpansion) => true,
            (TokenKind::RParen, TokenKind::ArithSubst)
                if next_token.map_or(false, |(t, _)| t.kind == TokenKind::RParen) =>
            {
                true
            }
            (TokenKind::Backtick, TokenKind::Backtick) => true,
            (TokenKind::DoubleRBracket, TokenKind::DoubleLBracket) => true,
            _ => false,
        }
    }

    pub fn extract_command(&self) -> ExtractedCommand {
        let mut nestings = Vec::new();
        let mut current_command_start = 0;
        let mut current_command_end = 0;
        let mut toks = self.tokens.iter().peekable();

        loop {
            let (token, pos) = match toks.next() {
                Some((t, p)) if t.kind != TokenKind::EOF => (t, p),
                _ => break,
            };

            let break_on_end_of_command = *pos >= self.cursor_char;

            match token.kind {
                TokenKind::Semicolon
                | TokenKind::Newline
                | TokenKind::Pipe
                | TokenKind::And
                | TokenKind::Or => {
                    if break_on_end_of_command {
                        break;
                    }
                    current_command_start = self.get_next_token_start(&mut toks);
                    current_command_end = current_command_start;
                }
                TokenKind::Assignment
                    if toks
                        .peek()
                        .map_or(false, |(t, _)| matches!(t.kind, TokenKind::Word(_))) =>
                {
                    toks.next(); // skip the value token
                    current_command_start = self.get_next_token_start(&mut toks);
                    current_command_end = current_command_start;
                }
                TokenKind::CmdSubst
                | TokenKind::ArithSubst
                | TokenKind::ParamExpansion
                | TokenKind::ProcessSubstIn
                | TokenKind::ProcessSubstOut
                | TokenKind::DoubleLBracket
                | TokenKind::Backtick
                    if Self::nested_opening_satisfied(&token, nestings.last()) =>
                {
                    // Enter nesting
                    nestings.push((current_command_start, token.kind.clone()));
                    current_command_start = self.get_next_token_start(&mut toks);
                    current_command_end = current_command_start;
                }
                TokenKind::RParen
                | TokenKind::RBrace
                | TokenKind::Backtick
                | TokenKind::DoubleRBracket
                    if Self::nested_closing_satisfied(&token, nestings.last(), toks.peek()) =>
                {
                    if break_on_end_of_command {
                        break;
                    }

                    let (start, kind) = nestings.pop().unwrap();
                    if kind == TokenKind::ArithSubst {
                        assert!(
                            toks.peek().unwrap().0.kind == TokenKind::RParen,
                            "expected two RParen tokens"
                        );
                        toks.next(); // consume the extra RParen
                    }
                    current_command_start = start;
                    current_command_end = self.get_next_token_start(&mut toks);
                }
                _ => {
                    // Keep building the current command
                    current_command_end = self.get_next_token_start(&mut toks);
                }
            }
        }
        ExtractedCommand {
            command_until_cursor: self
                .input
                .chars()
                .skip(current_command_start)
                .take(self.cursor_char - current_command_start)
                .collect(),
            command: self
                .input
                .chars()
                .skip(current_command_start)
                .take(current_command_end - current_command_start)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str, cursor_char: usize) -> ExtractedCommand {
        CommandExtractor::new(input, cursor_char).extract_command()
    }

    #[test]
    fn test_command_extraction() {
        let input = r#"git comm café"#;
        let res = run(input, "git comm".chars().count());
        assert_eq!(res.command_until_cursor, "git comm");
        assert_eq!(res.command, "git comm café");
    }

    #[test]
    fn test_with_assignment() {
        let input = r#"VAR=valué ABC=qwe ls -la"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "ls -la");
        assert_eq!(res.command_until_cursor, "ls -la");
    }

    #[test]
    fn test_list_of_commands() {
        let input = r#"git commit -m "Initial 🚀"; ls -la"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "ls -la");
        assert_eq!(res.command_until_cursor, "ls -la");
    }

    #[test]
    fn test_with_pipeline() {
        let input = r#"cat filé.txt | grep "pattern" | sort"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "sort");
        assert_eq!(res.command_until_cursor, "sort");

        let input2 = r#"echo "héllo" && echo "wörld""#;
        let res2 = run(input2, input2.chars().count());
        assert_eq!(res2.command, r#"echo "wörld""#);
        assert_eq!(res2.command_until_cursor, r#"echo "wörld""#);

        let input3 = r#"false || echo "fallback 😅""#;
        let res3 = run(input3, input3.chars().count());
        assert_eq!(res3.command, r#"echo "fallback 😅""#);
        assert_eq!(res3.command_until_cursor, r#"echo "fallback 😅""#);
    }

    #[test]
    fn test_subshell_in_command() {
        let input = r#"echo $(git rev-parse HEAD) résumé"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo $(git rev-parse HEAD) résumé"#);
        assert_eq!(
            res.command_until_cursor,
            r#"echo $(git rev-parse HEAD) résumé"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_subshell_command() {
        let input = r#"echo $(git rev-parse HEAD) café"#;
        let cursor_pos = "echo $(git rev-parse".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_subshell_command() {
        let input = r#"echo $(git rev-parse HEAD) 🎉"#;
        let cursor_pos = "echo $(git rev-parse HEAD".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse HEAD"#);
    }

    #[test]
    fn test_command_at_end_of_subshell() {
        let input = r#"echo $(ls -la)"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "echo $(ls -la)");
        assert_eq!(res.command_until_cursor, "echo $(ls -la)");
    }

    #[test]
    fn test_param_expansion_in_command() {
        let input = r#"echo ${HOME} naïve"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo ${HOME} naïve"#);
        assert_eq!(res.command_until_cursor, r#"echo ${HOME} naïve"#);
    }

    #[test]
    fn test_cursor_in_middle_of_param_expansion() {
        let input = r#"echo ${HOME} asdf"#;
        let cursor_pos = "echo ${HO".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"HOME"#);
        assert_eq!(res.command_until_cursor, "HO");
    }

    #[test]
    fn test_cursor_at_end_of_param_expansion() {
        let input = r#"echo ${HOME} asdf"#;
        let cursor_pos = "echo ${HOME}".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"echo ${HOME} asdf"#);
        assert_eq!(res.command_until_cursor, r#"echo ${HOME}"#);
    }

    #[test]
    fn test_command_at_end_of_param_expansion() {
        let input = r#"ls -la ${PWD}"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"ls -la ${PWD}"#);
        assert_eq!(res.command_until_cursor, r#"ls -la ${PWD}"#);
    }

    #[test]
    fn test_complex_param_expansion() {
        let input = r#"echo ${VAR:-dëfault} test 🎯"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo ${VAR:-dëfault} test 🎯"#);
        assert_eq!(res.command_until_cursor, r#"echo ${VAR:-dëfault} test 🎯"#);
    }

    #[test]
    fn test_cursor_inside_complex_param_expansion() {
        let input = r#"echo ${VAR:-dëfault} tëst"#;
        let cursor_pos = "echo ${VAR:-dëf".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "VAR:-dëfault");
        assert_eq!(res.command_until_cursor, "VAR:-dëf");
    }

    #[test]
    fn test_backtick_substitution_in_command() {
        let input = r#"echo `git rev-parse HEAD` café"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo `git rev-parse HEAD` café"#);
        assert_eq!(
            res.command_until_cursor,
            r#"echo `git rev-parse HEAD` café"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_backtick_command() {
        let input = r#"echo `git rev-parse HEAD` asdf"#;
        let cursor_pos = "echo `git rev-parse".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_backtick_command() {
        let input = r#"echo `git rev-parse HEAD` asdf"#;
        let cursor_pos = "echo `git rev-parse HEAD".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse HEAD"#);
    }

    #[test]
    fn test_command_at_end_of_backtick() {
        let input = r#"echo `ls -la`"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "echo `ls -la`");
        assert_eq!(res.command_until_cursor, "echo `ls -la`");
    }

    #[test]
    fn test_nested_backticks_in_command() {
        let input = r#"echo `echo \`date\`` tëst 🎯"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo `echo \`date\`` tëst 🎯"#);
        assert_eq!(res.command_until_cursor, r#"echo `echo \`date\`` tëst 🎯"#);
    }

    #[test]
    fn test_cursor_in_backtick_with_pipe() {
        let input = r#"echo `ls | grep test` done"#;
        let cursor_pos = "echo `ls | grep".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"grep test"#);
        assert_eq!(res.command_until_cursor, r#"grep"#);
    }

    #[test]
    fn test_arith_subst_in_command() {
        let input = r#"echo $((5 + 3)) rësult 📊"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo $((5 + 3)) rësult 📊"#);
        assert_eq!(res.command_until_cursor, r#"echo $((5 + 3)) rësult 📊"#);
    }

    #[test]
    fn test_cursor_in_middle_of_arith_subst() {
        let input = r#"echo $((5 + 3)) result"#;
        let cursor_pos = "echo $((5 +".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "5 + 3");
        assert_eq!(res.command_until_cursor, "5 +");
    }

    #[test]
    fn test_cursor_at_end_of_arith_subst() {
        let input = r#"echo $((10 * 2)) done"#;
        let cursor_pos = "echo $((10 * 2))".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"echo $((10 * 2)) done"#);
        assert_eq!(res.command_until_cursor, r#"echo $((10 * 2))"#);
    }

    #[test]
    fn test_command_at_end_of_arith_subst() {
        let input = r#"result=$((100 / 5))"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"result=$((100 / 5))"#);
        assert_eq!(res.command_until_cursor, r#"result=$((100 / 5))"#);
    }

    #[test]
    fn test_complex_arith_with_variables() {
        let input = r#"echo $(($VAR + 10)) test"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo $(($VAR + 10)) test"#);
        assert_eq!(res.command_until_cursor, r#"echo $(($VAR + 10)) test"#);
    }

    #[test]
    fn test_cursor_inside_complex_arith() {
        let input = r#"val=$((VAR * 2 + 5))"#;
        let cursor_pos = "val=$((VAR * 2".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "VAR * 2 + 5");
        assert_eq!(res.command_until_cursor, "VAR * 2");
    }

    #[test]
    fn test_nested_arith_operations() {
        let input = r#"echo $(( $(( 5 + 3 )) * 2 )) ënd ✅"#;
        let res = run(input, "echo $(( $(( 5 +".chars().count());
        assert_eq!(res.command, r#"5 + 3 "#);
        assert_eq!(res.command_until_cursor, r#"5 +"#);
    }

    #[test]
    fn test_proc_subst_in_command() {
        let input = r#"diff <(ls /tmp) <(ls /var) résult 🔍"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"diff <(ls /tmp) <(ls /var) résult 🔍"#);
        assert_eq!(
            res.command_until_cursor,
            r#"diff <(ls /tmp) <(ls /var) résult 🔍"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_in() {
        let input = r#"diff <(ls /tmp) <(ls /var) done"#;
        let cursor_pos = "diff <(ls /t".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"ls /tmp"#);
        assert_eq!(res.command_until_cursor, r#"ls /t"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_in() {
        let input = r#"diff <(ls /tmp) <(ls /var) done"#;
        let cursor_pos = "diff <(ls /tmp".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"ls /tmp"#);
        assert_eq!(res.command_until_cursor, r#"ls /tmp"#);
    }

    #[test]
    fn test_command_at_end_of_proc_subst_in() {
        let input = r#"cat <(echo test)"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"cat <(echo test)"#);
        assert_eq!(res.command_until_cursor, r#"cat <(echo test)"#);
    }

    #[test]
    fn test_proc_subst_out_in_command() {
        let input = r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#;
        let res = run(input, input.chars().count());
        assert_eq!(
            res.command,
            r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#
        );
        assert_eq!(
            res.command_until_cursor,
            r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_out() {
        let input = r#"tee >(gzip > file.gz) test"#;
        let cursor_pos = "tee >(gzip > fi".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"gzip > file.gz"#);
        assert_eq!(res.command_until_cursor, r#"gzip > fi"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_out() {
        let input = r#"tee >(cat) done"#;
        let cursor_pos = "tee >(cat".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"cat"#);
        assert_eq!(res.command_until_cursor, r#"cat"#);
    }

    #[test]
    fn test_mixed_proc_subst_in_and_out() {
        let input = r#"cmd <(input cmd) >(output cmd) final"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"cmd <(input cmd) >(output cmd) final"#);
        assert_eq!(
            res.command_until_cursor,
            r#"cmd <(input cmd) >(output cmd) final"#
        );
    }

    #[test]
    fn test_double_bracket_condition() {
        let input = r#"if [[ -f file.txt ]]; then echo found; fi"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "fi");
        assert_eq!(res.command_until_cursor, "fi");
    }

    #[test]
    fn test_cursor_inside_double_bracket() {
        let input = r#"[[ -f filé.txt ]] && echo yës"#;
        let cursor_pos = "[[ -f filé".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "-f filé.txt ");
        assert_eq!(res.command_until_cursor, "-f filé");
    }

    #[test]
    fn test_double_bracket_with_string_comparison() {
        let input = r#"[[ "$var" == "café" ]] && echo match 🎯"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, r#"echo match 🎯"#);
        assert_eq!(res.command_until_cursor, r#"echo match 🎯"#);
    }

    #[test]
    fn test_double_bracket_with_pattern() {
        let input = r#"[[ $file == *.txt ]] || echo "not a text file""#;
        let cursor_pos = "[[ $file == *.txt ]".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "[[ $file == *.txt ]] ");
        assert_eq!(res.command_until_cursor, "[[ $file == *.txt ]");
    }

    #[test]
    fn test_double_bracket_with_regex() {
        let input = r#"[[ $email =~ ^[a-z]+@[a-z]+$ ]]"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
        assert_eq!(res.command_until_cursor, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
    }

    #[test]
    fn test_double_bracket_logical_operators() {
        let input = r#"[[ -f file.txt && -r file.txt ]] && cat file.txt"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "cat file.txt");
        assert_eq!(res.command_until_cursor, "cat file.txt");
    }

    #[test]
    fn test_cursor_before_double_bracket() {
        let input = r#"if [[ -d /path/café ]]; then ls; fi"#;
        let cursor_pos = "if [[ -d /path/caf".chars().count();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "-d /path/café ");
        assert_eq!(res.command_until_cursor, "-d /path/caf");
    }

    #[test]
    fn test_double_bracket_with_emoji() {
        let input = r#"[[ "$msg" == "✅ done" ]] && echo success"#;
        let res = run(input, input.chars().count());
        assert_eq!(res.command, "echo success");
        assert_eq!(res.command_until_cursor, "echo success");
    }
}
