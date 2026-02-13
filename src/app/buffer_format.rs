use std::vec;

use crossterm::event;
use tree_sitter_highlight::HighlightConfiguration;
use tree_sitter_highlight::HighlightEvent;
use tree_sitter_highlight::Highlighter;

use crate::palette::Palette;
use crate::text_buffer::TextBuffer;
use ratatui::prelude::*;

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "function",
    "function.builtin",
    "keyword",
    "module",
    "number",
    "operator",
    "property",
    "property.builtin",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

#[derive(Debug)]
pub struct FormattedBuffer {
    pub spans: Vec<FormattedBufferSpan>,
    pub cursor_byte_pos: usize,
}

#[derive(Debug)]
pub struct FormattedBufferSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub span: Span<'static>,
    pub highlight_name: Option<String>,
    pub artifically_inserted_for_char: bool,
}

// TODO: second layer of formatting for animations
// it should go over the formmatted spans and modify them
// e.g. cursor animation, python animation

pub fn format_buffer(buffer: &TextBuffer) -> FormattedBuffer {
    let mut highlighter = Highlighter::new();

    let bash_language = tree_sitter_bash::LANGUAGE.into();

    let mut bash_config = HighlightConfiguration::new(
        bash_language,
        "bash",
        tree_sitter_bash::HIGHLIGHT_QUERY,
        "",
        "",
    )
    .unwrap();

    bash_config.configure(&HIGHLIGHT_NAMES);

    let source = buffer.buffer();

    let highlights = highlighter
        .highlight(&bash_config, source.as_bytes(), None, |_| None)
        .unwrap();

    let mut last_style: Option<&str> = None;
    let mut spans: Vec<FormattedBufferSpan> = highlights
        .into_iter()
        .filter_map(|event| match event {
            Ok(HighlightEvent::HighlightStart(s)) => {
                let highlight_name = HIGHLIGHT_NAMES.get(s.0).unwrap_or(&"unknown");
                last_style = Some(*highlight_name);
                None
            }
            Ok(HighlightEvent::HighlightEnd) => {
                last_style = None;
                None
            }
            Ok(HighlightEvent::Source { start, end }) => {
                let style = match last_style {
                    Some("command") => Palette::recognised_word(),
                    _ => Palette::normal_text(),
                };
                // Sometimes a new line will be in the middle of a span, so we need to split it into multiple spans
                let mut lines = vec![];
                let mut span_start = start;
                for (char_idx, c) in source[start..end].char_indices() {
                    let global_char_idx = start + char_idx;
                    if c == '\n' {
                        if span_start < global_char_idx {
                            lines.push((span_start, global_char_idx, style));
                        }
                        lines.push((global_char_idx, global_char_idx + 1, style));
                        span_start = global_char_idx + 1;
                    }
                }
                if span_start < end {
                    lines.push((span_start, end, style));
                }

                Some(lines)
            }
            Err(_) => None,
        })
        .flatten()
        .inspect(|x| {
            if cfg!(test) {
                let text = &source[x.0..x.1];
                let text_to_print = if text == "\n" { "\\n" } else { text };

                println!("{:?} {}", x, &text_to_print);
            }
        })
        .map(|(start, end, style)| FormattedBufferSpan {
            start_byte: start,
            end_byte: end,
            span: Span::styled(source[start..end].to_string(), style),
            highlight_name: None,
            artifically_inserted_for_char: false,
        })
        .collect();

    let cursor_pos = buffer.cursor_byte_pos();
    if spans.last().map(|s| s.end_byte) <= Some(cursor_pos) {
        spans.push(FormattedBufferSpan {
            start_byte: spans.last().map(|s| s.end_byte).unwrap_or(0),
            end_byte: cursor_pos + 1,
            span: Span::styled(" ", Palette::normal_text()),
            highlight_name: None,
            artifically_inserted_for_char: true,
        });
    }

    FormattedBuffer {
        spans,
        cursor_byte_pos: cursor_pos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn bash_highlight_example() {
        let buf = TextBuffer::new("       for       f in *.rs; do\necho '$f';\n\n;done");

        let formatted_buffer = format_buffer(&buf);
        for span in formatted_buffer.spans {
            eprintln!("{:?}", span);
        }

        assert!(false);
    }
}
