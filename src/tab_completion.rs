use flash::lexer;

#[allow(unused_imports)]
use crate::bash_funcs;

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(dead_code)]
enum CompletionContext {
    FirstWord(
        String, // left part of the word under cursor
    ),
    CommandComp {
        full_command: String,      // e.g. "grep asdf --inv"
        command_word: String,      // e.g. "grep"
        word_under_cursor: String, // e.g. "--inv"
    },
}

#[allow(dead_code)]
fn get_completion_context(buffer: &str, cursor: (usize, usize)) -> Option<CompletionContext> {
    // Not aiming to get this perfect, just a good enough effort
    let cursor_line = cursor.0 + 1;
    let cursor_col = cursor.1 + 1;

    let mut lexer = lexer::Lexer::new(&buffer);
    let mut prev_token: Option<lexer::Token> = None;

    let mut first_word: Option<(lexer::Token, usize)> = None;
    let mut current_word: Option<(lexer::Token, usize)> = None;

    // TODO handle multi byte chars?
    let mut byte_offset_in_buffer = 0;

    loop {
        let token = lexer.next_token();
        if token.kind == lexer::TokenKind::EOF {
            break;
        }
        dbg!(&token);

        while buffer.as_bytes().get(byte_offset_in_buffer) == Some(&b' ') {
            byte_offset_in_buffer += 1;
        }
        dbg!(&buffer[byte_offset_in_buffer..]);

        assert!(buffer[byte_offset_in_buffer..].starts_with(&token.value));
        // dbg!(&token, byte_offset_in_buffer);
        match token.kind {
            lexer::TokenKind::Word(_) => {
                if prev_token
                    .as_ref()
                    .map_or(true, |t| t.kind != lexer::TokenKind::Assignment)
                {
                    current_word = Some((token.clone(), byte_offset_in_buffer));
                    first_word = first_word.or(Some((token.clone(), byte_offset_in_buffer)));
                }
            }
            lexer::TokenKind::Quote
            | lexer::TokenKind::SingleQuote
            | lexer::TokenKind::Backtick
            | lexer::TokenKind::Dollar
            | lexer::TokenKind::LBrace
            | lexer::TokenKind::RBrace
            | lexer::TokenKind::LParen
            | lexer::TokenKind::RParen
            | lexer::TokenKind::CmdSubst
            | lexer::TokenKind::ArithSubst
            | lexer::TokenKind::ArithCommand
            | lexer::TokenKind::ParamExpansion
            | lexer::TokenKind::ParamExpansionOp(_) => {}
            lexer::TokenKind::Assignment => {
                first_word = None;
                current_word = None;
            }
            _ => {
                first_word = None;
                current_word = None;
            }
        }

        byte_offset_in_buffer += token.value.len();

        // peek_next_token updates internal state, DON'T USE IT
        // let next_token = lexer.peek_next_token();

        match token.position.line.cmp(&cursor_line) {
            std::cmp::Ordering::Less => {
                // cursor is after this token
            }
            std::cmp::Ordering::Greater => {
                // cursor is before this token
                break;
            }
            std::cmp::Ordering::Equal => {
                if token.position.column + token.value.len() < cursor_col {
                    // cursor is after this token
                } else if token.position.column >= cursor_col {
                    // cursor is before this token
                    break;
                } else {
                    // cursor is within this token
                    break;
                }
            }
        }

        prev_token = Some(token);
    }

    if let Some((first_word, first_word_start)) = first_word {
        if let Some((current_word, current_word_start)) = current_word {
            dbg!(
                "First word: {:?}, current word: {:?}",
                &first_word,
                &current_word
            );
            dbg!(
                "First word start: {}, current word start: {}",
                first_word_start,
                current_word_start
            );
            if first_word_start == current_word_start {
                return Some(CompletionContext::FirstWord(current_word.value.clone()));
            } else {
                let full_command =
                    &buffer[first_word_start..(current_word_start + current_word.value.len())];
                dbg!("Full command: {:?}", full_command);
                return Some(CompletionContext::CommandComp {
                    full_command: full_command.to_string(),
                    command_word: first_word.value.clone(),
                    word_under_cursor: current_word.value.clone(),
                });
            }
        } else {
            Some(CompletionContext::FirstWord(first_word.value.clone()))
        }
    } else {
        None
    }
}

pub fn tab_complete(_lines: &[String], _cursor: (usize, usize)) -> Option<()> {
    // let word_under_cursor = self.identify_word_under_cursor();
    // log::debug!("Word under cursor: {:?}", word_under_cursor);
    // let (left_part, right_part, is_first_word) = word_under_cursor?;

    // match is_first_word {
    //     true => {
    //         if let Some(completion) = self.tab_complete_first_word(&left_part) {
    //             self.buffer.insert_str(completion);
    //             self.buffer.insert_char(' ');
    //         }
    //     },
    //     false => {
    //         let full_command = self.buffer.lines().join("\n");
    //         let command_word = full_command
    //             .split_whitespace()
    //             .next()
    //             .unwrap_or("");
    //         let word_under_cursor = left_part.clone() + &right_part;

    //         let res = bash_funcs::run_autocomplete_compspec(
    //             &full_command,
    //             command_word,
    //             &word_under_cursor,
    //         );

    //         log::debug!("Compspec completions: {:?}", res);
    //         // if let Some(completion) = res.first() {

    //         //     for _ in 0..left_part.len() {
    //         //         self.buffer.delete_char();
    //         //     }
    //         //     self.buffer.insert_str(completion);
    //         //     self.buffer.insert_char(' ');
    //         // }
    //     }
    // }

    Some(())
}

// fn tab_complete_first_word(&self, left_part: &str) -> Option<String> {

//     if left_part.is_empty() {
//         return None;
//     }

//     let mut res = Vec::new();

//     for poss_completion in self
//         .defined_aliases
//         .iter()
//         .chain(self.defined_reserved_words.iter())
//         .chain(self.defined_shell_functions.iter())
//         .chain(self.defined_builtins.iter())
//         .chain(self.defined_executables.iter().map(|(_, name)| name))
//     {
//         if poss_completion.starts_with(&left_part) {
//             res.push(poss_completion[left_part.len()..].to_string());
//         }
//     }

//     res.sort_by_key(|s| s.len());

//     // If we found any completions, we can use the first one
//     res.first().cloned()

// }

#[cfg(test)]
mod tests {
    use super::*;

    // First word completion tests
    #[test]
    fn test_first_word_simple() {
        let line = "ech".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(Some(CompletionContext::FirstWord("ech".to_string())), res);
    }

    #[test]
    fn test_first_word_with_assignment() {
        let line = "ASDF=1 ech".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(Some(CompletionContext::FirstWord("ech".to_string())), res);
    }

    #[test]
    fn test_first_word_after_semicolon() {
        let line = "grep asdf a.txt; ech".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(Some(CompletionContext::FirstWord("ech".to_string())), res);
    }

    #[test]
    fn test_first_word_after_pipe() {
        let line = "cat file.txt | gre".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(Some(CompletionContext::FirstWord("gre".to_string())), res);
    }

    #[test]
    fn test_first_word_after_and() {
        let line = "make && ./te".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(Some(CompletionContext::FirstWord("./te".to_string())), res);
    }

    // Command completion tests
    #[test]
    fn test_command_arg_simple() {
        let line = "grep --inv".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "grep --inv".to_string(),
                command_word: "grep".to_string(),
                word_under_cursor: "--inv".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_command_arg_with_assignment() {
        let line = "ASDF=1;      grep   asdf --inv".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "grep   asdf --inv".to_string(),
                command_word: "grep".to_string(),
                word_under_cursor: "--inv".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_command_arg_with_quotes() {
        let line = "grep \"sdf$(echo 2)sdf\" --inv".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "grep \"sdf$(echo 2)sdf\" --inv".to_string(),
                command_word: "grep".to_string(),
                word_under_cursor: "--inv".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_command_arg_multiline() {
        let line = "some command\ngit commi mymessage".to_string();
        let cursor = (1, "git commi".len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "git commi".to_string(),
                command_word: "git".to_string(),
                word_under_cursor: "commi".to_string(),
            }),
            res
        );
    }

    // Mid-word cursor position tests
    #[test]
    fn test_cursor_mid_word() {
        let line = "git commi mymessage".to_string();
        let cursor = (0, "git com".len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "git commi".to_string(),
                command_word: "git".to_string(),
                word_under_cursor: "commi".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_cursor_after_space() {
        let line = "git commi mymessage".to_string();
        let cursor = (0, "git ".len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "git commi".to_string(),
                command_word: "git".to_string(),
                word_under_cursor: "commi".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_cursor_end_of_word() {
        let line = "git commi mymessage".to_string();
        let cursor = (0, "git commi".len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "git commi".to_string(),
                command_word: "git".to_string(),
                word_under_cursor: "commi".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_cursor_space_before_next_word() {
        let line = "git commi mymessage".to_string();
        let cursor = (0, "git commi ".len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "git commi mymessage".to_string(),
                command_word: "git".to_string(),
                word_under_cursor: "mymessage".to_string(),
            }),
            res
        );
    }

    // Path completion tests
    #[test]
    fn test_path_argument() {
        let line = "cat ./src/ma".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "cat ./src/ma".to_string(),
                command_word: "cat".to_string(),
                word_under_cursor: "./src/ma".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_absolute_path_argument() {
        let line = "ls /usr/loc".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "ls /usr/loc".to_string(),
                command_word: "ls".to_string(),
                word_under_cursor: "/usr/loc".to_string(),
            }),
            res
        );
    }

    // Edge cases
    #[test]
    fn test_empty_line() {
        let line = "".to_string();
        let cursor = (0, 0);
        let res = get_completion_context(&line, cursor);
        assert_eq!(None, res);
    }

    #[test]
    fn test_only_whitespace() {
        let line = "   ".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(None, res);
    }

    #[test]
    fn test_cursor_at_start() {
        let line = "grep pattern".to_string();
        let cursor = (0, 0);
        let res = get_completion_context(&line, cursor);
        assert_eq!(Some(CompletionContext::FirstWord("grep".to_string())), res);
    }

    #[test]
    fn test_with_cmd_sub() {
        let line = "echo ${VAR}_sdf qwe".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "echo ${VAR}_sdf qwe".to_string(),
                command_word: "echo".to_string(),
                word_under_cursor: "qwe".to_string(),
            }),
            res
        );
    }

    #[test]
    fn test_with_var() {
        let line = "echo $(cat sdf) qwe".to_string();
        let cursor = (0, line.len());
        let res = get_completion_context(&line, cursor);
        assert_eq!(
            Some(CompletionContext::CommandComp {
                full_command: "echo $(cat sdf) qwe".to_string(),
                command_word: "echo".to_string(),
                word_under_cursor: "qwe".to_string(),
            }),
            res
        );
    }
}
