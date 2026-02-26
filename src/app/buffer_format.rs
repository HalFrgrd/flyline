use flash::lexer::{Position, Token, TokenKind};
use std::borrow::Cow;
use std::sync::Arc;
use std::vec;

use unicode_segmentation::UnicodeSegmentation;

use crate::dparser::{AnnotatedToken, ToInclusiveRange, TokenAnnotation};
use crate::palette::Palette;
use ratatui::prelude::*;

/// A closure that takes the normal span and returns an animated span with grapheme-width-matching content.
#[derive(Clone)]
pub struct AnimatedSpanFn(Arc<dyn Fn(&Span<'static>) -> Span<'static> + Send + Sync>);

impl AnimatedSpanFn {
    pub fn new(f: impl Fn(&Span<'static>) -> Span<'static> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
}

impl std::fmt::Debug for AnimatedSpanFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnimatedSpanFn")
    }
}

pub type AnimatedSpanFnProvider<'a> = Box<dyn FnMut(&AnnotatedToken) -> Option<AnimatedSpanFn> + 'a>;

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
                let part_start = part.token.token.byte_range().start;

                let mut parts = vec![];
                let split = if part.token.token.byte_range().contains(&cursor_pos) {
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
                        token: AnnotatedToken {
                            token: Token {
                                kind: TokenKind::Whitespace(space.clone()),
                                value: space.clone(),
                                position: Position {
                                    byte: cursor_pos,
                                    line: 0,
                                    column: 0,
                                },
                            },
                            annotation: TokenAnnotation::None,
                        },
                        span: Span::from(space),
                        animated_span: None,
                        is_cursor_on_first_grapheme: true,
                        is_artificial_space: true,
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
            .find(|part| part.token.token.byte_range().contains(&byte_pos))
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
    pub token: AnnotatedToken,
    span: Span<'static>,
    /// Meant for animations. A closure that takes the normal span and returns a grapheme-width-matching span.
    /// If present, it will be used instead of span for display, but span will still be used for
    /// cursor positioning and other logic.
    animated_span: Option<AnimatedSpanFn>,
    /// true means cursor is on first grapheme, (and we should draw the contents with the cursor style)
    pub is_cursor_on_first_grapheme: bool,
    pub is_artificial_space: bool, // whether this part is an artificial space added for cursor positioning at the end of the buffer
    pub tooltip: Option<String>,
}

fn token_to_style(
    token: &AnnotatedToken,
    recognised_command: Option<bool>,
    cursor_on_this_or_closing_token: bool,
) -> Style {
    if cursor_on_this_or_closing_token {
        return Palette::opening_and_closing_pair();
    }

    if recognised_command == Some(true) {
        return Palette::recognised_word();
    }

    if token.annotation == TokenAnnotation::IsPartOfQuotedString
        || matches!(token.token.kind, TokenKind::SingleQuote | TokenKind::Quote)
    {
        return Palette::unrecognised_word();
    }
    Palette::normal_text()
}

#[derive(Debug)]
pub struct WordInfo {
    pub tooltip: Option<String>,
    pub is_recognised_command: bool,
}

pub type WordInfoFn<'a> = Box<dyn FnMut(&AnnotatedToken) -> Option<WordInfo> + 'a>;

impl FormattedBufferPart {
    pub fn new(
        token: &AnnotatedToken,
        wordinfo_fn: &mut Option<WordInfoFn<'_>>,
        cursor_on_this_or_closing_token: bool,
    ) -> Self {
        let word_info = wordinfo_fn.as_mut().and_then(|f| f(token));
        let tooltip = word_info.as_ref().and_then(|info| info.tooltip.clone());
        let recognised_command = word_info.as_ref().map(|info| info.is_recognised_command);

        let style = token_to_style(&token, recognised_command, cursor_on_this_or_closing_token);
        let span = Span::styled(token.token.value.clone(), style);

        Self {
            token: token.clone(),
            span,
            animated_span: None,
            is_cursor_on_first_grapheme: false,
            is_artificial_space: false,
            tooltip,
        }
    }

    pub fn normal_span(&self) -> &Span<'static> {
        &self.span
    }

    pub fn span_to_use(&self) -> std::borrow::Cow<'_, Span<'static>> {
        if let Some(f) = &self.animated_span {
            std::borrow::Cow::Owned((f.0)(&self.span))
        } else {
            std::borrow::Cow::Borrowed(&self.span)
        }
    }

    pub fn set_animated_span(&mut self, f: AnimatedSpanFn) {
        self.animated_span = Some(f);
    }

    pub fn clear_animated_span(&mut self) {
        self.animated_span = None;
    }

    pub fn split_at_cursor(
        &self,
        split_byte: usize,
        split_grapheme_idx: usize,
    ) -> (Option<Self>, Option<Self>) {
        let left_text = &self.span.content[..split_byte];
        let right_text = &self.span.content[split_byte..];

        let left = if !left_text.is_empty() {
            Some(Self {
                token: AnnotatedToken {
                    token: Token {
                        kind: self.token.token.kind.clone(),
                        value: left_text.to_string(),
                        position: self.token.token.position,
                    },
                    annotation: self.token.annotation.clone(),
                },
                span: Span::styled(left_text.to_string(), self.span.style),
                animated_span: self.animated_span.clone(),
                is_cursor_on_first_grapheme: false,
                is_artificial_space: self.is_artificial_space,
                tooltip: self.tooltip.clone(),
            })
        } else {
            None
        };

        let right = if !right_text.is_empty() {
            Some(Self {
                token: AnnotatedToken {
                    token: Token {
                        kind: self.token.token.kind.clone(),
                        value: right_text.to_string(),
                        position: Position {
                            byte: self.token.token.position.byte + split_byte,
                            line: self.token.token.position.line,
                            column: self.token.token.position.column + split_grapheme_idx,
                        },
                    },
                    annotation: self.token.annotation.clone(),
                },
                span: Span::styled(right_text.to_string(), self.span.style),
                animated_span: self.animated_span.clone(),
                is_cursor_on_first_grapheme: true,
                is_artificial_space: self.is_artificial_space,
                tooltip: self.tooltip.clone(),
            })
        } else {
            None
        };

        (left, right)
    }
}

pub fn format_buffer<'a>(
    annotated_tokens: &[AnnotatedToken],
    cursor_byte_pos: usize,
    buffer_byte_length: usize,
    app_is_running: bool,
    mut wordinfo_fn: Option<WordInfoFn<'a>>,
    mut animated_span_fn: Option<AnimatedSpanFnProvider<'a>>,
) -> FormattedBuffer {
    let check_highlight = |inclusive: bool| {
        annotated_tokens
            .iter()
            .map(|tok| {
                let range_check = |t: &AnnotatedToken| {
                    let range = t.token.byte_range();
                    if inclusive {
                        range.to_inclusive().contains(&cursor_byte_pos)
                    } else {
                        range.contains(&cursor_byte_pos)
                    }
                };

                match tok.annotation {
                    TokenAnnotation::IsOpening(Some(corresponding_idx)) => {
                        range_check(tok)
                            || annotated_tokens
                                .get(corresponding_idx)
                                .is_some_and(range_check)
                    }
                    TokenAnnotation::IsClosing(corresponding_idx) => {
                        range_check(tok)
                            || annotated_tokens
                                .get(corresponding_idx)
                                .is_some_and(range_check)
                    }
                    _ => false,
                }
            })
            .collect::<Vec<bool>>()
    };

    let strict_highlight = check_highlight(false);
    let inclusive_highlight = check_highlight(true);

    let use_inclusive = !strict_highlight.iter().any(|&b| b);

    let spans: Vec<FormattedBufferPart> = annotated_tokens
        .iter()
        .enumerate()
        .map(|(idx, tok)| {
            let highlight = app_is_running
                && (strict_highlight[idx] || (use_inclusive && inclusive_highlight[idx]));

            let mut part = FormattedBufferPart::new(tok, &mut wordinfo_fn, highlight);
            if let Some(f) = animated_span_fn.as_mut().and_then(|p| p(tok)) {
                part.set_animated_span(f);
            }
            part
        })
        .collect();

    let cursor_pos = cursor_byte_pos;
    let buf_byte_length = buffer_byte_length;
    FormattedBuffer {
        parts: spans,
        cursor_byte_pos: cursor_pos,
        buf_byte_length,
    }
}
