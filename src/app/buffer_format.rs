use flash::lexer::{Position, Token, TokenKind};
use std::borrow::Cow;
use std::vec;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::dparser::collect_tokens_include_whitespace;
use crate::palette::Palette;
use crate::text_buffer::TextBuffer;
use itertools::{EitherOrBoth, Itertools};
use ratatui::prelude::*;

#[derive(Debug)]
pub struct FormattedBuffer {
    pub parts: Vec<FormattedBufferPart>,
    pub cursor_byte_pos: usize,
    buf_byte_length: usize,
}

impl FormattedBuffer {
    pub fn split_at_cursor_from<'a>(
        &'a self,
    ) -> impl Iterator<Item = Cow<'a, FormattedBufferPart>> {
        let cursor_pos = self.cursor_byte_pos;

        self.parts
            .iter()
            .flat_map(move |part| {
                let part_start = part.token.byte_range().start;

                let mut parts = vec![];
                let split = if part.token.byte_range().contains(&cursor_pos) {
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
                if cursor_pos >= self.buf_byte_length {
                    let space = " ".to_string();

                    Some(Cow::Owned(FormattedBufferPart {
                        token: Token {
                            kind: TokenKind::Whitespace(space.clone()),
                            value: space.clone(),
                            position: Position {
                                byte: cursor_pos,
                                line: 0,
                                column: 0,
                            },
                        },
                        span: Span::from(space),
                        alternative_span: None,
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
        self.parts
            .iter()
            .find(|part| part.token.byte_range().contains(&byte_pos))
    }
}

impl Default for FormattedBuffer {
    fn default() -> Self {
        FormattedBuffer {
            parts: vec![],
            cursor_byte_pos: 0,
            buf_byte_length: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormattedBufferPart {
    pub token: flash::lexer::Token,
    span: Span<'static>,
    /// Meant for animations. Should have the same grapheme widths as span,
    /// but can have different content and style. If present, it will be used
    /// instead of span for display, but span will still be used for cursor
    /// positioning and other logic.
    alternative_span: Option<Span<'static>>,
    /// None means no cursor,
    /// Some(true) means cursor is on an actual grapheme, (and we should draw the contents with the cursor style)
    /// Some(false) means cursor is on an artificial position (e.g. end of line)
    pub cursor_info: Option<bool>,
    pub tooltip: Option<String>,
}

fn token_kind_to_style(kind: &TokenKind, recognised_command: Option<bool>) -> Style {
    match kind {
        TokenKind::Word(_) if recognised_command.unwrap_or(false) => Palette::recognised_word(),
        TokenKind::Word(w) if w.starts_with("'") || w.starts_with("\"") => {
            Palette::unrecognised_word()
        }
        _ => Palette::normal_text(),
    }
}

#[derive(Debug)]
pub struct WordInfo {
    pub tooltip: Option<String>,
    pub is_recognised_command: bool,
}

pub type WordInfoFn<'a> = Box<dyn FnMut(&str, Option<&'static str>) -> Option<WordInfo> + 'a>;

impl FormattedBufferPart {
    pub fn new(token: Token, wordinfo_fn: &mut Option<WordInfoFn<'_>>) -> Self {
        let word_info = wordinfo_fn.as_mut().and_then(|f| f(&token.value, None));
        let tooltip = word_info.as_ref().and_then(|info| info.tooltip.clone());
        let recognised_command = word_info.as_ref().map(|info| info.is_recognised_command);

        let style = token_kind_to_style(&token.kind, recognised_command);
        let span = Span::styled(token.value.clone(), style);

        Self {
            token,
            span,
            alternative_span: None,
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
                token: Token {
                    kind: self.token.kind.clone(),
                    value: left_text.to_string(),
                    position: self.token.position,
                },
                span: Span::styled(left_text.to_string(), self.span.style),
                alternative_span: alt_left,
                cursor_info: None,
                tooltip: self.tooltip.clone(),
            })
        } else {
            None
        };

        let right = if !right_text.is_empty() {
            Some(Self {
                token: Token {
                    kind: self.token.kind.clone(),
                    value: right_text.to_string(),
                    position: Position {
                        byte: self.token.position.byte + split_byte,
                        line: self.token.position.line,
                        column: self.token.position.column + split_grapheme_idx,
                    },
                },
                span: Span::styled(right_text.to_string(), self.span.style),
                alternative_span: alt_right,
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
    mut wordinfo_fn: Option<WordInfoFn<'a>>,
) -> FormattedBuffer {
    let tokens = collect_tokens_include_whitespace(buffer.buffer());

    let spans: Vec<FormattedBufferPart> = tokens
        .into_iter()
        .map(|tok| FormattedBufferPart::new(tok, &mut wordinfo_fn))
        .collect();

    let cursor_pos = buffer.cursor_byte_pos();
    let buf_byte_length = buffer.buffer().len();

    FormattedBuffer {
        parts: spans,
        cursor_byte_pos: cursor_pos,
        buf_byte_length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]

    // fn test_format_buffer() {
    //     let buffer = TextBuffer::new("echo \"hel\nlo\"");
    //     let formatted = format_buffer(&buffer, None);
    //     println!("{:#?}", formatted);

    //     panic!("Test not implemented yet");
    // }
}
