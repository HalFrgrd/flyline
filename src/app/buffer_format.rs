use flash::lexer::TokenKind;
use std::vec;

use crate::snake_animation::SnakeAnimation;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::dparser::{AnnotatedToken, ToInclusiveRange, TokenAnnotation};
use crate::palette::Palette;
use itertools::{EitherOrBoth, Itertools};
use ratatui::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub struct FormattedBuffer {
    pub parts: Vec<FormattedBufferPart>,
    pub draw_cursor_at_end: bool, // if true, it means the cursor is after all the tokens, so we should draw a cursor at the end of the line
}

impl FormattedBuffer {
    pub fn get_part_from_byte_pos(&self, byte_pos: usize) -> Option<&FormattedBufferPart> {
        self.parts
            .iter()
            .find(|part| part.token.token.byte_range().contains(&byte_pos))
    }

    /// Create a `FormattedBuffer` from a raw string and cursor position, with no word-info
    /// function. Only intended for use in tests.
    #[cfg(test)]
    pub fn from(input: &str, cursor_pos: usize) -> Self {
        let mut parser = crate::dparser::DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens().to_vec();
        format_buffer(&tokens, cursor_pos, input.len(), false, None)
    }
}

impl Default for FormattedBuffer {
    fn default() -> Self {
        FormattedBuffer {
            parts: vec![],
            draw_cursor_at_end: true,
        }
    }
}

#[derive(Clone)]
pub struct FormattedBufferPart {
    pub token: AnnotatedToken,
    span: Span<'static>,
    /// Meant for animations. Should have the same grapheme widths as span,
    /// but can have different content and style. If present, it will be used
    /// instead of span for display, but span will still be used for cursor
    /// positioning and other logic.
    alternative_span: Option<Arc<dyn Fn(std::time::Instant) -> Span<'static> + Send + Sync>>,
    /// true means cursor is on first grapheme, (and we should draw the contents with the cursor style)
    pub cursor_grapheme_idx: Option<usize>,
    pub tooltip: Option<String>,
}

impl std::fmt::Debug for FormattedBufferPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormattedBufferPart")
            .field("token", &self.token)
            .field("span", &self.span)
            .field(
                "alternative_span",
                &self.alternative_span.as_ref().map(|_| "<fn>"),
            )
            .field("cursor_grapheme_idx", &self.cursor_grapheme_idx)
            .field("tooltip", &self.tooltip)
            .finish()
    }
}

fn token_to_style(
    token: &AnnotatedToken,
    recognised_command: Option<bool>,
    cursor_on_this_or_closing_token: bool,
) -> Style {
    if cursor_on_this_or_closing_token {
        return Palette::opening_and_closing_pair();
    }

    if token.annotation == TokenAnnotation::IsCommandWord {
        if recognised_command == Some(true) {
            return Palette::recognised_word();
        }
        return Palette::unrecognised_word();
    }

    if token.annotation == TokenAnnotation::IsPartOfSingleQuotedString
        || token.token.kind == TokenKind::SingleQuote
    {
        return Palette::single_quoted_word();
    }

    if token.annotation == TokenAnnotation::IsPartOfDoubleQuotedString
        || token.token.kind == TokenKind::Quote
    {
        return Palette::double_quoted_word();
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
        cursor_byte_pos_in_token: Option<usize>,
    ) -> Self {
        let word_info = wordinfo_fn.as_mut().and_then(|f| f(token));
        let tooltip = word_info.as_ref().and_then(|info| info.tooltip.clone());
        let recognised_command = word_info.as_ref().map(|info| info.is_recognised_command);

        let style = token_to_style(token, recognised_command, cursor_on_this_or_closing_token);
        let span = Span::styled(token.token.value.clone(), style);

        let cursor_grapheme_idx = cursor_byte_pos_in_token.map(|byte_pos| {
            log::debug!(
                "Calculating cursor_grapheme_idx for byte_pos {} in token '{}'",
                byte_pos,
                token.token.value
            );

            let mut graph_idx = 0;
            let mut byte_count = 0;
            for g in token.token.value.graphemes(true) {
                let g_byte_len = g.len();
                if byte_count + g_byte_len > byte_pos {
                    break;
                }
                byte_count += g_byte_len;
                graph_idx += 1;
            }
            graph_idx
        });

        if let Some(idx) = cursor_grapheme_idx {
            log::debug!(
                "Cursor byte position {} corresponds to grapheme index {} in token '{}'",
                cursor_byte_pos_in_token.unwrap_or(0),
                idx,
                token.token.value
            );
        }

        let anim_span_fn: Option<
            Arc<dyn Fn(std::time::Instant) -> Span<'static> + Send + Sync>,
        > = if token.annotation == TokenAnnotation::IsCommandWord
            && token.token.value.starts_with("python")
        {
            let normal_string = token.token.value.clone();
            let snake_animation = SnakeAnimation::new();
            
            Some(Arc::new(move |now| {
                snake_animation.update_anim(now);
                let snake_str = snake_animation.apply_to_string(&normal_string);
                Span::styled(snake_str.clone(), Palette::recognised_word())
            }))
        } else {
            None
        };

        Self {
            token: token.clone(),
            span,
            alternative_span: anim_span_fn,
            cursor_grapheme_idx,
            tooltip,
        }
    }

    pub fn normal_span(&self) -> &Span<'static> {
        &self.span
    }

    pub fn get_possible_animated_span(&self, now: std::time::Instant) -> Span<'static> {
        if let Some(anim_fn) = &self.alternative_span {
            let anim_span = anim_fn(now);
            if let Err(e) =
                Self::check_anim_span_matches_graph_boundaries(&self.span, anim_span.clone())
            {
                log::error!(
                    "Animation span for token '{}' does not match grapheme boundaries of normal span. Error: {}. Falling back to normal span.",
                    self.token.token.value,
                    e
                );
            } else {
                return anim_span;
            }
        }
        self.span.clone()
    }

    fn check_anim_span_matches_graph_boundaries<'a>(
        normal_span: &Span<'a>,
        new_alt: Span<'a>,
    ) -> Result<(), String> {
        new_alt.content.graphemes(true).zip_longest(normal_span.content.graphemes(true))
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

        Ok(())
    }
}

pub fn format_buffer<'a>(
    annotated_tokens: &[AnnotatedToken],
    cursor_byte_pos: usize,
    buffer_byte_length: usize,
    app_is_running: bool,
    mut wordinfo_fn: Option<WordInfoFn<'a>>,
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
                    TokenAnnotation::IsClosing {
                        opening_idx: corresponding_idx,
                        ..
                    } => {
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
            let cursor_pos_in_token = if tok.token.byte_range().contains(&cursor_byte_pos) {
                Some(cursor_byte_pos - tok.token.byte_range().start)
            } else {
                None
            };
            FormattedBufferPart::new(tok, &mut wordinfo_fn, highlight, cursor_pos_in_token)
        })
        .collect();

    FormattedBuffer {
        parts: spans,
        draw_cursor_at_end: cursor_byte_pos >= buffer_byte_length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dparser::TokenAnnotation;

    // Helper: find all parts whose token value equals `val`.
    fn parts_with_value<'a>(fb: &'a FormattedBuffer, val: &str) -> Vec<&'a FormattedBufferPart> {
        fb.parts
            .iter()
            .filter(|p| p.token.token.value == val)
            .collect()
    }

    // ── FormattedBuffer::from ────────────────────────────────────────────────

    #[test]
    fn from_empty_string() {
        let fb = FormattedBuffer::from("", 0);
        assert!(fb.parts.is_empty());
        assert!(fb.draw_cursor_at_end);
    }

    #[test]
    fn from_annotates_opening_double_quote() {
        // `echo "` – the double quote is an unmatched opener.
        let input = r#"echo ""#;
        let cursor = input.len();
        let fb = FormattedBuffer::from(input, cursor);
        let quotes = parts_with_value(&fb, "\"");
        assert_eq!(quotes.len(), 1);
        assert!(
            matches!(quotes[0].token.annotation, TokenAnnotation::IsOpening(_)),
            "expected IsOpening, got {:?}",
            quotes[0].token.annotation
        );
    }

    #[test]
    fn from_annotates_closing_double_quote() {
        // `echo "hello"` – the second double quote is a closer.
        let input = r#"echo "hello""#;
        let cursor = input.len();
        let fb = FormattedBuffer::from(input, cursor);
        let quotes = parts_with_value(&fb, "\"");
        assert_eq!(quotes.len(), 2);
        assert!(matches!(
            quotes[0].token.annotation,
            TokenAnnotation::IsOpening(_)
        ));
        assert!(matches!(
            quotes[1].token.annotation,
            TokenAnnotation::IsClosing { .. }
        ));
    }

    #[test]
    fn from_annotates_opening_single_quote() {
        let input = "echo '";
        let fb = FormattedBuffer::from(input, input.len());
        let sq = parts_with_value(&fb, "'");
        assert_eq!(sq.len(), 1);
        assert!(matches!(
            sq[0].token.annotation,
            TokenAnnotation::IsOpening(_)
        ));
    }

    #[test]
    fn from_annotates_opening_brace() {
        let input = "echo {";
        let fb = FormattedBuffer::from(input, input.len());
        let braces = parts_with_value(&fb, "{");
        assert_eq!(braces.len(), 1);
        assert!(matches!(
            braces[0].token.annotation,
            TokenAnnotation::IsOpening(_)
        ));
    }
}
