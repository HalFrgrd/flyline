use std::borrow::Cow;
use std::vec;

use tree_sitter_highlight::HighlightConfiguration;
use tree_sitter_highlight::HighlightEvent;
use tree_sitter_highlight::Highlighter;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::palette::Palette;
use crate::text_buffer::TextBuffer;
use itertools::{EitherOrBoth, Itertools};
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
    byte_length: usize,
}

impl FormattedBuffer {
    pub fn split_at_cursor_from<'a>(
        &'a self,
    ) -> impl Iterator<Item = Cow<'a, FormattedBufferPart>> {
        let cursor_pos = self.cursor_byte_pos;

        self.parts
            .iter()
            .flat_map(move |part| {
                let part_start = part.start_byte;
                let part_end = part.start_byte + part.span.content.len();

                let mut parts = vec![];
                let split = if cursor_pos >= part_start && cursor_pos < part_end {
                    part.span
                        .content
                        .grapheme_indices(true)
                        .enumerate()
                        .find_map(|(grapheme_idx, (byte_idx, _))| {
                            if part_start + byte_idx == cursor_pos {
                                Some((byte_idx, grapheme_idx))
                            } else {
                                None
                            }
                        })
                } else {
                    None
                };

                if let Some((split_byte, split_grapheme_idx)) = split {
                    let (left, right) = part.split_at_cursor(split_byte, split_grapheme_idx);

                    if let Some(left) = left {
                        parts.push(Cow::Owned(left));
                    }

                    if let Some(right) = right {
                        parts.push(Cow::Owned(right));
                    }
                } else {
                    parts.push(Cow::Borrowed(part));
                }

                parts
            })
            .chain(
                // If the cursor is at the end of the buffer, we need to add an extra part for it
                if cursor_pos >= self.byte_length {
                    Some(Cow::Owned(FormattedBufferPart {
                        start_byte: cursor_pos,
                        span: Span::from(" ".to_string()),
                        alternative_span: None,
                        highlight_name: None,
                        cursor_info: Some(false),
                        tooltip: None,
                    }))
                } else {
                    None
                }
                .into_iter(),
            )
    }

    pub fn get_part_from_byte_pos(&self, byte_pos: usize) -> Option<&FormattedBufferPart> {
        self.parts.iter().find(|part| {
            let part_start = part.start_byte;
            let part_end = part.start_byte + part.span.content.len();
            byte_pos >= part_start && byte_pos < part_end
        })
    }
}

impl Default for FormattedBuffer {
    fn default() -> Self {
        FormattedBuffer {
            parts: vec![],
            cursor_byte_pos: 0,
            byte_length: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormattedBufferPart {
    pub start_byte: usize,
    span: Span<'static>,
    /// Meant for animations. Should have the same grapheme widths as span,
    /// but can have different content and style. If present, it will be used
    /// instead of span for display, but span will still be used for cursor
    /// positioning and other logic.
    alternative_span: Option<Span<'static>>,
    pub highlight_name: Option<String>,
    /// None means no cursor,
    /// Some(true) means cursor is on an actual grapheme, (and we should draw the contents with the cursor style)
    /// Some(false) means cursor is on an artificial position (e.g. end of line)
    pub cursor_info: Option<bool>,
    pub tooltip: Option<String>,
}

fn name_to_style(name: Option<&'static str>, recognised_command: Option<bool>) -> Style {
    match name {
        Some("command") if recognised_command.unwrap_or(false) => Palette::recognised_word(),
        Some("function") if recognised_command.unwrap_or(false) => Palette::recognised_word(),
        Some("command") => Palette::unrecognised_word(),
        Some("function") => Palette::unrecognised_word(),
        Some("string") => Palette::unrecognised_word(),
        _ => Palette::normal_text(),
    }
}

type TooltipFn<'a> = Box<dyn FnMut(&str) -> Option<String> + 'a>;
type RecognisedCommandFn<'a> = Box<dyn FnMut(&str) -> bool + 'a>;

impl FormattedBufferPart {
    pub fn new(
        start_byte: usize,
        content: &str,
        highlight_name: Option<&'static str>,
        tooltip_fn: &mut Option<TooltipFn<'_>>,
        recognised_command_fn: &mut Option<RecognisedCommandFn<'_>>,
    ) -> Self {
        let tooltip = tooltip_fn.as_mut().and_then(|f| f(content));
        let recognised_command = recognised_command_fn.as_mut().map(|f| f(content));

        let style = name_to_style(highlight_name, recognised_command);
        let span = Span::styled(content.to_string(), style);

        Self {
            start_byte,
            span,
            alternative_span: None,
            highlight_name: highlight_name.map(|s| s.to_string()),
            cursor_info: None,
            tooltip,
        }
    }

    pub fn normal_span(&self) -> &Span<'static> {
        &self.span
    }

    pub fn span_to_use(&self) -> &Span<'static> {
        self.alternative_span.as_ref().unwrap_or(&self.span)
    }

    pub fn clear_alternative_span(&mut self) {
        self.alternative_span = None;
    }

    pub fn set_alternative_span(&mut self, new_alt: Span<'static>) -> Result<(), String> {
        new_alt.content.graphemes(true).zip_longest(self.span.content.graphemes(true))
            .try_for_each(|g| match g {
                EitherOrBoth::Both(new_g, old_g) => {
                    if new_g.width() != old_g.width() {
                        Err(format!("New alternative span has different grapheme widths than the original span. Original grapheme: '{}' (width: {}), new grapheme: '{}' (width: {})", old_g, old_g.width(), new_g, new_g.width()))
                    } else {
                        Ok(())
                    }
                },
                _ => Err("New alternative span has different number of graphemes than the original span".to_string()),
            })?;

        self.alternative_span = Some(new_alt);
        Ok(())
    }

    pub fn split_at_cursor(
        &self,
        split_byte: usize,
        split_grapheme_idx: usize,
    ) -> (Option<Self>, Option<Self>) {
        let build_alt = |alt_span: &Span<'static>, split_idx: usize| {
            let mut left = String::new();
            let mut right = String::new();
            for (idx, g) in alt_span.content.graphemes(true).enumerate() {
                if idx < split_idx {
                    left.push_str(g);
                } else {
                    right.push_str(g);
                }
            }
            (
                if left.is_empty() {
                    None
                } else {
                    Some(Span::styled(left, alt_span.style))
                },
                if right.is_empty() {
                    None
                } else {
                    Some(Span::styled(right, alt_span.style))
                },
            )
        };

        let left_text = &self.span.content[..split_byte];
        let right_text = &self.span.content[split_byte..];

        let (alt_left, alt_right) = match self.alternative_span.as_ref() {
            Some(alt_span) => build_alt(alt_span, split_grapheme_idx),
            None => (None, None),
        };

        let left = if !left_text.is_empty() {
            Some(Self {
                start_byte: self.start_byte,
                span: Span::styled(left_text.to_string(), self.span.style),
                alternative_span: alt_left,
                highlight_name: self.highlight_name.clone(),
                cursor_info: None,
                tooltip: self.tooltip.clone(),
            })
        } else {
            None
        };

        let right = if !right_text.is_empty() {
            Some(Self {
                start_byte: self.start_byte + split_byte,
                span: Span::styled(right_text.to_string(), self.span.style),
                alternative_span: alt_right,
                highlight_name: self.highlight_name.clone(),
                cursor_info: Some(true),
                tooltip: self.tooltip.clone(),
            })
        } else {
            None
        };

        (left, right)
    }
}

pub fn format_buffer<'a>(
    buffer: &TextBuffer,
    mut tooltip_fn: Option<TooltipFn<'a>>,
    mut recognised_command_fn: Option<RecognisedCommandFn<'a>>,
) -> FormattedBuffer {
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
                last_style = HIGHLIGHT_NAMES.get(s.0).map(|s| *s);
                None
            }
            Ok(HighlightEvent::HighlightEnd) => {
                last_style = None;
                None
            }
            Ok(HighlightEvent::Source { start, end }) => {
                // Sometimes a new line will be in the middle of a span, so we need to split it into multiple spans
                let mut lines = vec![];
                let mut span_start = start;
                for (char_idx, c) in source[start..end].char_indices() {
                    let global_char_idx = start + char_idx;
                    if c == '\n' {
                        if span_start < global_char_idx {
                            lines.push((span_start, global_char_idx, last_style));
                        }
                        lines.push((global_char_idx, global_char_idx + 1, last_style));
                        span_start = global_char_idx + 1;
                    }
                }
                if span_start < end {
                    lines.push((span_start, end, last_style));
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
        .map(|(start, end, highlight_name)| {
            FormattedBufferPart::new(
                start,
                &source[start..end],
                highlight_name,
                &mut tooltip_fn,
                &mut recognised_command_fn,
            )
        })
        .collect();

    let cursor_pos = buffer.cursor_byte_pos();
    let byte_length = source.len();

    FormattedBuffer {
        parts: spans,
        cursor_byte_pos: cursor_pos,
        byte_length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    // #[test]
    // #[ignore]
    // fn bash_highlight_example() {
    //     let buf = TextBuffer::new("       for       f in *.rs; do\necho '$f';\n\n;done");

    //     let tooltip_fn = |_|  None;

    //     let formatted_buffer = format_buffer(&buf, tooltip_fn);
    //     for span in formatted_buffer.parts {
    //         eprintln!("{:?}", span);
    //     }

    //     assert!(false);
    // }

    #[test]
    #[ignore]
    fn grapheme_widths() {
        let text = "pyt⢸";
        println!("Text: {:?}", text);
        println!("Text width: {}", text.width());
        for g in text.graphemes(true) {
            println!("'{}  ({:?})' width: {}", g, g.as_bytes(), g.width());
        }
    }
    fn make_part(s: &str) -> FormattedBufferPart {
        FormattedBufferPart {
            start_byte: 0,
            span: Span::from(s.to_string()),
            alternative_span: None,
            highlight_name: None,
            cursor_info: None,
            tooltip: None,
        }
    }

    #[test]
    fn set_alternative_span_accepts_same_grapheme_widths() {
        let mut part = make_part("ab");
        let alt = Span::from("cd".to_string());

        assert!(part.set_alternative_span(alt).is_ok());
        assert_eq!(part.span_to_use().content, "cd");
    }

    #[test]
    fn set_alternative_span_rejects_different_grapheme_count() {
        let mut part = make_part("ab");
        let alt = Span::from("abc".to_string());

        assert!(part.set_alternative_span(alt).is_err());
        assert_eq!(part.span_to_use().content, "ab");
    }

    #[test]
    fn set_alternative_span_rejects_different_grapheme_widths() {
        let mut part = make_part("ab");
        let alt = Span::from("a🙂".to_string());

        assert!(part.set_alternative_span(alt).is_err());
        assert_eq!(part.span_to_use().content, "ab");
    }
}
