use std::vec;

use tree_sitter_highlight::HighlightConfiguration;
use tree_sitter_highlight::HighlightEvent;
use tree_sitter_highlight::Highlighter;
use unicode_segmentation::UnicodeSegmentation;

use crate::palette::Palette;
use crate::text_buffer::TextBuffer;
use itertools::Itertools;
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
    pub parts: Vec<FormattedBufferPart>,
    pub cursor_byte_pos: usize,
}

impl FormattedBuffer {
    pub fn split_at_cursor(&self) -> Vec<FormattedBufferPart> {
        let cursor_pos = self.cursor_byte_pos;

        self.parts
            .iter()
            .flat_map(move |part| {
                let mut parts = vec![];

                for (contains_cursor, chunk) in &part
                    .span
                    .content
                    .grapheme_indices(true)
                    .chunk_by(|(idx, _g)| part.start_byte + idx == cursor_pos)
                {
                    let chunk = chunk.collect_vec();

                    let contents = chunk.iter().map(|(_, g)| *g).collect::<String>();
                    let chunk_byte_start =
                        part.start_byte + chunk.first().map(|(idx, _)| *idx).unwrap_or(0);

                    parts.push(FormattedBufferPart {
                        start_byte: chunk_byte_start,
                        span: Span::styled(contents, part.span.style),
                        highlight_name: part.highlight_name.clone(),
                        cursor_info: if contains_cursor { Some(true) } else { None },
                    });
                }

                parts
            })
            .chain(
                // If the cursor is at the end of the buffer, we need to add an extra part for it
                if cursor_pos
                    >= self
                        .parts
                        .last()
                        .map(|p| p.start_byte + p.span.content.len())
                        .unwrap_or(0)
                {
                    vec![FormattedBufferPart {
                        start_byte: cursor_pos,
                        span: Span::from(" ".to_string()),
                        highlight_name: None,
                        cursor_info: Some(false),
                    }]
                } else {
                    vec![]
                },
            )
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct FormattedBufferPart {
    pub start_byte: usize,
    pub span: Span<'static>,
    pub highlight_name: Option<String>,
    pub cursor_info: Option<bool>, // None means no cursor, Some(true) means cursor is on an actual grapheme, Some(false) means cursor is on an artificial position (e.g. end of line)
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
    let spans: Vec<FormattedBufferPart> = highlights
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
        .map(|(start, end, style)| FormattedBufferPart {
            start_byte: start,
            span: Span::styled(source[start..end].to_string(), style),
            highlight_name: None,
            cursor_info: None,
        })
        .collect();

    let cursor_pos = buffer.cursor_byte_pos();

    FormattedBuffer {
        parts: spans,
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
        for span in formatted_buffer.parts {
            eprintln!("{:?}", span);
        }

        assert!(false);
    }
}
