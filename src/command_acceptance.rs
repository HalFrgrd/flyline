use tree_sitter::{Node, Parser};
use tree_sitter_bash;

pub fn will_bash_accept_buffer(buffer: &str) -> bool {
    // returns true iff bash won't try to get more input to complete the command
    // e.g. unclosed quotes, unclosed parens/braces/brackets, etc.
    // its ok if there are syntax errors, as long as the command is "complete"
    
    // Handle empty input
    if buffer.trim().is_empty() {
        return true;
    }
    
    // Handle line continuations
    if buffer.trim_end().ends_with('\\') {
        return false;
    }
    
    // Quick text-based checks for common incomplete patterns
    if has_obvious_incomplete_patterns(buffer) {
        return false;
    }
    
    let mut parser = Parser::new();
    let language = tree_sitter_bash::LANGUAGE.into();
    parser.set_language(&language).unwrap();
    
    let tree = parser.parse(buffer, None).unwrap();
    let root = tree.root_node();

    // Use tree-sitter's missing node detection
    !has_missing_nodes(&root)
}

fn has_obvious_incomplete_patterns(buffer: &str) -> bool {
    let trimmed = buffer.trim_end();
    
    // Check for trailing operators
    if trimmed.ends_with('|') && !trimmed.ends_with("||") {
        return true;
    }
    if trimmed.ends_with("||") || trimmed.ends_with("&&") {
        return true;
    }
    
    // Remove comments before checking quotes (comments can contain unmatched quotes)
    let buffer_without_comments = remove_comments(buffer);
    
    // Simple quote counting (not perfect but catches most cases)
    let single_quotes = buffer_without_comments.chars().filter(|&c| c == '\'').count();
    if single_quotes % 2 == 1 {
        return true;
    }
    
    // Count unescaped double quotes
    if count_unescaped_quotes(&buffer_without_comments) % 2 == 1 {
        return true;
    }
    
    // Check for common unclosed structures
    let open_parens = buffer.chars().filter(|&c| c == '(').count();
    let close_parens = buffer.chars().filter(|&c| c == ')').count();
    if open_parens > close_parens {
        return true;
    }
    
    let open_braces = buffer.chars().filter(|&c| c == '{').count();
    let close_braces = buffer.chars().filter(|&c| c == '}').count();
    if open_braces > close_braces {
        return true;
    }
    
    let open_brackets = buffer.chars().filter(|&c| c == '[').count();
    let close_brackets = buffer.chars().filter(|&c| c == ']').count();
    if open_brackets > close_brackets {
        return true;
    }
    
    // Check for incomplete control structures
    if has_incomplete_control_structure(buffer) {
        return true;
    }
    
    // Check for incomplete heredocs
    if has_incomplete_heredoc(buffer) {
        return true;
    }
    
    false
}

fn remove_comments(buffer: &str) -> String {
    // Simple approach: remove everything after # that's not inside quotes
    let lines: Vec<&str> = buffer.lines().collect();
    let mut result_lines = Vec::new();
    
    for line in lines {
        let mut result_line = String::new();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut escaped = false;
        
        for ch in line.chars() {
            if escaped {
                result_line.push(ch);
                escaped = false;
                continue;
            }
            
            if ch == '\\' {
                result_line.push(ch);
                escaped = true;
                continue;
            }
            
            if !in_single_quote && !in_double_quote && ch == '#' {
                // Start of comment - ignore rest of line
                break;
            }
            
            if ch == '\'' && !in_double_quote {
                in_single_quote = !in_single_quote;
            } else if ch == '"' && !in_single_quote {
                in_double_quote = !in_double_quote;
            }
            
            result_line.push(ch);
        }
        
        result_lines.push(result_line);
    }
    
    result_lines.join("\n")
}

fn has_incomplete_control_structure(buffer: &str) -> bool {
    let trimmed = buffer.trim();
    
    // Count control structure keywords
    let if_count = count_word_occurrences(trimmed, "if");
    let fi_count = count_word_occurrences(trimmed, "fi");
    if if_count > fi_count {
        return true;
    }
    
    let do_count = count_word_occurrences(trimmed, "do");
    let done_count = count_word_occurrences(trimmed, "done");
    if do_count > done_count {
        return true;
    }
    
    let case_count = count_word_occurrences(trimmed, "case");
    let esac_count = count_word_occurrences(trimmed, "esac");
    if case_count > esac_count {
        return true;
    }
    
    false
}

fn count_word_occurrences(text: &str, word: &str) -> usize {
    // Simple word boundary detection
    let mut count = 0;
    let mut chars = text.char_indices().peekable();
    
    while let Some((i, _)) = chars.next() {
        if text[i..].starts_with(word) {
            // Check if this is a word boundary
            let is_start_boundary = i == 0 || !text.chars().nth(i.saturating_sub(1)).unwrap_or(' ').is_alphanumeric();
            let end_pos = i + word.len();
            let is_end_boundary = end_pos >= text.len() || !text.chars().nth(end_pos).unwrap_or(' ').is_alphanumeric();
            
            if is_start_boundary && is_end_boundary {
                count += 1;
                // Skip ahead to avoid overlapping matches
                for _ in 0..word.len() {
                    chars.next();
                }
            }
        }
    }
    count
}

fn has_incomplete_heredoc(buffer: &str) -> bool {
    // Look for all heredoc patterns and check if they're properly terminated
    let mut pos = 0;
    
    while let Some(heredoc_start) = buffer[pos..].find("<<") {
        let absolute_start = pos + heredoc_start;
        let after_heredoc = &buffer[absolute_start + 2..];
        
        if let Some(first_newline) = after_heredoc.find('\n') {
            let delimiter_line = after_heredoc[..first_newline].trim();
            if !delimiter_line.is_empty() {
                // Extract just the first delimiter from the line (handle multiple heredocs)
                let delimiter = if let Some(space_or_redirect) = delimiter_line.find(|c: char| c.is_whitespace() || c == '<') {
                    delimiter_line[..space_or_redirect].trim_start_matches('-').trim()
                } else {
                    delimiter_line.trim_start_matches('-').trim()
                };
                
                if !delimiter.is_empty() {
                    // Look for the terminator after the first newline
                    let content_start = absolute_start + 2 + first_newline + 1;
                    let remaining_content = &buffer[content_start..];
                    
                    if !remaining_content.contains(&format!("\n{}", delimiter)) 
                        && !remaining_content.starts_with(delimiter) 
                        && !remaining_content.starts_with(&format!("{}\n", delimiter)) {
                        return true;
                    }
                }
            }
        } else {
            // No newline after heredoc operator
            return true;
        }
        
        pos = absolute_start + 2;
    }
    
    false
}

fn has_missing_nodes(node: &Node) -> bool {
    if node.is_missing() {
        return true;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_missing_nodes(&child) {
            return true;
        }
    }

    false
}

fn count_unescaped_quotes(s: &str) -> usize {
    let mut count = 0;
    let mut escaped = false;
    
    for ch in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        
        if ch == '\\' {
            escaped = true;
            continue;
        }
        
        if ch == '"' {
            count += 1;
        }
    }
    
    count
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
