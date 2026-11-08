use std::borrow::Cow;
use std::vec;

use crate::content::{SnakeAnimation, Tag};
use crate::grammar::{
    AnnotatedToken, ClosingAnnotation, OptionKind, ToInclusiveRange, TokenKind,
    dequoting_function_rust,
};
use crate::palette::Palette;
use crate::shell;
use itertools::{EitherOrBoth, Itertools};
use ratatui::prelude::*;
use std::sync::{Arc, Mutex, OnceLock};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
            true,
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

fn is_bash_reserved_token_kind(kind: &TokenKind) -> bool {
    matches!(
        kind,
        // Control flow keywords
        TokenKind::If
            | TokenKind::Then
            | TokenKind::Elif
            | TokenKind::Else
            | TokenKind::Fi
            | TokenKind::Case
            | TokenKind::Esac
            | TokenKind::For
            | TokenKind::While
            | TokenKind::Until
            | TokenKind::Do
            | TokenKind::Done
            | TokenKind::In
            | TokenKind::Select
            | TokenKind::Function
            // Other keywords
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Return
            | TokenKind::Export
            // Operators and separators
            | TokenKind::And        // &&
            | TokenKind::Or         // ||
            | TokenKind::Pipe       // |
            | TokenKind::Semicolon  // ;
            | TokenKind::DoubleSemicolon // ;;
            | TokenKind::Assignment // =
            | TokenKind::Background // &
            | TokenKind::Less       // <
            | TokenKind::Great      // >
            | TokenKind::DGreat     // >>
            | TokenKind::InputDup   // <&
            | TokenKind::OutputDup  // >&
            | TokenKind::ReadWrite  // <>
            | TokenKind::Clobber    // >|
            // History expansion token and complete builtin (explicitly requested)
            | TokenKind::History    // ! - history expansion
            | TokenKind::Complete // complete - tab completion builtin
    )
}

fn token_to_style(
    token: &AnnotatedToken,
    recognised_command: Option<bool>,
    recognised_env_var: Option<bool>,
    cursor_on_this_or_closing_token: bool,
    palette: &Palette,
) -> Style {
    if cursor_on_this_or_closing_token {
        return palette.opening_and_closing_pair();
    }

    // Env var coloring has the highest priority among base colors: a token can have both
    // `is_env_var` and `is_inside_double_quotes` (e.g. `$HOME` in `"$HOME"`), and the env var
    // color should win over the double-quoted color. It also wins over the rainbow colour of
    // the `${` and `}` of `${HOME}`, which are part of the variable.
    if token.annotations.is_env_var {
        if recognised_env_var == Some(false) {
            return palette.unrecognised_env_var();
        }
        return palette.env_var();
    }

    // Rainbow-colour all opening and closing delimiter tokens by nesting depth.
    if let Some(depth) = token.annotations.bracket_depth {
        return palette.rainbow_bracket(depth);
    }

    if token.annotations.command_word.is_some() {
        if recognised_command == Some(true) {
            return palette.recognised_command();
        }
        return palette.unrecognised_command();
    }

    match token.annotations.option {
        Some(OptionKind::Short) => return palette.short_option(),
        Some(OptionKind::Long) => return palette.long_option(),
        None => {}
    }

    if token.annotations.is_path_candidate && names_existing_path(&token.token.value) {
        return palette.path();
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

    if is_bash_reserved_token_kind(&token.token.kind) {
        return palette.bash_reserved();
    }

    palette.normal_text()
}

/// The path that `word` names, for a word with nothing to expand but a leading `~` (see
/// `Annotations::is_path_candidate`). Like bash, this expands a leading `~` (`$HOME`), `~+`
/// (`$PWD`), `~-` (`$OLDPWD`) or `~user` up to the first `/`, and removes the backslashes that
/// escape characters. A relative path stays relative.
fn word_to_path(word: &str) -> String {
    if let Some(after_tilde) = word.strip_prefix('~') {
        let (prefix, rest) =
            after_tilde.split_at(after_tilde.find('/').unwrap_or(after_tilde.len()));
        let home = match prefix {
            "" => shell::backend().env_var("HOME"),
            "+" => shell::backend().env_var("PWD"),
            "-" => shell::backend().env_var("OLDPWD"),
            user => crate::users::get_all_users()
                .iter()
                .find(|u| u.username == user)
                .map(|u| u.home_dir.clone()),
        };
        if let Some(home) = home {
            return home + &dequoting_function_rust(rest);
        }
    }
    dequoting_function_rust(word)
}

/// Whether `word` names an existing file or directory, resolving a relative path against the
/// shell's working directory. A symlink exists even when its target does not.
fn names_existing_path(word: &str) -> bool {
    let path = std::path::Path::new(&shell::backend().cwd()).join(word_to_path(word));
    std::fs::symlink_metadata(path).is_ok()
}

/// Split `token` at its escape sequences (see `Annotations::escape_sequences`) so that each
/// one can be styled on its own. Returns the pieces in order, each with whether it is an
/// escape sequence. A token without escape sequences is returned whole.
fn split_at_escape_sequences(token: &AnnotatedToken) -> Vec<(Cow<'_, AnnotatedToken>, bool)> {
    let escape_sequences = &token.annotations.escape_sequences;
    if escape_sequences.is_empty() {
        return vec![(Cow::Borrowed(token), false)];
    }

    let value = &token.token.value;
    let piece = |range: std::ops::Range<usize>, is_escape_sequence: bool| {
        let mut piece = token.clone();
        let text = value[range.clone()].to_string();
        piece.token.position.byte += range.start;
        piece.token.position.column += value[..range.start].chars().count();
        piece.token.kind = TokenKind::Word(text.clone());
        // Keep the escape sequence a piece holds, relative to the start of the piece.
        piece.annotations.escape_sequences = escape_sequences
            .iter()
            .filter(|escape| range.start <= escape.start && escape.end <= range.end)
            .map(|escape| escape.start - range.start..escape.end - range.start)
            .collect();
        piece.token.value = text;
        (Cow::Owned(piece), is_escape_sequence)
    };

    let mut pieces = Vec::new();
    let mut end_of_last = 0;
    for escape_sequence in escape_sequences {
        if end_of_last < escape_sequence.start {
            pieces.push(piece(end_of_last..escape_sequence.start, false));
        }
        pieces.push(piece(escape_sequence.clone(), true));
        end_of_last = escape_sequence.end;
    }
    if end_of_last < value.len() {
        pieces.push(piece(end_of_last..value.len(), false));
    }
    pieces
}

#[derive(Debug)]
struct WordInfo {
    pub tooltip: Option<String>,
    pub is_recognised_command: bool,
}

fn get_word_info(token: &AnnotatedToken) -> Option<WordInfo> {
    if token.annotations.is_env_var && token.token.kind.is_word() {
        let env_var_name = &token.token.value;

        let tooltip = shell::backend().format_var(env_var_name);

        return Some(WordInfo {
            tooltip: Some(tooltip),
            is_recognised_command: false,
        });
    } else if let Some(value) = &token.annotations.command_word {
        let command_info = shell::backend().command_info(value);
        return Some(WordInfo {
            tooltip: Some(command_info.to_description()),
            is_recognised_command: command_info.is_known(),
        });
    }
    None
}

impl FormattedBufferPart {
    pub fn new(
        token: &AnnotatedToken,
        cursor_on_this_or_closing_token: bool,
        cursor_byte_pos_in_token: Option<usize>,
        selection_byte_pos_in_token: Option<usize>,
        palette: &Palette,
        recognised_env_var: Option<bool>,
        enable_easter_eggs: bool,
    ) -> Self {
        let word_info = get_word_info(token);
        let tooltip = word_info.as_ref().and_then(|info| info.tooltip.clone());
        let recognised_command = word_info.as_ref().map(|info| info.is_recognised_command);

        let style = token_to_style(
            token,
            recognised_command,
            recognised_env_var,
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
        > = if enable_easter_eggs
            && token.annotations.command_word.is_some()
            && token.token.value.starts_with("python")
        {
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
    enable_easter_eggs: bool,
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

    let mut env_var_recognised = vec![false; annotated_tokens.len()];
    for idx in 0..annotated_tokens.len() {
        let tok = &annotated_tokens[idx];
        if tok.annotations.is_env_var && tok.token.kind.is_word() {
            let name = &tok.token.value;
            if shell::backend().env_var(name).is_some() {
                env_var_recognised[idx] = true;
                if idx > 0 && annotated_tokens[idx - 1].token.kind == TokenKind::Dollar {
                    env_var_recognised[idx - 1] = true;
                }
                // The `${` and `}` of `${NAME}`, when they are part of the variable.
                for delimiter_idx in [idx.wrapping_sub(1), idx + 1] {
                    if annotated_tokens
                        .get(delimiter_idx)
                        .is_some_and(|delimiter| {
                            delimiter.annotations.is_env_var
                                && matches!(
                                    delimiter.token.kind,
                                    TokenKind::ParamExpansion | TokenKind::RBrace
                                )
                        })
                    {
                        env_var_recognised[delimiter_idx] = true;
                    }
                }
            }
        }
    }

    let spans: Vec<FormattedBufferPart> = annotated_tokens
        .iter()
        .enumerate()
        .flat_map(|(idx, tok)| {
            let highlight = app_is_running
                && (strict_highlight[idx] || (use_inclusive && inclusive_highlight[idx]));
            let recognised_env_var = if tok.annotations.is_env_var {
                Some(env_var_recognised[idx])
            } else {
                None
            };
            // The escape sequences of a string become parts of their own, so they can be
            // styled differently from the rest of the string.
            split_at_escape_sequences(tok)
                .into_iter()
                .map(move |(piece, is_escape_sequence)| {
                    let cursor_pos_in_token = if piece.token.byte_range().contains(&cursor_byte_pos)
                    {
                        Some(cursor_byte_pos - piece.token.byte_range().start)
                    } else {
                        None
                    };
                    let selection_pos_in_token = selection_byte_pos.and_then(|s| {
                        if piece.token.byte_range().contains(&s) {
                            Some(s - piece.token.byte_range().start)
                        } else {
                            None
                        }
                    });
                    let mut part = FormattedBufferPart::new(
                        &piece,
                        highlight,
                        cursor_pos_in_token,
                        selection_pos_in_token,
                        palette,
                        recognised_env_var,
                        enable_easter_eggs,
                    );
                    if is_escape_sequence {
                        part.span.style = palette.escape_sequence();
                    }
                    part
                })
        })
        .collect();

    // if log::log_enabled!(log::Level::Trace) {
    //     for part in &spans {
    //         log::trace!(
    //             "Token: '{:?}', byte range: {:?}, cursor_grapheme_idx: {:?}, selection_byte_grapheme_idx: {:?}",
    //             part.token.token,
    //             part.token.token.byte_range(),
    //             part.cursor_grapheme_idx,
    //             part.selection_byte_grapheme_idx
    //         );
    //     }
    // }

    FormattedBuffer {
        parts: spans,
        draw_cursor_at_end: cursor_byte_pos >= buffer_byte_length,
    }
}

pub fn format_agent_buffer(
    annotated_tokens: &[AnnotatedToken],
    cursor_byte_pos: usize,
    selection_byte_pos: Option<usize>,
    buffer_byte_length: usize,
    palette: &Palette,
    enable_easter_eggs: bool,
) -> FormattedBuffer {
    let mut found_first_word = false;
    let spans: Vec<FormattedBufferPart> = annotated_tokens
        .iter()
        .map(|tok| {
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
            let mut part = FormattedBufferPart::new(
                tok,
                false,
                cursor_pos_in_token,
                selection_pos_in_token,
                palette,
                None,
                enable_easter_eggs,
            );

            if tok.token.kind.is_word() && !found_first_word {
                part.span.style = palette.recognised_command();
                found_first_word = true;
            } else {
                part.span.style = palette.normal_text();
            }
            part
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
    use crate::palette::PaletteStyleKind;

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

    // ── rainbow bracket colouring ─────────────────────────────────────────

    /// Opening and closing bracket tokens are coloured with rainbow palette
    /// colours, cycling by nesting depth.
    #[test]
    fn rainbow_brackets_use_depth_based_style() {
        // `echo $(echo $(true))` — outer $( depth 0, inner $( depth 1.
        let input = "echo $(echo $(true))";
        let cursor = input.len();
        let fb = FormattedBuffer::from(input, cursor, None);

        let palette = Palette::dark();

        // Collect all parts that have a bracket_depth annotation.
        let bracket_parts: Vec<&FormattedBufferPart> = fb
            .parts
            .iter()
            .filter(|p| p.token.annotations.bracket_depth.is_some())
            .collect();

        assert!(!bracket_parts.is_empty(), "expected bracket parts");

        for part in bracket_parts {
            let depth = part.token.annotations.bracket_depth.unwrap();
            let expected = palette.rainbow_bracket(depth);
            assert_eq!(
                part.normal_span().style,
                expected,
                "expected rainbow style at depth {} for token '{}'",
                depth,
                part.token.token.value
            );
        }
    }

    /// Depth cycles modulo 4: depth 4 is styled the same as depth 0.
    #[test]
    fn rainbow_bracket_style_cycles_modulo_4() {
        let palette = Palette::dark();
        assert_eq!(palette.rainbow_bracket(0), palette.rainbow_bracket(4));
        assert_eq!(palette.rainbow_bracket(1), palette.rainbow_bracket(5));
        assert_eq!(palette.rainbow_bracket(2), palette.rainbow_bracket(6));
        assert_eq!(palette.rainbow_bracket(3), palette.rainbow_bracket(7));
    }

    #[test]
    fn test_recognised_vs_unrecognised_env_var_styling() {
        let palette = Palette::dark();
        let input = "echo $HOME $UNRECOGNISED_VAR_XYZ";
        let fb = FormattedBuffer::from(input, input.len(), None);

        let dollar_signs = parts_with_value(&fb, "$");
        assert_eq!(dollar_signs.len(), 2);

        let home_parts = parts_with_value(&fb, "HOME");
        assert_eq!(home_parts.len(), 1);

        let unrec_parts = parts_with_value(&fb, "UNRECOGNISED_VAR_XYZ");
        assert_eq!(unrec_parts.len(), 1);

        assert_eq!(home_parts[0].normal_span().style, palette.env_var());
        assert_eq!(dollar_signs[0].normal_span().style, palette.env_var());

        assert_eq!(
            unrec_parts[0].normal_span().style,
            palette.unrecognised_env_var()
        );
        assert_eq!(
            dollar_signs[1].normal_span().style,
            palette.unrecognised_env_var()
        );
    }

    #[test]
    fn test_brace_delimited_env_var_styling() {
        let palette = Palette::dark();
        let input = "echo ${HOME} ${UNRECOGNISED_VAR_XYZ}";
        let fb = FormattedBuffer::from(input, input.len(), None);

        let home_parts = parts_with_value(&fb, "HOME");
        assert_eq!(home_parts.len(), 1);
        assert_eq!(home_parts[0].normal_span().style, palette.env_var());

        let unrec_parts = parts_with_value(&fb, "UNRECOGNISED_VAR_XYZ");
        assert_eq!(unrec_parts.len(), 1);
        assert_eq!(
            unrec_parts[0].normal_span().style,
            palette.unrecognised_env_var()
        );

        // The `${` and `}` take the colour of the variable they delimit.
        let delimiter_styles: Vec<_> = fb
            .parts
            .iter()
            .filter(|p| {
                matches!(
                    p.token.token.kind,
                    TokenKind::ParamExpansion | TokenKind::RBrace
                )
            })
            .map(|p| p.normal_span().style)
            .collect();
        assert_eq!(
            delimiter_styles,
            vec![
                palette.env_var(),
                palette.env_var(),
                palette.unrecognised_env_var(),
                palette.unrecognised_env_var(),
            ]
        );
    }

    #[test]
    fn test_brace_delimiters_around_more_than_a_name_stay_brackets() {
        let palette = Palette::dark();
        let input = "echo ${HOME:-/tmp}";
        let fb = FormattedBuffer::from(input, input.len(), None);

        for part in fb.parts.iter().filter(|p| {
            matches!(
                p.token.token.kind,
                TokenKind::ParamExpansion | TokenKind::RBrace
            )
        }) {
            assert_eq!(part.normal_span().style, palette.rainbow_bracket(0));
        }
    }

    // ── options and escape sequences ──────────────────────────────────────

    fn format_with_palette(input: &str, palette: &Palette) -> FormattedBuffer {
        let mut parser = crate::dparser::DParser::from(input);
        parser.walk_to_end();
        format_buffer(
            parser.tokens(),
            input.len(),
            None,
            input.len(),
            false,
            palette,
            true,
        )
    }

    /// The dark palette with its own option and escape-sequence styles, as after
    /// `flyline set-style short-option=blue long-option="bold bright_blue" escape-sequence=bold`.
    fn palette_with_option_and_escape_styles() -> Palette {
        let mut palette = Palette::dark();
        palette.set(
            PaletteStyleKind::ShortOption,
            Style::default().fg(Color::Blue),
        );
        palette.set(
            PaletteStyleKind::LongOption,
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        );
        palette.set(
            PaletteStyleKind::EscapeSequence,
            Style::default().add_modifier(Modifier::BOLD),
        );
        palette
    }

    #[test]
    fn test_option_styling() {
        let palette = palette_with_option_and_escape_styles();
        let fb = format_with_palette("ls -la --color=auto - -- -x", &palette);
        let style_of = |value: &str| parts_with_value(&fb, value)[0].normal_span().style;

        assert_eq!(style_of("-la"), Style::default().fg(Color::Blue));
        assert_eq!(
            style_of("--color=auto"),
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD)
        );

        // `-` (standard input), `--` and the operands after it are not options.
        assert_eq!(style_of("-"), palette.normal_text());
        assert_eq!(style_of("--"), palette.normal_text());
        assert_eq!(style_of("-x"), palette.normal_text());

        let mut palette = palette;
        palette.set(
            PaletteStyleKind::NormalText,
            Style::default().add_modifier(Modifier::ITALIC),
        );
        let fb = format_with_palette("ls -la", &palette);
        assert_eq!(
            parts_with_value(&fb, "-la")[0].normal_span().style,
            Style::default().fg(Color::Blue)
        );
    }

    #[test]
    fn test_option_and_escape_sequence_defaults() {
        let input = r#"ls -la --all "a\"b" $'c\nd'"#;
        let red = Style::default().fg(Color::Red);
        for palette in [Palette::dark(), Palette::light()] {
            let fb = format_with_palette(input, &palette);
            let style_of = |value: &str| parts_with_value(&fb, value)[0].normal_span().style;

            assert_eq!(style_of("-la"), palette.normal_text());
            assert_eq!(style_of("--all"), palette.normal_text());
            assert_eq!(style_of("a"), palette.double_quoted_text());
            assert_eq!(style_of(r#"\""#), red);
            assert_eq!(style_of("c"), palette.single_quoted_text());
            assert_eq!(style_of(r"\n"), red);
        }
    }

    #[test]
    fn test_escape_sequences_in_double_quoted_strings() {
        let palette = palette_with_option_and_escape_styles();
        let input = r#"echo "Price: \$5, a \"quote\", a backslash \\, a newline \n""#;
        let fb = format_with_palette(input, &palette);

        // `\n` is not an escape sequence inside double quotes: bash keeps the backslash.
        let text = palette.double_quoted_text();
        let escape = palette.escape_sequence();
        let parts: Vec<(&str, Style)> = fb
            .parts
            .iter()
            .map(|p| (p.token.token.value.as_str(), p.normal_span().style))
            .collect();
        assert_eq!(
            parts[3..12],
            [
                ("Price: ", text),
                (r"\$", escape),
                ("5, a ", text),
                (r#"\""#, escape),
                ("quote", text),
                (r#"\""#, escape),
                (", a backslash ", text),
                (r"\\", escape),
                (r", a newline \n", text),
            ]
        );
        assert_eq!(parts[2].0, "\"");
        assert_eq!(parts[12].0, "\"");
        assert_eq!(parts.len(), 13);
    }

    #[test]
    fn test_escape_sequences_in_ansi_c_strings() {
        let palette = palette_with_option_and_escape_styles();
        let input = "echo $'Line one\\nLine two\\tTabbed \\x41 \\u00e9'";
        let fb = format_with_palette(input, &palette);

        let text = palette.single_quoted_text();
        let escape = palette.escape_sequence();
        let string_parts: Vec<(&str, Style)> = fb
            .parts
            .iter()
            .filter(|p| p.token.annotations.is_inside_single_quotes)
            .map(|p| (p.token.token.value.as_str(), p.normal_span().style))
            .collect();
        assert_eq!(
            string_parts,
            vec![
                ("Line one", text),
                (r"\n", escape),
                ("Line two", text),
                (r"\t", escape),
                ("Tabbed ", text),
                (r"\x41", escape),
                (" ", text),
                ("\\u00e9", escape),
            ]
        );

        // Without the `$`, a backslash in single quotes is plain text.
        let fb = format_with_palette(r"echo 'a\nb'", &palette);
        assert_eq!(parts_with_value(&fb, r"a\nb")[0].normal_span().style, text);
    }

    #[test]
    fn test_escape_sequence_parts_keep_their_place_in_the_buffer() {
        // The `\"` is at bytes 7..9; the cursor is on its `"` and the selection starts at its `\`.
        let input = r#"echo "a\"b""#;
        let fb = FormattedBuffer::from(input, 8, Some(7));

        let joined: String = fb
            .parts
            .iter()
            .map(|p| p.token.token.value.as_str())
            .collect();
        assert_eq!(joined, input);

        let escape = fb.get_part_from_byte_pos(8).unwrap();
        assert_eq!(escape.token.token.value, r#"\""#);
        assert_eq!(escape.token.token.byte_range(), 7..9);
        assert_eq!(escape.cursor_grapheme_idx, Some(1));
        assert_eq!(escape.selection_byte_grapheme_idx, Some(0));
        let tags: Vec<Tag> = escape
            .get_spans(None, None)
            .into_iter()
            .flat_map(|(_, tags, _, _, _)| tags)
            .collect();
        assert_eq!(tags, vec![Tag::Command(7), Tag::Command(8)]);

        let before = parts_with_value(&fb, "a")[0];
        assert_eq!(before.token.token.byte_range(), 6..7);
        assert_eq!(before.cursor_grapheme_idx, None);
        let after = parts_with_value(&fb, "b")[0];
        assert_eq!(after.token.token.byte_range(), 9..10);
        assert_eq!(after.cursor_grapheme_idx, None);
    }

    // ── paths ─────────────────────────────────────────────────────────────

    /// A new directory under the system temp directory, holding a file `notes.txt` and a
    /// directory `my dir`.
    fn temp_dir_with_files() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("flyline_paths_{}", rand::random::<u32>()));
        std::fs::create_dir_all(dir.join("my dir")).unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        dir
    }

    #[test]
    fn test_existing_paths_get_the_path_style() {
        let palette = Palette::dark();
        let dir = temp_dir_with_files();
        let dir = dir.to_str().unwrap();
        let notes = format!("{dir}/notes.txt");
        let my_dir = format!(r"{dir}/my\ dir");
        let missing = format!("{dir}/missing.txt");

        let fb = format_with_palette(&format!("ls {notes} {my_dir} {missing}"), &palette);
        let style_of = |value: &str| parts_with_value(&fb, value)[0].normal_span().style;
        assert_eq!(style_of(&notes), palette.path());
        assert_eq!(style_of(&my_dir), palette.path());
        // A path that does not exist is normal text.
        assert_eq!(style_of(&missing), palette.normal_text());

        // The target of a redirection is a path too.
        let fb = format_with_palette(&format!("echo hi >{notes}"), &palette);
        assert_eq!(
            parts_with_value(&fb, &notes)[0].normal_span().style,
            palette.path()
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_relative_paths_and_tilde() {
        // Tests run in the crate root, which the test backend reports as the working directory.
        let palette = Palette::dark();
        let fb = format_with_palette(
            "ls . .. src src/lib.rs ~+ ~+/src no_such_file ~+/no_such_file",
            &palette,
        );
        let style_of = |value: &str| parts_with_value(&fb, value)[0].normal_span().style;
        for existing in [".", "..", "src", "src/lib.rs", "~+", "~+/src"] {
            assert_eq!(style_of(existing), palette.path());
        }
        assert_eq!(style_of("no_such_file"), palette.normal_text());
        assert_eq!(style_of("~+/no_such_file"), palette.normal_text());
    }

    #[test]
    fn test_only_argument_words_get_the_path_style() {
        // Every word names the existing `README.md` (or matches it), but only the argument
        // `README.md` gets the path style.
        let palette = Palette::dark();
        let fb = format_with_palette(
            r#"README.md README.md "README.md" 'README.md' README.m?"#,
            &palette,
        );
        let styles: Vec<Style> = fb
            .parts
            .iter()
            .filter(|p| p.token.token.value.starts_with("README.m"))
            .map(|p| p.normal_span().style)
            .collect();
        assert_eq!(
            styles,
            vec![
                // The command word,
                palette.unrecognised_command(),
                // an argument,
                palette.path(),
                // quoted words,
                palette.double_quoted_text(),
                palette.single_quoted_text(),
                // and a glob.
                palette.normal_text(),
            ]
        );
    }

    #[test]
    fn test_word_to_path() {
        let home = shell::backend().env_var("HOME").unwrap();
        let cwd = shell::backend().cwd();

        assert_eq!(word_to_path("~"), home);
        assert_eq!(word_to_path("~/notes"), format!("{home}/notes"));
        assert_eq!(word_to_path("~+/src"), format!("{cwd}/src"));
        assert_eq!(word_to_path("../tmp"), "../tmp");
        // A backslash escapes the next character, so an escaped `~` is not expanded.
        assert_eq!(word_to_path(r"My\ Documents"), "My Documents");
        assert_eq!(word_to_path(r"~/my\ dir"), format!("{home}/my dir"));
        assert_eq!(word_to_path(r"\~/notes"), "~/notes");
        // `~name` of a user that does not exist stays as it is.
        assert_eq!(
            word_to_path("~no_such_user_xyz/notes"),
            "~no_such_user_xyz/notes"
        );
    }

    #[test]
    fn test_enable_easter_eggs_toggle() {
        let input = "python3";
        let mut parser = crate::dparser::DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens().to_vec();

        let fb_enabled = format_buffer(
            &tokens,
            input.len(),
            None,
            input.len(),
            false,
            &Palette::dark(),
            true,
        );
        assert!(
            fb_enabled
                .parts
                .iter()
                .any(|p| p.animated_span_fn.is_some())
        );

        let fb_disabled = format_buffer(
            &tokens,
            input.len(),
            None,
            input.len(),
            false,
            &Palette::dark(),
            false,
        );
        assert!(
            fb_disabled
                .parts
                .iter()
                .all(|p| p.animated_span_fn.is_none())
        );
    }

    #[test]
    fn test_autocd_directory_recognised_as_command() {
        let temp_dir = std::env::temp_dir();
        let dir_path = temp_dir.to_str().unwrap();

        let input = dir_path;
        let mut parser = crate::dparser::DParser::from(input);
        parser.walk_to_end();
        let tokens = parser.tokens().to_vec();

        // When autocd is disabled (default in tests), dir path is unrecognised command
        shell::set_test_autocd(false);
        let info = get_word_info(&tokens[0]).unwrap();
        assert!(!info.is_recognised_command);

        // When autocd is enabled, dir path is recognised as command
        shell::set_test_autocd(true);
        let info = get_word_info(&tokens[0]).unwrap();
        assert!(info.is_recognised_command);
        assert_eq!(info.tooltip, Some(dir_path.to_string()));

        shell::set_test_autocd(false);
    }
}
