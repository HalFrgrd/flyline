use flash::lexer::TokenKind;
use std::vec;

use crate::snake_animation::SnakeAnimation;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[cfg(not(test))]
use crate::bash_funcs;
use crate::dparser::{AnnotatedToken, ClosingAnnotation, ToInclusiveRange};
use crate::palette::Palette;
use itertools::{EitherOrBoth, Itertools};
use ratatui::prelude::*;
use std::sync::{Arc, Mutex, OnceLock};

// Store it globally so that the animation looks smooth between calls
static SNAKE_ANIMATION: OnceLock<Mutex<SnakeAnimation>> = OnceLock::new();

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

    /// Create a `FormattedBuffer` from a raw string and cursor position. Only intended for use in tests.
    #[cfg(test)]
    pub fn from(input: &str, cursor_pos: usize) -> Self {
        let mut parser = crate::dparser::DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens().to_vec();
        format_buffer(&tokens, cursor_pos, input.len(), false, &Palette::dark())
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
    /// We can replace the span with an animated version.
    /// The animated span should have the same grapheme widths as span,
    /// but can have different content and style. If present, it will be used
    /// instead of span for display, but span will still be used for cursor
    /// positioning and other logic.
    animated_span_fn: Option<Arc<dyn Fn(std::time::Instant) -> Span<'static> + Send + Sync>>,
    /// Where to draw the cursor if it is on this token. This is a grapheme index, not a byte index.
    pub cursor_grapheme_idx: Option<usize>,
    pub tooltip: Option<String>,
}

impl std::fmt::Debug for FormattedBufferPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormattedBufferPart")
            .field("token", &self.token)
            .field("span", &self.span)
            .field(
                "animated_span_fn",
                &self.animated_span_fn.as_ref().map(|_| "<fn>"),
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
    palette: &Palette,
) -> Style {
    if cursor_on_this_or_closing_token {
        return palette.opening_and_closing_pair();
    }

    // Env var coloring has the highest priority among base colors: a token can have both
    // `is_env_var` and `is_inside_double_quotes` (e.g. `$HOME` in `"$HOME"`), and the env var
    // color should win over the double-quoted color.
    if token.annotations.is_env_var {
        return palette.env_var();
    }

    if token.annotations.command_word.is_some() {
        if recognised_command == Some(true) {
            return palette.recognised_command();
        }
        return palette.unrecognised_command();
    }

    if token.annotations.is_inside_single_quotes || token.token.kind == TokenKind::SingleQuote {
        return palette.single_quoted_text();
    }

    if token.annotations.is_inside_double_quotes || token.token.kind == TokenKind::Quote {
        return palette.double_quoted_text();
    }

    if token.annotations.is_comment {
        return palette.comment();
    }

    palette.normal_text()
}

#[derive(Debug)]
struct WordInfo {
    pub tooltip: Option<String>,
    pub is_recognised_command: bool,
}

#[cfg(not(test))]
fn get_word_info(token: &AnnotatedToken) -> Option<WordInfo> {
    if token.annotations.is_env_var && token.token.kind.is_word() {
        let env_var_name = &token.token.value;

        let tooltip = bash_funcs::format_shell_var(env_var_name);

        return Some(WordInfo {
            tooltip: Some(tooltip),
            is_recognised_command: false,
        });
    } else if let Some(value) = &token.annotations.command_word {
        let (command_type, description) = bash_funcs::get_command_info(value);
        return Some(WordInfo {
            tooltip: Some(description.to_string()),
            is_recognised_command: command_type != bash_funcs::CommandType::Unknown,
        });
    } else if token.annotations.is_empty() && token.token.value.starts_with('~') {
        let expanded = bash_funcs::expand_filename(&token.token.value);
        if expanded != token.token.value {
            return Some(WordInfo {
                tooltip: Some(format!("{}={}", token.token.value, expanded)),
                is_recognised_command: false,
            });
        }
    }
    None
}

#[cfg(test)]
fn get_word_info(_token: &AnnotatedToken) -> Option<WordInfo> {
    None
}

impl FormattedBufferPart {
    pub fn new(
        token: &AnnotatedToken,
        cursor_on_this_or_closing_token: bool,
        cursor_byte_pos_in_token: Option<usize>,
        palette: &Palette,
    ) -> Self {
        let word_info = get_word_info(token);
        let tooltip = word_info.as_ref().and_then(|info| info.tooltip.clone());
        let recognised_command = word_info.as_ref().map(|info| info.is_recognised_command);

        let style = token_to_style(
            token,
            recognised_command,
            cursor_on_this_or_closing_token,
            palette,
        );
        let span = Span::styled(token.token.value.clone(), style);

        let cursor_grapheme_idx = cursor_byte_pos_in_token.map(|byte_pos| {
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

        let animated_span_fn: Option<
            Arc<dyn Fn(std::time::Instant) -> Span<'static> + Send + Sync>,
        > = if token.annotations.command_word.is_some() && token.token.value.starts_with("python") {
            let normal_string = token.token.value.clone();
            let recognised_style = palette.recognised_command();

            Some(Arc::new(move |now| {
                let mut anim = SNAKE_ANIMATION
                    .get_or_init(|| Mutex::new(SnakeAnimation::new()))
                    .lock()
                    .unwrap();
                anim.update_anim(now);
                let snake_str = anim.apply_to_string(&normal_string);
                Span::styled(snake_str, recognised_style)
            }))
        } else {
            None
        };

        Self {
            token: token.clone(),
            span,
            animated_span_fn,
            cursor_grapheme_idx,
            tooltip,
        }
    }

    pub fn normal_span(&self) -> &Span<'static> {
        &self.span
    }

    pub fn get_possible_animated_span(&self, now: std::time::Instant) -> Span<'static> {
        if let Some(anim_fn) = &self.animated_span_fn {
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

    /// Returns the number of graphemes in this part's normal span.
    #[allow(dead_code)]
    fn grapheme_count(&self) -> usize {
        self.span.content.graphemes(true).count()
    }

    /// Split this part at grapheme index `n`. Returns `(left, right)` where
    /// `left` contains the first `n` graphemes of the original part and
    /// `right` contains the remaining graphemes.
    ///
    /// Both halves share the original `token` and `tooltip`. The
    /// `cursor_grapheme_idx` is moved to whichever half it falls into. If an
    /// `animated_span_fn` is present, both halves get a wrapped copy that
    /// invokes the original closure and then takes the first `n` graphemes
    /// (left) or skips the first `n` graphemes (right) of its result.
    ///
    /// `n` is clamped to the range `[0, grapheme_count]`.
    #[allow(dead_code)]
    pub fn split_at(&self, n: usize) -> (FormattedBufferPart, FormattedBufferPart) {
        let total = self.grapheme_count();
        let n = n.min(total);

        let graphemes: Vec<&str> = self.span.content.graphemes(true).collect();
        let (left_graphemes, right_graphemes) = graphemes.split_at(n);
        let left_content: String = left_graphemes.concat();
        let right_content: String = right_graphemes.concat();
        let left_span = Span::styled(left_content, self.span.style);
        let right_span = Span::styled(right_content, self.span.style);

        let (left_cursor_idx, right_cursor_idx) = match self.cursor_grapheme_idx {
            Some(idx) if idx < n => (Some(idx), None),
            Some(idx) => (None, Some(idx - n)),
            None => (None, None),
        };

        let (left_anim_fn, right_anim_fn) = match &self.animated_span_fn {
            Some(orig) => {
                let orig_left = orig.clone();
                let orig_right = orig.clone();
                let take_n = n;
                let skip_n = n;
                let left_fn: Arc<dyn Fn(std::time::Instant) -> Span<'static> + Send + Sync> =
                    Arc::new(move |now| {
                        let span = orig_left(now);
                        let content: String = span.content.graphemes(true).take(take_n).collect();
                        Span::styled(content, span.style)
                    });
                let right_fn: Arc<dyn Fn(std::time::Instant) -> Span<'static> + Send + Sync> =
                    Arc::new(move |now| {
                        let span = orig_right(now);
                        let content: String = span.content.graphemes(true).skip(skip_n).collect();
                        Span::styled(content, span.style)
                    });
                (Some(left_fn), Some(right_fn))
            }
            None => (None, None),
        };

        let left = FormattedBufferPart {
            token: self.token.clone(),
            span: left_span,
            animated_span_fn: left_anim_fn,
            cursor_grapheme_idx: left_cursor_idx,
            tooltip: self.tooltip.clone(),
        };
        let right = FormattedBufferPart {
            token: self.token.clone(),
            span: right_span,
            animated_span_fn: right_anim_fn,
            cursor_grapheme_idx: right_cursor_idx,
            tooltip: self.tooltip.clone(),
        };
        (left, right)
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

pub fn format_buffer(
    annotated_tokens: &[AnnotatedToken],
    cursor_byte_pos: usize,
    buffer_byte_length: usize,
    app_is_running: bool,
    palette: &Palette,
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

                if let Some(crate::dparser::OpeningState::Matched(corresponding_idx)) =
                    tok.annotations.opening
                {
                    range_check(tok)
                        || annotated_tokens
                            .get(corresponding_idx)
                            .is_some_and(range_check)
                } else if let Some(ClosingAnnotation {
                    opening_idx: corresponding_idx,
                    ..
                }) = tok.annotations.closing
                {
                    range_check(tok)
                        || annotated_tokens
                            .get(corresponding_idx)
                            .is_some_and(range_check)
                } else {
                    false
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
            FormattedBufferPart::new(tok, highlight, cursor_pos_in_token, palette)
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
            quotes[0].token.annotations.opening.is_some(),
            "expected opening annotation, got {:?}",
            quotes[0].token.annotations
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
        assert!(quotes[0].token.annotations.opening.is_some());
        assert!(quotes[1].token.annotations.closing.is_some());
    }

    #[test]
    fn from_annotates_opening_single_quote() {
        let input = "echo '";
        let fb = FormattedBuffer::from(input, input.len());
        let sq = parts_with_value(&fb, "'");
        assert_eq!(sq.len(), 1);
        assert!(sq[0].token.annotations.opening.is_some());
    }

    #[test]
    fn from_annotates_opening_brace() {
        let input = "echo {";
        let fb = FormattedBuffer::from(input, input.len());
        let braces = parts_with_value(&fb, "{");
        assert_eq!(braces.len(), 1);
        assert!(braces[0].token.annotations.opening.is_some());
    }

    // ── FormattedBufferPart::split_at ────────────────────────────────────

    fn first_word_part(input: &str, value: &str) -> FormattedBufferPart {
        let fb = FormattedBuffer::from(input, input.len());
        fb.parts
            .into_iter()
            .find(|p| p.token.token.value == value)
            .expect("expected to find the requested token in the formatted buffer")
    }

    #[test]
    fn split_at_zero_yields_empty_left() {
        let part = first_word_part("hello", "hello");
        let (left, right) = part.split_at(0);
        assert_eq!(left.normal_span().content, "");
        assert_eq!(right.normal_span().content, "hello");
    }

    #[test]
    fn split_at_full_length_yields_empty_right() {
        let part = first_word_part("hello", "hello");
        let total = part.grapheme_count();
        let (left, right) = part.split_at(total);
        assert_eq!(left.normal_span().content, "hello");
        assert_eq!(right.normal_span().content, "");
    }

    #[test]
    fn split_at_in_middle_partitions_graphemes() {
        let part = first_word_part("hello", "hello");
        let (left, right) = part.split_at(2);
        assert_eq!(left.normal_span().content, "he");
        assert_eq!(right.normal_span().content, "llo");
    }

    #[test]
    fn split_at_clamps_oversized_index() {
        let part = first_word_part("hi", "hi");
        let (left, right) = part.split_at(99);
        assert_eq!(left.normal_span().content, "hi");
        assert_eq!(right.normal_span().content, "");
    }

    #[test]
    fn split_at_routes_cursor_into_correct_half() {
        // Cursor positioned after "he" in "hello".
        let fb = FormattedBuffer::from("hello", 2);
        let part = fb
            .parts
            .into_iter()
            .find(|p| p.token.token.value == "hello")
            .unwrap();
        assert_eq!(part.cursor_grapheme_idx, Some(2));

        // Split before the cursor — cursor moves to the right half.
        let (left, right) = part.clone().split_at(1);
        assert_eq!(left.cursor_grapheme_idx, None);
        assert_eq!(right.cursor_grapheme_idx, Some(1));

        // Split after the cursor — cursor stays in the left half.
        let (left, right) = part.clone().split_at(3);
        assert_eq!(left.cursor_grapheme_idx, Some(2));
        assert_eq!(right.cursor_grapheme_idx, None);

        // Split exactly at the cursor — cursor goes to the right half (index 0).
        let (left, right) = part.split_at(2);
        assert_eq!(left.cursor_grapheme_idx, None);
        assert_eq!(right.cursor_grapheme_idx, Some(0));
    }

    #[test]
    fn split_at_preserves_grapheme_boundaries_for_multi_byte() {
        // "a世b" — three graphemes, the middle one is multi-byte.
        let part = first_word_part("a世b", "a世b");
        let (left, right) = part.split_at(2);
        assert_eq!(left.normal_span().content, "a世");
        assert_eq!(right.normal_span().content, "b");
    }

    #[test]
    fn split_at_propagates_animated_span_fn() {
        use std::sync::Arc;

        // Build a part with an animated span fn returning "ABCDE" so we can
        // verify the wrapped left/right closures slice graphemes correctly.
        let part = first_word_part("hello", "hello");
        let style = part.normal_span().style;
        let animated: Arc<dyn Fn(std::time::Instant) -> Span<'static> + Send + Sync> =
            Arc::new(move |_| Span::styled("ABCDE", style));
        let part = FormattedBufferPart {
            animated_span_fn: Some(animated),
            ..part
        };

        let (left, right) = part.split_at(2);
        let now = std::time::Instant::now();
        assert_eq!(left.get_possible_animated_span(now).content, "AB");
        assert_eq!(right.get_possible_animated_span(now).content, "CDE");
    }
}
