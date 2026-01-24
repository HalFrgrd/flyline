use tree_sitter::{Node, Parser};
use tree_sitter_bash;

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

trait ToInclusiveRange {
    fn to_inclusive(&self) -> core::ops::RangeInclusive<usize>;
}

impl ToInclusiveRange for std::ops::Range<usize> {
    fn to_inclusive(&self) -> core::ops::RangeInclusive<usize> {
        self.start..=self.end
    }
}

trait IsSubRange {
    fn is_sub_range(&self, other: &core::ops::Range<usize>) -> bool;
}

impl IsSubRange for core::ops::Range<usize> {
    fn is_sub_range(&self, other: &core::ops::Range<usize>) -> bool {
        self.start >= other.start && self.end <= other.end
    }
}

impl<'a> CompletionContext<'a> {
    fn classify_word_type(word: &str) -> Option<CompType> {
        if word.starts_with('$') {
            Some(CompType::EnvVariable)
        } else if word.starts_with('~') && !word.contains("/") {
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

// Very useful
// https://tree-sitter.github.io/tree-sitter/7-playground.html

pub fn get_completion_context<'a>(
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
    let word_under_cursor = cursor_node
        .utf8_text(buffer.as_bytes())
        .unwrap_or(buffer[0..0].as_ref());

    if cfg!(test) {
        dbg!(&cursor_byte_pos);
        dbg!(&cursor_node);
    }

    // Find the command context for this cursor position
    let comp_context_node = find_comp_context_from_cursor(&cursor_node);
    let comp_context_range = trim_node(&comp_context_node, cursor_byte_pos);
    assert!(comp_context_range.is_sub_range(&comp_context_node.byte_range()));

    let command = &buffer[comp_context_range.clone()];
    let command_until_cursor =
        &buffer[comp_context_range.start..cursor_byte_pos.min(comp_context_range.end)];

    CompletionContext::new(buffer, command_until_cursor, command, word_under_cursor)
}

fn find_deepest_node_at_position<'a>(node: &Node<'a>, cursor_byte_pos: usize) -> Node<'a> {
    // Check children to find the deepest node containing the cursor
    for child in node.children(&mut node.walk()) {
        if !child.is_named() {
            // This prevents matching on punctuation nodes like ; or & or ))
            continue;
        }
        if child.byte_range().to_inclusive().contains(&cursor_byte_pos) {
            return find_deepest_node_at_position(&child, cursor_byte_pos);
        }
    }

    // No child contains the cursor, so this is the deepest node
    *node
}

fn find_comp_context_from_cursor<'tree>(cursor_node: &Node<'tree>) -> Node<'tree> {
    let mut current_node = *cursor_node;

    // Traverse up the tree to find the appropriate command context
    loop {
        if cfg!(test) {
            dbg!(&current_node);
            dbg!(&current_node.parent());
        }

        match current_node.kind() {
            "test_command" => {
                return current_node;
            }
            _ => {}
        }

        let parent = match current_node.parent() {
            Some(p) => p,
            None => {
                return current_node;
            }
        };

        match parent.kind() {
            "command" => {
                return parent;
            }

            "program"
            | "pipeline"
            | "command_substitution"
            | "test_command"
            | "arithmetic_expansion"
            | "expansion"
            | "process_substitution" => {
                return current_node;
            }

            _ => {
                current_node = parent;
            }
        }
    }
}

fn trim_node(node: &Node, cursor_byte_pos: usize) -> core::ops::Range<usize> {
    if node.kind() != "command" {
        return node.byte_range();
    }

    let mut start = node.start_byte();

    // Determine the start of the "real" command by skipping leading variable assignments.
    for child in node.children(&mut node.walk()) {
        if child.kind() == "variable_assignment" {
            if child.byte_range().to_inclusive().contains(&cursor_byte_pos) {
                // cursor is inside the assignment, so return the assignment as the command
                return child.byte_range();
            } else if child.end_byte() < cursor_byte_pos {
                // skip leading assignments
                start = child.end_byte();
                if cfg!(test) {
                    println!("Skipping leading assignment, new start: {}", start);
                }
            }
        } else {
            break;
        }
    }

    if cfg!(test) {
        dbg!(&node);
        dbg!(&start);
        println!("first child for byte {}", start);
        dbg!(node.first_child_for_byte(start));
    }

    match node.first_child_for_byte(start) {
        Some(child) => {
            if cfg!(test) {
                println!("Found first child for byte {}", start);
                dbg!(&child);
            }

            if (child.start_byte()..=node.end_byte()).contains(&cursor_byte_pos) {
                if cfg!(test) {
                    println!(
                        "Cursor byte pos {} is within command bounds {}-{}",
                        cursor_byte_pos,
                        child.start_byte(),
                        node.end_byte()
                    );
                }
                child.start_byte()..node.end_byte()
            } else {
                if cfg!(test) {
                    println!(
                        "Cursor byte pos {} is NOT within command bounds {}-{}, returning cursor pos as both start and end",
                        cursor_byte_pos,
                        child.start_byte(),
                        node.end_byte()
                    );
                }
                cursor_byte_pos..cursor_byte_pos
            }
        }
        _ => {
            if cfg!(test) {
                println!("No child found for byte {}", start);
            }

            start..start
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
    fn test_with_assignment_basic() {
        let input = "A=b ls -la";
        let cursor_pos = "A=b ".len();
        let res = run(input, cursor_pos);
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
        let input = r#"VAR=valué ABC=qwe         ls -la"#;
        let cursor_pos = "VAR=valué ABC=qwe   ".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "");
        assert_eq!(res.context_until_cursor, "");
    }

    #[test]
    fn test_with_assignment_at_assignment() {
        let input = r#"VAR=valué ABC=qwe ls -la"#;
        let cursor_pos = "VAR=valué ABC=qwe".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "ABC=qwe");
        assert_eq!(res.context_until_cursor, "ABC=qwe");
    }

    #[test]
    fn test_list_of_commands() {
        let input = r#"git commit -m "Initial 🚀"; ls -la"#;
        let res = run(input, input.len());
        assert_eq!(res.context, "ls -la");
        assert_eq!(res.context_until_cursor, "ls -la");
    }

    #[test]
    fn test_with_pipeline() {
        let input = r#"cat filé.txt | grep "pattern" | sort"#;
        let res = run(input, input.len());
        assert_eq!(res.context, "sort");
        assert_eq!(res.context_until_cursor, "sort");

        let input2 = r#"echo "héllo" && echo "wörld""#;
        let res2 = run(input2, input2.len());
        assert_eq!(res2.context, r#"echo "wörld""#);
        assert_eq!(res2.context_until_cursor, r#"echo "wörld""#);

        let input3 = r#"false || echo "fallback 😅""#;
        let res3 = run(input3, input3.len());
        assert_eq!(res3.context, r#"echo "fallback 😅""#);
        assert_eq!(res3.context_until_cursor, r#"echo "fallback 😅""#);
    }

    #[test]
    fn test_subshell_in_command() {
        let input = "echo $(git rev-parse HEAD) résumé";
        let res = run(input, input.len());
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
        let input = r#"echo $(git rev-parse HEAD) café"#;
        let cursor_pos = "echo $(git rev-parse".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor, r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_subshell_command() {
        let input = r#"echo $(git rev-parse HEAD) 🎉"#;
        let cursor_pos = "echo $(git rev-parse HEAD".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor, r#"git rev-parse HEAD"#);
    }

    #[test]
    fn test_command_at_end_of_subshell() {
        let input = r#"echo $(ls -la)"#;
        let res = run(input, input.len());
        assert_eq!(res.context, "echo $(ls -la)");
        assert_eq!(res.context_until_cursor, "echo $(ls -la)");
    }

    #[test]
    fn test_param_expansion_in_command() {
        let input = r#"echo ${HOME} naïve"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"echo ${HOME} naïve"#);
        assert_eq!(res.context_until_cursor, r#"echo ${HOME} naïve"#);
    }

    #[test]
    fn test_cursor_in_middle_of_param_expansion() {
        let input = r#"echo ${HOME} asdf"#;
        let cursor_pos = "echo ${HO".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"HOME"#);
        assert_eq!(res.context_until_cursor, "HO");
    }

    #[test]
    fn test_cursor_at_end_of_param_expansion() {
        let input = r#"echo ${HOME} asdf"#;
        let cursor_pos = "echo ${HOME}".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"echo ${HOME} asdf"#);
        assert_eq!(res.context_until_cursor, r#"echo ${HOME}"#);
    }

    #[test]
    fn test_command_at_end_of_param_expansion() {
        let input = r#"ls -la ${PWD}"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"ls -la ${PWD}"#);
        assert_eq!(res.context_until_cursor, r#"ls -la ${PWD}"#);
    }

    #[test]
    fn test_complex_param_expansion() {
        let input = r#"echo ${VAR:-dëfault} test 🎯"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"echo ${VAR:-dëfault} test 🎯"#);
        assert_eq!(res.context_until_cursor, r#"echo ${VAR:-dëfault} test 🎯"#);
    }

    #[test]
    fn test_cursor_inside_complex_param_expansion() {
        let input = r#"echo ${VAR:-dëfault} tëst"#;
        let cursor_pos = "echo ${VAR:-dëf".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "dëfault");
        assert_eq!(res.context_until_cursor, "dëf");
    }

    #[test]
    fn test_backtick_substitution_in_command() {
        let input = r#"echo `git rev-parse HEAD` café"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"echo `git rev-parse HEAD` café"#);
        assert_eq!(
            res.context_until_cursor,
            r#"echo `git rev-parse HEAD` café"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_backtick_command() {
        let input = r#"echo `git rev-parse HEAD` asdf"#;
        let cursor_pos = "echo `git rev-parse".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"git rev-parse HEAD"#);
        assert_eq!(res.context_until_cursor, r#"git rev-parse"#);
    }

    #[test]
    fn test_cursor_at_end_of_backtick_command() {
        let input = r#"a `b c`"#;
        let cursor_pos = "a `b c".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "b c");
        assert_eq!(res.context_until_cursor, "b c");
    }

    #[test]
    fn test_command_at_end_of_backtick() {
        let input = r#"echo `ls -la` qwe"#;
        let cursor_pos = "echo `ls -la`".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "echo `ls -la` qwe");
        assert_eq!(res.context_until_cursor, "echo `ls -la`");
    }

    #[test]
    fn test_nested_backticks_in_command() {
        let input = r#"echo `echo \`date\`` tëst 🎯"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"echo `echo \`date\`` tëst 🎯"#);
        assert_eq!(res.context_until_cursor, r#"echo `echo \`date\`` tëst 🎯"#);
    }

    #[test]
    fn test_cursor_in_backtick_with_pipe() {
        let input = r#"echo `ls | grep test` done"#;
        let cursor_pos = "echo `ls | grep".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"grep test"#);
        assert_eq!(res.context_until_cursor, r#"grep"#);
    }

    #[test]
    fn test_arith_subst_in_command() {
        let input = r#"echo $((5 + 3)) rësult 📊"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"echo $((5 + 3)) rësult 📊"#);
        assert_eq!(res.context_until_cursor, r#"echo $((5 + 3)) rësult 📊"#);
    }

    #[test]
    fn test_cursor_in_middle_of_arith_subst() {
        let input = r#"echo $((5 + 3)) result"#;
        let cursor_pos = "echo $((5 + 3".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "5 + 3");
        assert_eq!(res.context_until_cursor, "5 + 3");
    }

    #[test]
    fn test_cursor_in_middle_of_arith_subst_2() {
        let input = r#"echo $((5 + 3)) result"#;
        let cursor_pos = "echo $((5 + 3)".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "echo $((5 + 3)) result");
        assert_eq!(res.context_until_cursor, "echo $((5 + 3)");
    }

    #[test]
    fn test_cursor_at_end_of_arith_subst() {
        let input = r#"echo $((10 * 2)) done"#;
        let cursor_pos = "echo $((10 * 2))".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"echo $((10 * 2)) done"#);
        assert_eq!(res.context_until_cursor, r#"echo $((10 * 2))"#);
    }

    #[test]
    fn test_command_at_end_of_arith_subst() {
        let input = r#"result=$((100 / 5))"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"result=$((100 / 5))"#);
        assert_eq!(res.context_until_cursor, r#"result=$((100 / 5))"#);
    }

    #[test]
    fn test_complex_arith_with_variables() {
        let input = r#"echo $(($VAR + 10)) test"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"echo $(($VAR + 10)) test"#);
        assert_eq!(res.context_until_cursor, r#"echo $(($VAR + 10)) test"#);
    }

    #[test]
    fn test_cursor_inside_complex_arith() {
        let input = r#"val=$((VAR * 2 + 5))"#;
        let cursor_pos = "val=$((VAR * 2".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "VAR * 2 + 5");
        assert_eq!(res.context_until_cursor, "VAR * 2");
    }

    #[test]
    fn test_nested_arith_operations() {
        let input = r#"echo $(( $(( 5 + 3 )) * 2 )) ënd ✅"#;
        let res = run(input, "echo $(( $(( 5 +".len());
        assert_eq!(res.context, r#"5 + 3"#);
        assert_eq!(res.context_until_cursor, r#"5 +"#);
    }

    #[test]
    fn test_proc_subst_in_command() {
        let input = r#"diff <(ls /tmp) <(ls /var) résult 🔍"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"diff <(ls /tmp) <(ls /var) résult 🔍"#);
        assert_eq!(
            res.context_until_cursor,
            r#"diff <(ls /tmp) <(ls /var) résult 🔍"#
        );
    }

    #[test]
    fn test_cursor_in_middle_of_proc_subst_in() {
        let input = r#"diff <(ls /tmp) <(ls /var) done"#;
        let cursor_pos = "diff <(ls /t".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"ls /tmp"#);
        assert_eq!(res.context_until_cursor, r#"ls /t"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_in() {
        let input = r#"diff <(ls /tmp) <(ls /var) done"#;
        let cursor_pos = "diff <(ls /tmp".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"ls /tmp"#);
        assert_eq!(res.context_until_cursor, r#"ls /tmp"#);
    }

    #[test]
    fn test_command_at_end_of_proc_subst_in() {
        let input = r#"cat <(echo test)"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"cat <(echo test)"#);
        assert_eq!(res.context_until_cursor, r#"cat <(echo test)"#);
    }

    #[test]
    fn test_proc_subst_out_in_command() {
        let input = r#"tee >(gzip > filé.gz) >(bzip2 > filé.bz2) 🎉"#;
        let res = run(input, input.len());
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
        let input = r#"tee >(gzip > file.gz) test"#;
        let cursor_pos = "tee >(gzip > fi".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"gzip > file.gz"#);
        assert_eq!(res.context_until_cursor, r#"gzip > fi"#);
    }

    #[test]
    fn test_cursor_at_end_of_proc_subst_out() {
        let input = r#"tee >(cat) done"#;
        let cursor_pos = "tee >(cat".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, r#"cat"#);
        assert_eq!(res.context_until_cursor, r#"cat"#);
    }

    #[test]
    fn test_mixed_proc_subst_in_and_out() {
        let input = r#"cmd <(input cmd) >(output cmd) final"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"cmd <(input cmd) >(output cmd) final"#);
        assert_eq!(
            res.context_until_cursor,
            r#"cmd <(input cmd) >(output cmd) final"#
        );
    }

    #[test]
    #[ignore] // Need to think more on what the expected behavior is here
    fn test_double_bracket_condition() {
        let input = r#"if [[ -f file.txt ]]; then echo found; fi"#;
        let res = run(input, input.len());
        assert_eq!(res.context, "fi");
        assert_eq!(res.context_until_cursor, "fi");
    }

    #[test]
    fn test_cursor_inside_double_bracket() {
        let input = r#"[[ -f filé.txt ]] && echo yës"#;
        let cursor_pos = "[[ -f filé".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "-f filé.txt");
        assert_eq!(res.context_until_cursor, "-f filé");
    }

    #[test]
    fn test_double_bracket_with_string_comparison() {
        let input = r#"[[ "$var" == "café" ]] && echo match 🎯"#;
        let res = run(input, input.len());
        assert_eq!(res.context, r#"echo match 🎯"#);
        assert_eq!(res.context_until_cursor, r#"echo match 🎯"#);
    }

    #[test]
    fn test_double_bracket_with_pattern() {
        let input = r#"[[ $file == *.txt ]] || echo "not a text file""#;
        let cursor_pos = "[[ $file == *.txt ]".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "[[ $file == *.txt ]]");
        assert_eq!(res.context_until_cursor, "[[ $file == *.txt ]");
    }

    #[test]
    fn test_double_bracket_with_regex() {
        let input = r#"[[ $email =~ ^[a-z]+@[a-z]+$ ]]"#;
        let res = run(input, input.len());
        assert_eq!(res.context, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
        assert_eq!(res.context_until_cursor, "[[ $email =~ ^[a-z]+@[a-z]+$ ]]");
    }

    #[test]
    fn test_double_bracket_logical_operators() {
        let input = r#"[[ -f file.txt && -r file.txt ]] && cat file.txt"#;
        let res = run(input, input.len());
        assert_eq!(res.context, "cat file.txt");
        assert_eq!(res.context_until_cursor, "cat file.txt");
    }

    #[test]
    fn test_cursor_before_double_bracket() {
        let input = r#"if [[ -d /path/café ]]; then ls; fi"#;
        let cursor_pos = "if [[ -d /path/caf".len();
        let res = run(input, cursor_pos);
        assert_eq!(res.context, "-d /path/café");
        assert_eq!(res.context_until_cursor, "-d /path/caf");
    }

    #[test]
    fn test_double_bracket_with_emoji() {
        let input = r#"[[ "$msg" == "✅ done" ]] && echo success"#;
        let res = run(input, input.len());
        assert_eq!(res.context, "echo success");
        assert_eq!(res.context_until_cursor, "echo success");
    }

    // Tests for CompletionContext with various cursor positions and non-ASCII characters

    #[test]
    fn test_completion_context_cursor_at_start_of_line() {
        // Cursor at position 0 (start of line)
        let input = "café --option 🎯";
        let ctx = get_completion_context(input, 0);
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
        let input = "café --option 🎯";
        let cursor_pos = "caf".len();
        let ctx = get_completion_context(input, cursor_pos);
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
        let input = "🚀rocket --verbose naïve";
        let cursor_pos = "🚀rock".len();
        let ctx = get_completion_context(input, cursor_pos);
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
        let input = "echo 'Tëst message' résumé 📄";
        let cursor_pos = input.len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "ls --sïze café 日本語";
        let cursor_pos = "ls --sïze caf".len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "文件 --option värde";
        let cursor_pos = 0;
        let ctx = get_completion_context(input, cursor_pos);
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
        let input = "git 提交 --mëssage 'hëllo'";
        let cursor_pos = "git 提".len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "cat مرحبا --öption 🔥";
        let cursor_pos = input.len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "ls файл --süze привет 🎯";
        let cursor_pos = "ls фай".len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "grep 'pättërn' файл.txt 日本語 🚀";
        let cursor_pos = "grep 'pättërn' ".len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "🎉 🎊 🎈 --flâg";
        let cursor_pos = 0;
        let ctx = get_completion_context(input, cursor_pos);
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
        let input = "find . -näme 'fîlé' -type f 🔍";
        let cursor_pos = input.len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "écho 'mëssagé' 文件 🎨";
        let cursor_pos = "écho 'mëssagé' ".len();
        let ctx = get_completion_context(input, cursor_pos);

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
        let input = "cat ไฟล์ --öption วันนี้ 🌟";
        let cursor_pos = "cat ไฟ".len();
        let ctx = get_completion_context(input, cursor_pos);

        match ctx.comp_type {
            CompType::CommandComp { command_word } => {
                assert_eq!(ctx.context, "cat ไฟล์ --öption วันนี้ 🌟");
                assert_eq!(command_word, "cat");
                assert_eq!(ctx.word_under_cursor, "ไฟล์");
            }
            _ => panic!("Expected CommandComp"),
        }
    }
}
