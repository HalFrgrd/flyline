use flash::lexer::TokenKind;
use std::vec;

use crate::snake_animation::SnakeAnimation;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[cfg(not(test))]
use crate::bash_funcs;
use crate::content_builder::Tag;
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
    pub fn from(input: &str, cursor_pos: usize, selection_byte: Option<usize>) -> Self {
        let mut parser = crate::dparser::DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens().to_vec();
        format_buffer(
            &tokens,
            cursor_pos,
            selection_byte,
            input.len(),
            false,
            &Palette::dark(),
        )
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
    /// Where the selection anchor falls within this token, as a grapheme
    /// index. `Some` only when the selection_byte position lies inside this
    /// token's byte range.
    pub selection_byte_grapheme_idx: Option<usize>,
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
            .field(
                "selection_byte_grapheme_idx",
                &self.selection_byte_grapheme_idx,
            )
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
        selection_byte_pos_in_token: Option<usize>,
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

        let byte_pos_to_grapheme_idx = |byte_pos: usize| {
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
        };

        let cursor_grapheme_idx = cursor_byte_pos_in_token.map(byte_pos_to_grapheme_idx);
        let selection_byte_grapheme_idx = selection_byte_pos_in_token.map(byte_pos_to_grapheme_idx);

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
            selection_byte_grapheme_idx,
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

    /// Yield drawable sub-spans for this part along with metadata.
    ///
    /// Each yielded item is `(span, graph_idx_to_tag, is_cursor,
    /// is_selection_byte, is_in_selection)`.
    ///
    /// - When this part contains neither the cursor nor the selection
    ///   anchor, a single tuple covering the whole part is returned. Its
    ///   `is_in_selection` is computed from `selection_range` (i.e. whether
    ///   the part lies wholly inside the selection range).
    /// - When this part contains the cursor or the selection anchor (or
    ///   both), it is split into one tuple per grapheme so that each
    ///   single-cell flag can be independently set.
    pub fn get_spans(
        &self,
        animation_time: Option<std::time::Instant>,
        selection_range: Option<std::ops::Range<usize>>,
    ) -> Vec<(Span<'static>, Vec<Tag>, bool, bool, bool)> {
        let display_span = if self.token.token.kind == TokenKind::Newline {
            // For newlines, draw a space instead so that we can have a place to put the cursor
            Span::from(" ")
        } else if let Some(now) = animation_time {
            self.get_possible_animated_span(now)
        } else {
            self.normal_span().clone()
        };

        let token_byte_start = self.token.token.byte_range().start;

        let has_cursor = self.cursor_grapheme_idx.is_some();
        let has_sel_byte = self.selection_byte_grapheme_idx.is_some();

        // Compute "in selection" for a grapheme starting at the given
        // absolute buffer byte offset. The selection range is half-open.
        let in_selection_at =
            |byte: usize| -> bool { selection_range.as_ref().is_some_and(|r| r.contains(&byte)) };

        if !has_cursor && !has_sel_byte {
            // Single tuple covering the whole part.
            let tags: Vec<Tag> = self
                .span
                .content
                .graphemes(true)
                .scan(token_byte_start, |acc, graph| {
                    let tag = Tag::Command(*acc);
                    *acc += graph.len();
                    Some(tag)
                })
                .collect();
            let is_in_sel = in_selection_at(token_byte_start) && !tags.is_empty();
            return vec![(display_span, tags, false, false, is_in_sel)];
        }

        // Split into per-grapheme tuples.
        let normal_graphemes: Vec<&str> = self.span.content.graphemes(true).collect();
        let display_graphemes: Vec<&str> = display_span.content.graphemes(true).collect();
        let display_style = display_span.style;

        let mut out = Vec::with_capacity(normal_graphemes.len());
        let mut byte = token_byte_start;
        for (g_idx, normal_g) in normal_graphemes.iter().enumerate() {
            let display_g = display_graphemes.get(g_idx).copied().unwrap_or(normal_g);
            let single_span = Span::styled(display_g.to_string(), display_style);
            let tags = vec![Tag::Command(byte)];
            let is_cursor = Some(g_idx) == self.cursor_grapheme_idx;
            let is_sel_byte = Some(g_idx) == self.selection_byte_grapheme_idx;
            let is_in_sel = in_selection_at(byte);
            out.push((single_span, tags, is_cursor, is_sel_byte, is_in_sel));
            byte += normal_g.len();
        }
        out
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
    selection_byte_pos: Option<usize>,
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
            let selection_pos_in_token = selection_byte_pos.and_then(|s| {
                if tok.token.byte_range().contains(&s) {
                    Some(s - tok.token.byte_range().start)
                } else {
                    None
                }
            });
            FormattedBufferPart::new(
                tok,
                highlight,
                cursor_pos_in_token,
                selection_pos_in_token,
                palette,
            )
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
        let fb = FormattedBuffer::from("", 0, None);
        assert!(fb.parts.is_empty());
        assert!(fb.draw_cursor_at_end);
    }

    #[test]
    fn from_annotates_opening_double_quote() {
        // `echo "` – the double quote is an unmatched opener.
        let input = r#"echo ""#;
        let cursor = input.len();
        let fb = FormattedBuffer::from(input, cursor, None);
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
        let fb = FormattedBuffer::from(input, cursor, None);
        let quotes = parts_with_value(&fb, "\"");
        assert_eq!(quotes.len(), 2);
        assert!(quotes[0].token.annotations.opening.is_some());
        assert!(quotes[1].token.annotations.closing.is_some());
    }

    #[test]
    fn from_annotates_opening_single_quote() {
        let input = "echo '";
        let fb = FormattedBuffer::from(input, input.len(), None);
        let sq = parts_with_value(&fb, "'");
        assert_eq!(sq.len(), 1);
        assert!(sq[0].token.annotations.opening.is_some());
    }

    #[test]
    fn from_annotates_opening_brace() {
        let input = "echo {";
        let fb = FormattedBuffer::from(input, input.len(), None);
        let braces = parts_with_value(&fb, "{");
        assert_eq!(braces.len(), 1);
        assert!(braces[0].token.annotations.opening.is_some());
    }

    // ── FormattedBufferPart::split_at ────────────────────────────────────

    fn first_word_part(input: &str, value: &str) -> FormattedBufferPart {
        let fb = FormattedBuffer::from(input, input.len(), None);
        fb.parts
            .into_iter()
            .find(|p| p.token.token.value == value)
            .expect("expected to find the requested token in the formatted buffer")
    }

    // ── FormattedBufferPart::get_spans ────────────────────────────────────

    /// Convenience: collect just the rendered text and per-grapheme flags.
    fn get_spans_simple(
        part: &FormattedBufferPart,
        selection_range: Option<std::ops::Range<usize>>,
    ) -> Vec<(String, bool, bool, bool)> {
        part.get_spans(None, selection_range)
            .into_iter()
            .map(|(s, _tags, c, sb, sel)| (s.content.into_owned(), c, sb, sel))
            .collect()
    }

    #[test]
    fn get_spans_no_cursor_no_selection_byte_returns_single_tuple() {
        let part = first_word_part("hello", "hello");
        // No cursor, no selection_byte, no selection range.
        let spans = get_spans_simple(&part, None);
        assert_eq!(spans, vec![("hello".to_string(), false, false, false)]);
    }

    #[test]
    fn get_spans_part_fully_inside_selection_marks_single_tuple_selected() {
        // "hello" is fully inside selection range [0..5).
        let part = first_word_part("hello world", "hello");
        let spans = get_spans_simple(&part, Some(0..5));
        assert_eq!(spans, vec![("hello".to_string(), false, false, true)]);
    }

    #[test]
    fn get_spans_part_outside_selection_marks_single_tuple_not_selected() {
        let part = first_word_part("hello world", "world");
        let spans = get_spans_simple(&part, Some(0..5));
        assert_eq!(spans, vec![("world".to_string(), false, false, false)]);
    }

    #[test]
    fn get_spans_with_cursor_yields_per_grapheme() {
        // Cursor at byte 2 inside "hello".
        let fb = FormattedBuffer::from("hello", 2, None);
        let part = fb
            .parts
            .into_iter()
            .find(|p| p.token.token.value == "hello")
            .unwrap();
        let spans = get_spans_simple(&part, None);
        assert_eq!(
            spans,
            vec![
                ("h".to_string(), false, false, false),
                ("e".to_string(), false, false, false),
                ("l".to_string(), true, false, false),
                ("l".to_string(), false, false, false),
                ("o".to_string(), false, false, false),
            ]
        );
    }

    #[test]
    fn get_spans_with_selection_byte_yields_per_grapheme() {
        // Selection from byte 1 to byte 4 inside "hello", cursor at 4.
        // Tokens: "hello" (0..5).
        let fb = FormattedBuffer::from("hello", 4, Some(1));
        let part = fb
            .parts
            .into_iter()
            .find(|p| p.token.token.value == "hello")
            .unwrap();
        // selection range is 1..4 — so graphemes at byte 1, 2, 3 are selected.
        let spans = get_spans_simple(&part, Some(1..4));
        assert_eq!(
            spans,
            vec![
                ("h".to_string(), false, false, false),
                ("e".to_string(), false, true, true),
                ("l".to_string(), false, false, true),
                ("l".to_string(), false, false, true),
                ("o".to_string(), true, false, false),
            ]
        );
    }

    #[test]
    fn get_spans_tags_track_byte_offset_in_buffer() {
        // The "world" token starts at byte 6 in "hello world". When we ask
        // for per-grapheme tuples (cursor inside the token), each tag must
        // reflect the absolute byte offset.
        let fb = FormattedBuffer::from("hello world", 7, None);
        let part = fb
            .parts
            .into_iter()
            .find(|p| p.token.token.value == "world")
            .unwrap();
        let tuples = part.get_spans(None, None);
        let tags: Vec<Tag> = tuples.into_iter().flat_map(|(_, t, _, _, _)| t).collect();
        assert_eq!(
            tags,
            vec![
                Tag::Command(6),
                Tag::Command(7),
                Tag::Command(8),
                Tag::Command(9),
                Tag::Command(10),
            ]
        );
    }

    #[test]
    fn get_spans_single_tuple_tags_track_byte_offset() {
        // No cursor, no selection_byte: a single tuple, but tags should
        // still hold the absolute per-grapheme byte offsets.
        let fb = FormattedBuffer::from("hello world", 11, None);
        let part = fb
            .parts
            .into_iter()
            .find(|p| p.token.token.value == "hello")
            .unwrap();
        let tuples = part.get_spans(None, None);
        assert_eq!(tuples.len(), 1);
        assert_eq!(
            tuples[0].1,
            vec![
                Tag::Command(0),
                Tag::Command(1),
                Tag::Command(2),
                Tag::Command(3),
                Tag::Command(4),
            ]
        );
    }

    // ── format_buffer selection bookkeeping ───────────────────────────────

    #[test]
    fn format_buffer_records_selection_byte_grapheme_idx() {
        // selection_byte at byte 8 lies inside "world" (6..11), so its
        // grapheme idx within that token is 8 - 6 = 2.
        let fb = FormattedBuffer::from("hello world", 0, Some(8));
        let world = fb
            .parts
            .iter()
            .find(|p| p.token.token.value == "world")
            .unwrap();
        assert_eq!(world.selection_byte_grapheme_idx, Some(2));
    }

    #[test]
    fn format_buffer_no_selection_means_no_selection_grapheme_idx() {
        let fb = FormattedBuffer::from("hello world", 0, None);
        assert!(
            fb.parts
                .iter()
                .all(|p| p.selection_byte_grapheme_idx.is_none())
        );
    }
}
