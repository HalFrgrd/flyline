#![allow(dead_code)]


use flash::lexer::{Lexer, Token};
use std::collections::HashMap;

fn line_and_column_to_char_pos(input: &str) -> HashMap<(usize, usize), usize> {
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

pub fn safe_into_tokens_and_char_pos(input: &str) -> Vec<(flash::lexer::Token, usize)> {
    let mut lexer = Lexer::new(input);

    let mut tokens: Vec<(Token, usize)> = Vec::new();

    let line_col_to_char = line_and_column_to_char_pos(input);

    let mut i = 0;
    loop {
        let token = lexer.next_token();
        if token.kind == flash::lexer::TokenKind::EOF {
            break;
        }

        if let Some((prev_token, _)) = tokens.last() {
            // prevent infinite loops on malformed input
            if token.position.line == prev_token.position.line
                && token.position.column == prev_token.position.column
                && token.kind == prev_token.kind
            {
                log::warn!(
                    "Lexer stuck on token at pos {:?}: {:?}",
                    token.position,
                    token.kind
                );
                break;
            }
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

    tokens
}

pub fn safe_into_tokens(input: &str) -> Vec<flash::lexer::Token> {
    safe_into_tokens_and_char_pos(input)
        .into_iter()
        .map(|(token, _)| token)
        .collect()
}
