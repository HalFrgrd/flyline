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
pub struct FormattedBufferSpan<'a> {
    pub start_byte: usize,
    pub end_byte: usize,
    pub span: Span<'a>,
    pub highlight_name: Option<String>,
}

pub fn format_buffer<'a>(buffer: &'a TextBuffer) -> Vec<FormattedBufferSpan<'a>> {
    let mut formatted_spans = vec![];

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

    highlights.fold(None, |current_style, event| {
        match event.unwrap() {
            HighlightEvent::Source { start, end } => {
                // eprintln!("source: {start}-{end}: {:?}", &source[start..end]);
                let source = &source[start..end];

                let style = match &current_style {
                    Some("command") => Palette::recognised_word(),
                    _ => Palette::normal_text(),
                };

                formatted_spans.push(FormattedBufferSpan {
                    start_byte: start,
                    end_byte: end,
                    span: Span::styled(source, style),
                    highlight_name: current_style.map(|s| s.to_string()),
                });
                current_style
            }
            HighlightEvent::HighlightStart(s) => {
                let highlight_name = HIGHLIGHT_NAMES.get(s.0).unwrap_or(&"unknown");
                // eprintln!("highlight style started: {highlight_name}");
                Some(*highlight_name)
            }
            HighlightEvent::HighlightEnd => {
                // eprintln!("highlight style ended");
                None
            }
        }
    });

    formatted_spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn bash_highlight_example() {
        let buf = TextBuffer::new("for       f in *.rs; do\necho \"$f\";\ndone");

        let formatted_buffer = format_buffer(&buf);
        for span in formatted_buffer {
            eprintln!("{:?}", span);
        }

        assert!(false);
    }
}
