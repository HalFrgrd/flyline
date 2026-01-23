use crate::text_buffer::SubString;
use tree_sitter::{Node, Parser};
use tree_sitter_bash;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompType {
    FirstWord(
        SubString, // the first word under the cursor. cursor might be in the middle of it
    ),
    CommandComp {
        full_command: String,         // "git commi asdf" with cursor just after com
        command_word: String,         // "git"
        word_under_cursor: SubString, // "commi"
        cursor_byte_pos: usize,       // 7 since cursor is after "com" in "git com|mi asdf"
    },
    CursorOnBlank(SubString),  // the blank space under the cursor
    EnvVariable(SubString),    // the env variable under the cursor, with the leading $
    TildeExpansion(SubString), // the tilde under the cursor, e.g. "~us|erna"
    GlobExpansion(SubString),  // the glob pattern under the cursor, e.g. "*.rs|t"
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CompletionContext<'a> {
    pub buffer: &'a str,
    pub command_until_cursor: &'a str,
    pub command: &'a str,
    pub comp_type: CompType,
}

impl<'a> CompletionContext<'a> {
    fn classify_word_type(word: &SubString) -> Option<CompType> {
        if word.s.starts_with('$') {
            Some(CompType::EnvVariable(word.clone()))
        } else if word.s.starts_with('~') && !word.s.contains("/") {
            Some(CompType::TildeExpansion(word.clone()))
        } else if word.s.contains('*') || word.s.contains('?') || word.s.contains('[') {
            // TODO "*.md will match this. need some better logic here
            Some(CompType::GlobExpansion(word.clone()))
        } else {
            None
        }
    }

    pub fn new(buffer: &'a str, command_until_cursor: &'a str, command: &'a str) -> Self {
        let comp_type = if false && command_until_cursor.ends_with(char::is_whitespace) {
            let cursor_white_space = match command_until_cursor.char_indices().next_back() {
                Some((byte, _)) => &command_until_cursor[byte..],
                None => "",
            };

            CompType::CursorOnBlank(SubString::new(buffer, cursor_white_space).unwrap())
        } else if command.trim().is_empty()
            || !command_until_cursor.chars().any(|c| c.is_whitespace())
        {
            let first_word =
                SubString::new(buffer, command.split_whitespace().next().unwrap_or("")).unwrap();
            if let Some(comp_type) = Self::classify_word_type(&first_word) {
                comp_type
            } else {
                CompType::FirstWord(first_word)
            }
        } else {
            let cursor_byte_pos = command_until_cursor.len();
            let word_under_cursor =
                crate::text_buffer::extract_word_at_byte(command, cursor_byte_pos);

            if let Some(comp_type) = Self::classify_word_type(&word_under_cursor) {
                comp_type
            } else {
                CompType::CommandComp {
                    full_command: command.to_string(),
                    command_word: command.split_whitespace().next().unwrap_or("").to_string(),
                    word_under_cursor: word_under_cursor,
                    cursor_byte_pos,
                }
            }
        };

        CompletionContext {
            buffer,
            command_until_cursor,
            command,
            comp_type,
        }
    }
}

pub fn get_completion_context<'a>(
    buffer: &'a str,
    cursor_byte_pos: usize,
) -> CompletionContext<'a> {
    extract_command_with_tree_sitter(buffer, cursor_byte_pos)
}

// Very useful
// https://tree-sitter.github.io/tree-sitter/7-playground.html

fn extract_command_with_tree_sitter<'a>(
    buffer: &'a str,
    cursor_byte_pos: usize,
) -> CompletionContext<'a> {
    let mut parser = Parser::new();
    let language = tree_sitter_bash::LANGUAGE.into();
    parser
        .set_language(&language)
        .expect("Error loading bash grammar");

    let tree = parser.parse(buffer, None).expect("Failed to parse buffer");
    let root_node = tree.root_node();

    // Find the deepest node that contains the cursor
    let cursor_node = find_deepest_node_at_position(&root_node, cursor_byte_pos);

    // Find the command context for this cursor position
    let (command_start, command_end) = find_command_bounds_from_node(&cursor_node);

    let command = &buffer[command_start..command_end];
    let command_until_cursor = if cursor_byte_pos > command_start {
        &buffer[command_start..cursor_byte_pos.min(command_end)]
    } else {
        ""
    };

    CompletionContext::new(buffer, command_until_cursor, command)
}

fn find_deepest_node_at_position<'a>(node: &Node<'a>, cursor_byte_pos: usize) -> Node<'a> {
    // If cursor is not within this node, return the node itself
    if cursor_byte_pos < node.start_byte() || cursor_byte_pos > node.end_byte() {
        return *node;
    }

    // Check children to find the deepest node containing the cursor
    for child in node.children(&mut node.walk()) {
        if !child.is_named() {
            // This prevents matching on punctuation nodes like ; or & or ))
            continue;
        }
        if cursor_byte_pos >= child.start_byte() && cursor_byte_pos <= child.end_byte() {
            return find_deepest_node_at_position(&child, cursor_byte_pos);
        }
    }

    // No child contains the cursor, so this is the deepest node
    *node
}

fn find_command_bounds_from_node(cursor_node: &Node) -> (usize, usize) {
    let mut current_node = *cursor_node;

    // Traverse up the tree to find the appropriate command context
    loop {
        dbg!(&current_node);
        dbg!(&current_node.parent());

        let parent = match current_node.parent() {
            Some(p) => p,
            None => {
                return (current_node.start_byte(), current_node.end_byte());
            }
        };

        match parent.kind() {
            "command" => {
                return (parent.start_byte(), parent.end_byte());
            }

            "program"
            | "pipeline"
            | "command_substitution"
            | "test_command"
            | "arithmetic_expansion"
            | "expansion"
            | "process_substitution" => {
                return (current_node.start_byte(), current_node.end_byte());
            }

            _ => {
                current_node = parent;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<'a>(input: &'a str, cursor_byte_pos: usize) -> CompletionContext<'a> {
        get_completion_context(input, cursor_byte_pos)
    }

    #[test]
    fn test_command_extraction() {
        let input = r#"git commi café"#;
        let res = run(input, "git com".len());
        assert_eq!(res.command_until_cursor, "git com");
        assert_eq!(res.command, "git commi café");

        match res.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "git commi café");
                assert_eq!(command_word, "git");
                assert_eq!(word_under_cursor.s, "commi");
                assert_eq!(cursor_byte_pos, "git com".len());
                assert_eq!(word_under_cursor.end, "git commi".len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_with_assignment() {
        let input = r#"VAR=valué ABC=qwe ls -la"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "ls -la");
        assert_eq!(res.command_until_cursor, "ls -la");
    }

    #[test]
    fn test_list_of_commands() {
        let input = r#"git commit -m "Initial 🚀"; ls -la"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "ls -la");
        assert_eq!(res.command_until_cursor, "ls -la");
    }

    #[test]
    fn test_with_pipeline() {
        let input = r#"cat filé.txt | grep "pattern" | sort"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "sort");
        assert_eq!(res.command_until_cursor, "sort");

        let input2 = r#"echo "héllo" && echo "wörld""#;
        let res2 = run(input2, input2.len());
        assert_eq!(res2.command, r#"echo "wörld""#);
        assert_eq!(res2.command_until_cursor, r#"echo "wörld""#);

        let input3 = r#"false || echo "fallback 😅""#;
        let res3 = run(input3, input3.len());
        assert_eq!(res3.command, r#"echo "fallback 😅""#);
        assert_eq!(res3.command_until_cursor, r#"echo "fallback 😅""#);
    }

    #[test]
    fn test_subshell_in_command() {
        let input = "echo $(git rev-parse HEAD) résumé";
        let res = run(input, input.len());
        assert_eq!(res.command, "echo $(git rev-parse HEAD) résumé");
        assert_eq!(
            res.command_until_cursor,
            "echo $(git rev-parse HEAD) résumé"
        );

        match res.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "echo $(git rev-parse HEAD) résumé");
                assert_eq!(command_word, "echo");
                assert_eq!(word_under_cursor.s, "résumé");
                assert_eq!(cursor_byte_pos, input.len());
                assert_eq!(word_under_cursor.end, input.len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_cursor_in_middle_of_subshell_command() {
        let input = r#"echo $(git rev-parse HEAD) café"#;
        let cursor_pos = "echo $(git rev-parse".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_subshell_command() {
        let input = r#"echo $(git rev-parse HEAD) 🎉"#;
        let cursor_pos = "echo $(git rev-parse HEAD".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse HEAD"#);
    }

    #[test]
    fn test_command_at_end_of_subshell() {
        let input = r#"echo $(ls -la)"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "echo $(ls -la)");
        assert_eq!(res.command_until_cursor, "echo $(ls -la)");
    }

    #[test]
    fn test_param_expansion_in_command() {
        let input = r#"echo ${HOME} naïve"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"echo ${HOME} naïve"#);
        assert_eq!(res.command_until_cursor, r#"echo ${HOME} naïve"#);
    }

    #[test]
    fn test_cursor_in_middle_of_param_expansion() {
        let input = r#"echo ${HOME} asdf"#;
        let cursor_pos = "echo ${HO".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"HOME"#);
        assert_eq!(res.command_until_cursor, "HO");
    }

    #[test]
    fn test_cursor_at_end_of_param_expansion() {
        let input = r#"echo ${HOME} asdf"#;
        let cursor_pos = "echo ${HOME}".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"echo ${HOME} asdf"#);
        assert_eq!(res.command_until_cursor, r#"echo ${HOME}"#);
    }

    #[test]
    fn test_command_at_end_of_param_expansion() {
        let input = r#"ls -la ${PWD}"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"ls -la ${PWD}"#);
        assert_eq!(res.command_until_cursor, r#"ls -la ${PWD}"#);
    }

    #[test]
    fn test_complex_param_expansion() {
        let input = r#"echo ${VAR:-dëfault} test 🎯"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"echo ${VAR:-dëfault} test 🎯"#);
        assert_eq!(res.command_until_cursor, r#"echo ${VAR:-dëfault} test 🎯"#);
    }

    #[test]
    fn test_cursor_inside_complex_param_expansion() {
        let input = r#"echo ${VAR:-dëfault} tëst"#;
        let cursor_pos = "echo ${VAR:-dëf".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "dëfault");
        assert_eq!(res.command_until_cursor, "dëf");
    }

    #[test]
    fn test_backtick_substitution_in_command() {
        let input = r#"echo `git rev-parse HEAD` café"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"echo `git rev-parse HEAD` café"#);
        assert_eq!(
            res.command_until_cursor,
            r#"echo `git rev-parse HEAD` café"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_backtick_command() {
        let input = r#"echo `git rev-parse HEAD` asdf"#;
        let cursor_pos = "echo `git rev-parse".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_backtick_command() {
        let input = r#"echo `git rev-parse HEAD` asdf"#;
        let cursor_pos = "echo `git rev-parse HEAD".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"git rev-parse HEAD"#);
        assert_eq!(res.command_until_cursor, r#"git rev-parse HEAD"#);
    }

    #[test]
    fn test_command_at_end_of_backtick() {
        let input = r#"echo `ls -la`"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "echo `ls -la`");
        assert_eq!(res.command_until_cursor, "echo `ls -la`");
    }

    #[test]
    fn test_nested_backticks_in_command() {
        let input = r#"echo `echo \`date\`` tëst 🎯"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"echo `echo \`date\`` tëst 🎯"#);
        assert_eq!(res.command_until_cursor, r#"echo `echo \`date\`` tëst 🎯"#);
    }

    #[test]
    fn test_cursor_in_backtick_with_pipe() {
        let input = r#"echo `ls | grep test` done"#;
        let cursor_pos = "echo `ls | grep".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"grep test"#);
        assert_eq!(res.command_until_cursor, r#"grep"#);
    }

    #[test]
    fn test_arith_subst_in_command() {
        let input = r#"echo $((5 + 3)) rësult 📊"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"echo $((5 + 3)) rësult 📊"#);
        assert_eq!(res.command_until_cursor, r#"echo $((5 + 3)) rësult 📊"#);
    }

    #[test]
    fn test_cursor_in_middle_of_arith_subst() {
        let input = r#"echo $((5 + 3)) result"#;
        let cursor_pos = "echo $((5 +".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "5 + 3");
        assert_eq!(res.command_until_cursor, "5 +");
    }

    #[test]
    fn test_cursor_at_end_of_arith_subst() {
        let input = r#"echo $((10 * 2)) done"#;
        let cursor_pos = "echo $((10 * 2))".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"echo $((10 * 2)) done"#);
        assert_eq!(res.command_until_cursor, r#"echo $((10 * 2))"#);
    }

    #[test]
    fn test_command_at_end_of_arith_subst() {
        let input = r#"result=$((100 / 5))"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"result=$((100 / 5))"#);
        assert_eq!(res.command_until_cursor, r#"result=$((100 / 5))"#);
    }

    #[test]
    fn test_complex_arith_with_variables() {
        let input = r#"echo $(($VAR + 10)) test"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"echo $(($VAR + 10)) test"#);
        assert_eq!(res.command_until_cursor, r#"echo $(($VAR + 10)) test"#);
    }

    #[test]
    fn test_cursor_inside_complex_arith() {
        let input = r#"val=$((VAR * 2 + 5))"#;
        let cursor_pos = "val=$((VAR * 2".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "VAR * 2 + 5");
        assert_eq!(res.command_until_cursor, "VAR * 2");
    }

    #[test]
    fn test_nested_arith_operations() {
        let input = r#"echo $(( $(( 5 + 3 )) * 2 )) ënd ✅"#;
        let res = run(input, "echo $(( $(( 5 +".len());
        assert_eq!(res.command, r#"5 + 3 "#);
        assert_eq!(res.command_until_cursor, r#"5 +"#);
    }

    #[test]
    fn test_proc_subst_in_command() {
        let input = r#"diff <(ls /tmp) <(ls /var) résult 🔍"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"diff <(ls /tmp) <(ls /var) résult 🔍"#);
        assert_eq!(
            res.command_until_cursor,
            r#"diff <(ls /tmp) <(ls /var) résult 🔍"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_in() {
        let input = r#"diff <(ls /tmp) <(ls /var) done"#;
        let cursor_pos = "diff <(ls /t".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"ls /tmp"#);
        assert_eq!(res.command_until_cursor, r#"ls /t"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_in() {
        let input = r#"diff <(ls /tmp) <(ls /var) done"#;
        let cursor_pos = "diff <(ls /tmp".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"ls /tmp"#);
        assert_eq!(res.command_until_cursor, r#"ls /tmp"#);
    }

    #[test]
    fn test_command_at_end_of_proc_subst_in() {
        let input = r#"cat <(echo test)"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"cat <(echo test)"#);
        assert_eq!(res.command_until_cursor, r#"cat <(echo test)"#);
    }

    #[test]
    fn test_proc_subst_out_in_command() {
        let input = r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#;
        let res = run(input, input.len());
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
        let cursor_pos = "tee >(gzip > fi".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"gzip > file.gz"#);
        assert_eq!(res.command_until_cursor, r#"gzip > fi"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_out() {
        let input = r#"tee >(cat) done"#;
        let cursor_pos = "tee >(cat".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, r#"cat"#);
        assert_eq!(res.command_until_cursor, r#"cat"#);
    }

    #[test]
    fn test_mixed_proc_subst_in_and_out() {
        let input = r#"cmd <(input cmd) >(output cmd) final"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"cmd <(input cmd) >(output cmd) final"#);
        assert_eq!(
            res.command_until_cursor,
            r#"cmd <(input cmd) >(output cmd) final"#
        );
    }

    #[test]
    fn test_double_bracket_condition() {
        let input = r#"if [[ -f file.txt ]]; then echo found; fi"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "fi");
        assert_eq!(res.command_until_cursor, "fi");
    }

    #[test]
    fn test_cursor_inside_double_bracket() {
        let input = r#"[[ -f filé.txt ]] && echo yës"#;
        let cursor_pos = "[[ -f filé".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "-f filé.txt ");
        assert_eq!(res.command_until_cursor, "-f filé");
    }

    #[test]
    fn test_double_bracket_with_string_comparison() {
        let input = r#"[[ "$var" == "café" ]] && echo match 🎯"#;
        let res = run(input, input.len());
        assert_eq!(res.command, r#"echo match 🎯"#);
        assert_eq!(res.command_until_cursor, r#"echo match 🎯"#);
    }

    #[test]
    fn test_double_bracket_with_pattern() {
        let input = r#"[[ $file == *.txt ]] || echo "not a text file""#;
        let cursor_pos = "[[ $file == *.txt ]".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "[[ $file == *.txt ]] ");
        assert_eq!(res.command_until_cursor, "[[ $file == *.txt ]");
    }

    #[test]
    fn test_double_bracket_with_regex() {
        let input = r#"[[ $email =~ ^[a-z]+@[a-z]+$ ]]"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
        assert_eq!(res.command_until_cursor, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
    }

    #[test]
    fn test_double_bracket_logical_operators() {
        let input = r#"[[ -f file.txt && -r file.txt ]] && cat file.txt"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "cat file.txt");
        assert_eq!(res.command_until_cursor, "cat file.txt");
    }

    #[test]
    fn test_cursor_before_double_bracket() {
        let input = r#"if [[ -d /path/café ]]; then ls; fi"#;
        let cursor_pos = "if [[ -d /path/caf".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.command, "-d /path/café");
        assert_eq!(res.command_until_cursor, "-d /path/caf");
    }

    #[test]
    fn test_double_bracket_with_emoji() {
        let input = r#"[[ "$msg" == "✅ done" ]] && echo success"#;
        let res = run(input, input.len());
        assert_eq!(res.command, "echo success");
        assert_eq!(res.command_until_cursor, "echo success");
    }

    // Tests for CompletionContext with various cursor positions and non-ASCII characters

    #[test]
    fn test_completion_context_cursor_at_start_of_line() {
        // Cursor at position 0 (start of line)
        let input = "café --option 🎯";
        let ctx = get_completion_context(input, 0);
        match ctx.comp_type {
            CompType::FirstWord(cursor_word) => {
                assert_eq!(cursor_word.s, "café");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_first_word() {
        // Cursor in the middle of first word with non-ASCII
        let input = "café --option 🎯";
        let cursor_pos = "caf".len();
        let ctx = get_completion_context(input, cursor_pos);
        match ctx.comp_type {
            CompType::FirstWord(cursor_word) => {
                assert_eq!(cursor_word.s, "café");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_after_first_word_emoji() {
        // Cursor after first word that contains emoji
        let input = "🚀rocket --verbose naïve";
        let cursor_pos = "🚀rock".len();
        let ctx = get_completion_context(input, cursor_pos);
        match ctx.comp_type {
            CompType::FirstWord(cursor_word) => {
                assert_eq!(cursor_word.s, "🚀rocket");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_at_end_of_line() {
        // Cursor at end of line with non-ASCII
        let input = "echo 'Tëst message' résumé 📄";
        let cursor_pos = input.len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "echo 'Tëst message' résumé 📄");
                assert_eq!(command_word, "echo");
                assert_eq!(word_under_cursor.s, "📄");
                assert_eq!(cursor_byte_pos, input.len());
                assert_eq!(word_under_cursor.end, input.len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_middle_word_with_unicode() {
        // Cursor in middle of word with unicode characters
        let input = "ls --sïze café 日本語";
        let cursor_pos = "ls --sïze caf".len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "ls --sïze café 日本語");
                assert_eq!(command_word, "ls");
                assert_eq!(word_under_cursor.s, "café");
                assert_eq!(cursor_byte_pos, "ls --sïze caf".len());
                assert_eq!(word_under_cursor.end, "ls --sïze café".len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_at_start_chinese_chars() {
        // Cursor at start with Chinese characters
        let input = "文件 --option värde";
        let cursor_pos = 0;
        let ctx = get_completion_context(input, cursor_pos);
        match ctx.comp_type {
            CompType::FirstWord(cursor_word) => {
                assert_eq!(cursor_word.s, "文件");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_cursor_in_middle_chinese() {
        // Cursor in middle of Chinese word
        let input = "git 提交 --mëssage 'hëllo'";
        let cursor_pos = "git 提".len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "git 提交 --mëssage 'hëllo'");
                assert_eq!(command_word, "git");
                assert_eq!(word_under_cursor.s, "提交");
                assert_eq!(cursor_byte_pos, "git 提".len());
                assert_eq!(word_under_cursor.end, "git 提交".len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_end_arabic_text() {
        // Cursor at end with Arabic text
        let input = "cat مرحبا --öption 🔥";
        let cursor_pos = input.len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "cat مرحبا --öption 🔥");
                assert_eq!(command_word, "cat");
                assert_eq!(word_under_cursor.s, "🔥");
                assert_eq!(cursor_byte_pos, input.len());
                assert_eq!(word_under_cursor.end, input.len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_cursor_middle_cyrillic() {
        // Cursor in middle of Cyrillic word
        let input = "ls файл --süze привет 🎯";
        let cursor_pos = "ls фай".len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "ls файл --süze привет 🎯");
                assert_eq!(command_word, "ls");
                assert_eq!(word_under_cursor.s, "файл");
                assert_eq!(cursor_byte_pos, "ls фай".len());
                assert_eq!(word_under_cursor.end, "ls файл".len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_blank_space_mixed_scripts() {
        // Cursor on blank space with mixed scripts
        let input = "grep 'pättërn' файл.txt 日本語 🚀";
        let cursor_pos = "grep 'pättërn' ".len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "grep 'pättërn' файл.txt 日本語 🚀");
                assert_eq!(command_word, "grep");
                assert_eq!(word_under_cursor.s, "файл.txt");
                assert_eq!(cursor_byte_pos, "grep 'pättërn' ".len());
                assert_eq!(word_under_cursor.end, "grep 'pättërn' файл.txt".len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_start_emoji_only() {
        // Cursor at start of emoji-only command
        let input = "🎉 🎊 🎈 --flâg";
        let cursor_pos = 0;
        let ctx = get_completion_context(input, cursor_pos);
        match ctx.comp_type {
            CompType::FirstWord(cursor_word) => {
                assert_eq!(cursor_word.s, "🎉");
            }
            _ => panic!("Expected FirstWord"),
        }
    }

    #[test]
    fn test_completion_context_end_accented_characters() {
        // Cursor at end with heavily accented text
        let input = "find . -näme 'fîlé' -type f 🔍";
        let cursor_pos = input.len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "find . -näme 'fîlé' -type f 🔍");
                assert_eq!(command_word, "find");
                assert_eq!(word_under_cursor.s, "🔍");
                assert_eq!(cursor_byte_pos, input.len());
                assert_eq!(word_under_cursor.end, input.len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_space_between_multibyte() {
        // Cursor on space between multibyte characters
        let input = "écho 'mëssagé' 文件 🎨";
        let cursor_pos = "écho 'mëssagé' ".len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "écho 'mëssagé' 文件 🎨");
                assert_eq!(command_word, "écho");
                assert_eq!(word_under_cursor.s, "文件");
                assert_eq!(cursor_byte_pos, "écho 'mëssagé' ".len());
                assert_eq!(word_under_cursor.end, "écho 'mëssagé' 文件".len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }

    #[test]
    fn test_completion_context_middle_thai_text() {
        // Cursor in middle of Thai text
        let input = "cat ไฟล์ --öption วันนี้ 🌟";
        let cursor_pos = "cat ไฟ".len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp {
                full_command,
                command_word,
                word_under_cursor,
                cursor_byte_pos,
            } => {
                assert_eq!(full_command, "cat ไฟล์ --öption วันนี้ 🌟");
                assert_eq!(command_word, "cat");
                assert_eq!(word_under_cursor.s, "ไฟล์");
                assert_eq!(cursor_byte_pos, "cat ไฟ".len());
                assert_eq!(word_under_cursor.end, "cat ไฟล์".len());
            }
            _ => panic!("Expected CommandComp"),
        }
    }
}
