//! Display-only projection of a selected completion onto the command buffer.

use super::formatted_buffer::FormattedBuffer;
use super::{App, ContentMode};
use crate::active_suggestions::ProcessedSuggestion;
use crate::content::Tag;
use crate::grammar::TokenKind;
use ratatui::prelude::*;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// One span of real input and its corresponding span in the displayed candidate.
/// The same mapping drives emphasis, cursor placement and mouse hit-testing.
struct SourceMapping {
    source: Range<usize>,
    display: Range<usize>,
}

pub(super) struct CompletionPreview {
    range: Range<usize>,
    text: String,
    cursor: usize,
    cursor_in_preview: usize,
    mapping: Vec<SourceMapping>,
}

impl App<'_> {
    /// Shared by rendering and key contexts so hidden history suggestions can
    /// never override the completion currently shown in the command line.
    pub(super) fn completion_preview(&self) -> Option<CompletionPreview> {
        if !self.mode.is_running() || !crate::settings().preview {
            return None;
        }
        let ContentMode::TabCompletion(active) = &self.content_mode else {
            return None;
        };
        let (suggestion, matches) = active.selected_suggestion()?;
        CompletionPreview::new(&self.buffer, &active.word_under_cursor, suggestion, matches)
    }
}

impl CompletionPreview {
    pub(super) fn new(
        buffer: &flybuffer::TextBuffer,
        target: &flybuffer::SubString,
        suggestion: &ProcessedSuggestion,
        matching_indices: &[usize],
    ) -> Option<Self> {
        let text = suggestion.formatted();
        let end = target.start.checked_add(target.s.len())?;
        let cursor = buffer.cursor_byte_pos();
        if buffer.selection_byte().is_some()
            || !(target.start..=end).contains(&cursor)
            || buffer.buffer().get(target.start..end)? != target.s
            || text.is_empty()
        {
            return None;
        }
        let prefix_len = if target.s.starts_with(&suggestion.prefix) {
            suggestion.prefix.len()
        } else {
            0
        };
        // Preserve only a real closing quote, never a separator space which
        // may instead be an escaped character belonging to the filename.
        let closing_quote = suggestion
            .suffix
            .chars()
            .next()
            .filter(|q| matches!(q, '\'' | '"'))
            .filter(|q| {
                target.s.starts_with(*q)
                    && target.s.ends_with(*q)
                    && target.s.len() > prefix_len
                    && target.s.len() > 1
            });
        let body_end = target.s.len() - closing_quote.map_or(0, char::len_utf8);
        let display_body: Vec<_> = suggestion.s.char_indices().collect();
        // Align case-insensitive prefixes in lowercase coordinates: one input
        // character may correspond to several candidate characters or vice versa.
        let expand_case = !matching_indices.is_empty()
            && matching_indices
                .iter()
                .copied()
                .eq(0..matching_indices.len())
            && suggestion
                .s
                .to_lowercase()
                .starts_with(&target.s[prefix_len..body_end].to_lowercase());
        let mut matches = matching_indices
            .iter()
            .filter_map(|&index| display_body.get(index))
            .flat_map(|&(byte, ch)| {
                std::iter::repeat_n(
                    (byte, ch),
                    if expand_case {
                        ch.to_lowercase().count()
                    } else {
                        1
                    },
                )
            });
        // With no match metadata (e.g. globs), map only a shared literal prefix.
        let literal_end = if matching_indices.is_empty() {
            target
                .s
                .chars()
                .zip(text.chars())
                .take_while(|(a, b)| a == b)
                .map(|(ch, _)| ch.len_utf8())
                .sum()
        } else {
            0
        };
        let mapping: Vec<_> = target
            .s
            .char_indices()
            .flat_map(|(source, ch)| {
                std::iter::repeat_n(
                    (source, ch),
                    if expand_case && (prefix_len..body_end).contains(&source) {
                        ch.to_lowercase().count()
                    } else {
                        1
                    },
                )
            })
            .filter_map(|(source, ch)| {
                let displayed = if source < prefix_len {
                    Some((source, ch))
                } else if closing_quote.is_some() && source == body_end {
                    Some((suggestion.prefix.len() + suggestion.s.len(), ch))
                } else if matching_indices.is_empty() {
                    (source < literal_end).then_some((source, ch))
                } else {
                    matches
                        .next()
                        .map(|(byte, matched)| (suggestion.prefix.len() + byte, matched))
                }?;
                Some(SourceMapping {
                    source: source..source + ch.len_utf8(),
                    display: displayed.0..displayed.0 + displayed.1.len_utf8(),
                })
            })
            .collect();
        let mapped_cursor = mapping
            .iter()
            .rev()
            .find(|entry| entry.source.end <= cursor - target.start)
            .map_or(0, |entry| entry.display.end);
        // Keep the cursor outside combining-character/emoji graphemes.
        let cursor_in_preview = if mapped_cursor == 0 {
            0
        } else {
            text.grapheme_indices(true)
                .map(|(byte, graph)| byte + graph.len())
                .find(|end| *end >= mapped_cursor)
                .unwrap_or(text.len())
        };
        Some(Self {
            range: target.start..end,
            text,
            cursor,
            cursor_in_preview,
            mapping,
        })
    }
}

pub(super) struct DrawableBufferSpan {
    pub span: Span<'static>,
    pub tags: Vec<Tag>,
    pub is_cursor: bool,
    pub is_in_selection: bool,
    pub newline: bool,
}

impl FormattedBuffer {
    /// Splice a preview into the rendered spans, leaving text, undo history,
    /// syntax tokens and mouse offsets in the editor's real buffer untouched.
    pub(super) fn drawable_spans(
        &self,
        animation_time: Option<std::time::Instant>,
        selection_range: Option<std::ops::Range<usize>>,
        preview: Option<&CompletionPreview>,
        preview_style: Style,
        matched_style: Style,
    ) -> Vec<DrawableBufferSpan> {
        let mut out = Vec::new();
        let mut inserted = false;
        let mut cursor_after_preview = false;
        for part in &self.parts {
            for (span, tags, is_cursor, _, is_in_selection) in
                part.get_spans(animation_time, selection_range.clone())
            {
                let Some(preview) = preview else {
                    out.push(DrawableBufferSpan {
                        span,
                        tags,
                        is_cursor,
                        is_in_selection,
                        newline: part.token.token.kind == TokenKind::Newline,
                    });
                    continue;
                };
                // Splitting at grapheme boundaries also handles targets spanning
                // several syntax tokens (quotes, escapes and fuzzy matches).
                for (graph, tag) in span.content.graphemes(true).zip(tags) {
                    let Tag::Command(byte) = tag else { continue };
                    if !inserted && byte >= preview.range.start {
                        cursor_after_preview =
                            Self::push_preview(&mut out, preview, preview_style, matched_style);
                        inserted = true;
                    }
                    if preview.range.contains(&byte) {
                        continue;
                    }
                    out.push(DrawableBufferSpan {
                        span: Span::styled(graph.to_owned(), span.style),
                        tags: vec![tag],
                        is_cursor: std::mem::take(&mut cursor_after_preview),
                        is_in_selection,
                        newline: part.token.token.kind == TokenKind::Newline,
                    });
                }
            }
        }
        if let Some(preview) = preview
            && !inserted
        {
            cursor_after_preview =
                Self::push_preview(&mut out, preview, preview_style, matched_style);
        }
        if cursor_after_preview {
            out.push(Self::cursor_marker());
        }
        out
    }

    fn push_preview(
        out: &mut Vec<DrawableBufferSpan>,
        preview: &CompletionPreview,
        style: Style,
        matched_style: Style,
    ) -> bool {
        let mut mapping = preview.mapping.iter().peekable();
        for (byte, graph) in preview.text.grapheme_indices(true) {
            while mapping
                .peek()
                .is_some_and(|entry| entry.display.end <= byte)
            {
                mapping.next();
            }
            let source = mapping
                .peek()
                .filter(|entry| entry.display.start < byte + graph.len())
                .map(|entry| preview.range.start + entry.source.start);
            let is_match = source.is_some();
            let is_cursor = byte == preview.cursor_in_preview;
            if is_cursor && !is_match {
                // Anchor before ghost text wraps: a wide suggested glyph must
                // not move the real insertion point to the next terminal row.
                out.push(Self::cursor_marker());
            }
            let style = if is_match {
                matched_style
                    .remove_modifier(Modifier::DIM)
                    .add_modifier(Modifier::BOLD | Modifier::ITALIC)
            } else {
                style.add_modifier(Modifier::ITALIC)
            };
            let newline = graph == "\n" || graph == "\r\n";
            out.push(DrawableBufferSpan {
                span: Span::styled(if newline { " " } else { graph }.to_owned(), style),
                tags: vec![Tag::Command(source.unwrap_or(preview.cursor))],
                is_cursor: is_cursor && is_match,
                is_in_selection: false,
                newline,
            });
        }
        // At the candidate's end, let the following real grapheme position the
        // cursor (and wrap if needed). Only end-of-buffer needs a blank marker.
        preview.cursor_in_preview == preview.text.len()
    }

    fn cursor_marker() -> DrawableBufferSpan {
        DrawableBufferSpan {
            span: Span::raw(""),
            tags: vec![],
            is_cursor: true,
            is_in_selection: false,
            newline: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_suggestions::ProcessedSuggestion;
    use crate::palette::Palette;
    use flybuffer::{SubString, TextBuffer};
    use ratatui::text::StyledGrapheme;

    fn render(input: &str, target: &str, replacement: &str) -> Vec<DrawableBufferSpan> {
        render_matched(
            input,
            target,
            &ProcessedSuggestion::new(replacement, "", ""),
            &[],
            &Palette::dark(),
        )
    }

    fn render_matched(
        input: &str,
        target: &str,
        suggestion: &ProcessedSuggestion,
        matches: &[usize],
        palette: &Palette,
    ) -> Vec<DrawableBufferSpan> {
        render_at(input, target, suggestion, matches, palette, target.len())
    }

    fn render_at(
        input: &str,
        target: &str,
        suggestion: &ProcessedSuggestion,
        matches: &[usize],
        palette: &Palette,
        cursor_in_word: usize,
    ) -> Vec<DrawableBufferSpan> {
        let start = input.find(target).unwrap();
        let mut buffer = TextBuffer::new(input);
        buffer.try_move_cursor_to_byte_pos(start + cursor_in_word, true);
        let preview = CompletionPreview::new(
            &buffer,
            &SubString {
                start,
                s: target.into(),
            },
            suggestion,
            matches,
        )
        .unwrap();
        FormattedBuffer::from(input, buffer.cursor_byte_pos(), None).drawable_spans(
            None,
            None,
            Some(&preview),
            palette.inline_suggestion(),
            palette.normal_text(),
        )
    }

    fn before_cursor(spans: &[DrawableBufferSpan]) -> String {
        assert_eq!(spans.iter().filter(|span| span.is_cursor).count(), 1);
        let index = spans.iter().position(|span| span.is_cursor).unwrap();
        text(&spans[..index])
    }

    fn layout(spans: Vec<DrawableBufferSpan>, width: u16) -> (crate::content::Coord, Vec<String>) {
        let mut content = crate::content::Contents::new(width);
        let mut cursor = None;
        for span in spans {
            if span.is_cursor {
                let graph = span
                    .span
                    .styled_graphemes(span.span.style)
                    .next()
                    .unwrap_or_else(|| StyledGrapheme::new(" ", Style::default()));
                content.move_to_next_insertion_point(&graph, false, None);
                assert!(cursor.replace(content.cursor_position()).is_none());
            }
            content.write_tagged_span_dont_overwrite(&crate::content::TaggedSpan::per_grapheme(
                span.span, span.tags,
            ));
            if span.newline {
                content.newline();
                content.write_tagged_span(&crate::content::TaggedSpan::new(
                    Span::raw("> "),
                    Tag::Prompt,
                ));
            }
        }
        (cursor.unwrap(), content.get_buffer_lines())
    }

    #[test]
    fn preview_cursor_follows_source_position_and_fuzzy_match_indices() {
        for (input, candidate, matches, cursor, expected) in [
            ("f", "far", vec![0], 1, "f"),
            ("fb", "foo_bar", vec![0, 4], 2, "foo_b"),
            ("fb", "foo_bar", vec![0, 4], 1, "f"),
            ("fo", "foo", vec![0, 1], 1, "f"),
            ("fo", "foo", vec![0, 1], 0, ""),
            ("FO", "foo", vec![0, 1], 2, "fo"),
            ("f", "f/f", vec![2], 1, "f/f"),
            ("e", "e\u{301}clair", vec![0], 1, "e\u{301}"),
            ("é", "été", vec![0], "é".len(), "é"),
            ("f*", "far", vec![], 2, "f"),
        ] {
            let spans = render_at(
                &format!("cmd {input} tail"),
                input,
                &ProcessedSuggestion::new(candidate, "", " "),
                &matches,
                &Palette::dark(),
                cursor,
            );
            assert_eq!(
                before_cursor(&spans),
                format!("cmd {expected}"),
                "input {input:?}, cursor {cursor}"
            );
            assert_eq!(text(&spans), format!("cmd {candidate}  tail"));
        }
    }

    #[test]
    fn cursor_maps_preserved_paths_and_both_sides_of_a_closing_quote() {
        let path = render_matched(
            "cmd rép/é",
            "rép/é",
            &ProcessedSuggestion::new("été", "rép/", " "),
            &[0],
            &Palette::dark(),
        );
        assert_eq!(before_cursor(&path), "cmd rép/é");
        let suggestion = ProcessedSuggestion::new("été", "\"", "\"");
        for (cursor, expected) in [("\"é".len(), "cmd \"é"), ("\"é\"".len(), "cmd \"été\"")] {
            let spans = render_at(
                "cmd \"é\" tail",
                "\"é\"",
                &suggestion,
                &[0],
                &Palette::dark(),
                cursor,
            );
            assert_eq!(before_cursor(&spans), expected);
        }
    }

    #[test]
    fn cursor_stays_at_the_start_of_an_empty_word() {
        let buffer = TextBuffer::new("cmd ");
        let preview = CompletionPreview::new(
            &buffer,
            &SubString {
                start: 4,
                s: String::new(),
            },
            &ProcessedSuggestion::new("far", "", ""),
            &[],
        )
        .unwrap();
        let spans = FormattedBuffer::from("cmd ", 4, None).drawable_spans(
            None,
            None,
            Some(&preview),
            Style::default(),
            Style::default(),
        );
        assert_eq!(before_cursor(&spans), "cmd ");
        assert_eq!(text(&spans), "cmd far");
    }

    #[test]
    fn cursor_coordinates_follow_input_when_preview_wraps_or_contains_newlines() {
        let spans = || {
            render_matched(
                "cmd f",
                "f",
                &ProcessedSuggestion::new("far", "", ""),
                &[0],
                &Palette::dark(),
            )
        };
        assert_eq!(layout(spans(), 20).0, crate::content::Coord::new(0, 5));
        assert_eq!(layout(spans(), 5).0, crate::content::Coord::new(1, 0));

        let wide_ghost = render_matched(
            "cmd f",
            "f",
            &ProcessedSuggestion::new("f界", "", ""),
            &[0],
            &Palette::dark(),
        );
        let (cursor, lines) = layout(wide_ghost, 6);
        assert_eq!(cursor, crate::content::Coord::new(0, 5));
        assert!(lines[1].starts_with("界"));

        // A real wide character following the replaced word behaves like a
        // normal editor cursor, rather than the virtual ghost-text boundary.
        let wide_suffix = render_matched(
            "cmd f界",
            "f",
            &ProcessedSuggestion::new("f", "", ""),
            &[0],
            &Palette::dark(),
        );
        assert_eq!(layout(wide_suffix, 6).0, crate::content::Coord::new(1, 0));

        let multiline = render_matched(
            "cmd fb\ntail",
            "fb",
            &ProcessedSuggestion::new("foo\nbar", "", ""),
            &[0, 4],
            &Palette::dark(),
        );
        assert_eq!(text(&multiline), "cmd foo\nbar\ntail");
        assert_eq!(multiline.iter().filter(|span| span.newline).count(), 2);
        assert_eq!(layout(multiline, 20).0, crate::content::Coord::new(1, 3));
    }

    fn emphasized_text(spans: &[DrawableBufferSpan]) -> String {
        spans
            .iter()
            .filter(|s| {
                s.span
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD | Modifier::ITALIC)
            })
            .map(|s| s.span.content.as_ref())
            .collect()
    }

    #[test]
    fn typed_prefix_has_normal_contrast_and_generated_text_keeps_preview_style() {
        for palette in [Palette::dark(), Palette::light()] {
            let spans = render_matched(
                "cmd fo",
                "fo",
                &ProcessedSuggestion::new("foo", "", " "),
                &[0, 1],
                &palette,
            );
            assert_eq!(text(&spans), "cmd foo ");
            assert_eq!(emphasized_text(&spans), "fo");
            for span in spans
                .iter()
                .filter(|s| s.span.style.add_modifier.contains(Modifier::ITALIC))
            {
                let expected = if span.span.style.add_modifier.contains(Modifier::BOLD) {
                    palette
                        .normal_text()
                        .remove_modifier(Modifier::DIM)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC)
                } else {
                    palette.inline_suggestion().add_modifier(Modifier::ITALIC)
                };
                assert_eq!(span.span.style, expected);
            }
        }
    }

    #[test]
    fn fuzzy_match_highlights_typed_letters_at_their_actual_positions() {
        let spans = render_matched(
            "cmd fb",
            "fb",
            &ProcessedSuggestion::new("foo_bar", "", " "),
            &[0, 4],
            &Palette::dark(),
        );
        assert_eq!(text(&spans), "cmd foo_bar ");
        assert_eq!(emphasized_text(&spans), "fb");
    }

    #[test]
    fn mouse_positions_share_the_same_unicode_mapping_as_emphasis() {
        let spans = render_matched(
            "cmd rép/é",
            "rép/é",
            &ProcessedSuggestion::new("été", "rép/", " "),
            &[0],
            &Palette::dark(),
        );
        let typed: Vec<_> = spans
            .iter()
            .filter(|span| {
                span.span
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD | Modifier::ITALIC)
            })
            .map(|span| (span.span.content.as_ref(), span.tags[0]))
            .collect();
        assert_eq!(
            typed,
            vec![
                ("r", Tag::Command(4)),
                ("é", Tag::Command(5)),
                ("p", Tag::Command(7)),
                ("/", Tag::Command(8)),
                ("é", Tag::Command(9))
            ]
        );
        assert_eq!(before_cursor(&spans), "cmd rép/é");
        assert_eq!(text(&spans), "cmd rép/été ");
        assert_eq!(emphasized_text(&spans), "rép/é");
    }

    #[test]
    fn case_expanding_prefixes_preserve_emphasis_cursor_and_mouse_positions() {
        use crate::active_suggestions::{ActiveSuggestions, ActiveSuggestionsBuilder};

        for (input, candidate, matches, typed) in [
            ("i\u{307}", "İabc", vec![0], vec![("İ", Tag::Command(4))]),
            (
                "İ",
                "i\u{307}xyz",
                vec![0, 1],
                vec![("i\u{307}", Tag::Command(4))],
            ),
            (
                "İi\u{307}",
                "i\u{307}İabc",
                vec![0, 1, 2],
                vec![("i\u{307}", Tag::Command(4)), ("İ", Tag::Command(6))],
            ),
        ] {
            let active = ActiveSuggestions::new(
                ActiveSuggestionsBuilder::from_processed([ProcessedSuggestion::new(
                    candidate, "", "",
                )]),
                SubString {
                    start: 4,
                    s: input.into(),
                },
                std::time::Duration::ZERO,
                false,
                crate::settings::SuggestionSortOrder::Alphabetical,
                crate::settings::FuzzyMode::None,
            );
            let (suggestion, actual_matches) = active.selected_suggestion().unwrap();
            assert_eq!(
                actual_matches, matches,
                "input {input:?}, candidate {candidate:?}"
            );
            let spans = render_matched(
                &format!("cmd {input}"),
                input,
                suggestion,
                actual_matches,
                &Palette::dark(),
            );
            let actual_typed: Vec<_> = spans
                .iter()
                .filter(|span| span.span.style.add_modifier.contains(Modifier::BOLD))
                .map(|span| (span.span.content.as_ref(), span.tags[0]))
                .collect();
            assert_eq!(actual_typed, typed);
            assert_eq!(
                before_cursor(&spans),
                format!("cmd {}", emphasized_text(&spans))
            );
            assert_eq!(text(&spans), format!("cmd {candidate}"));
        }
    }

    #[test]
    fn escaped_space_is_input_not_a_generated_separator() {
        let spans = render_matched(
            "cmd foo\\ ",
            "foo\\ ",
            &ProcessedSuggestion::new("foo\\ one", "", " "),
            &[0, 1, 2, 3, 4],
            &Palette::dark(),
        );
        assert_eq!(text(&spans), "cmd foo\\ one ");
        assert_eq!(before_cursor(&spans), "cmd foo\\ ");
        assert_eq!(emphasized_text(&spans), "foo\\ ");
        assert!(
            !spans
                .last()
                .unwrap()
                .span
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        let (cursor, _) = layout(spans, 30);
        assert_eq!(cursor, crate::content::Coord::new(0, 9));
    }

    #[test]
    fn matched_combining_character_styles_the_entire_grapheme() {
        let spans = render_matched(
            "cmd e",
            "e",
            &ProcessedSuggestion::new("e\u{301}clair", "", ""),
            &[0],
            &Palette::dark(),
        );
        assert_eq!(emphasized_text(&spans), "e\u{301}");
        let first = spans.iter().find(|s| s.span.content == "e\u{301}").unwrap();
        assert_eq!(first.tags, vec![Tag::Command(4)]);
    }

    #[test]
    fn preserves_typed_quotes_and_escaped_fuzzy_matches() {
        let quoted = render_matched(
            "cmd \"é\"",
            "\"é\"",
            &ProcessedSuggestion::new("été", "\"", "\""),
            &[0],
            &Palette::dark(),
        );
        assert_eq!(text(&quoted), "cmd \"été\"");
        assert_eq!(emphasized_text(&quoted), "\"é\"");
        let escaped = render_matched(
            "cmd fb",
            "fb",
            &ProcessedSuggestion::new("foo\\ bar", "", " "),
            &[0, 5],
            &Palette::dark(),
        );
        assert_eq!(text(&escaped), "cmd foo\\ bar ");
        assert_eq!(emphasized_text(&escaped), "fb");
    }

    fn text(spans: &[DrawableBufferSpan]) -> String {
        spans
            .iter()
            .map(|s| if s.newline { "\n" } else { &s.span.content })
            .collect()
    }

    #[test]
    fn replaces_whole_fuzzy_match_and_preserves_suffix_mouse_offsets() {
        let spans = render_matched(
            "cmd fb tail",
            "fb",
            &ProcessedSuggestion::new("foo\\ bar", "", " "),
            &[0, 5],
            &Palette::dark(),
        );
        assert_eq!(text(&spans), "cmd foo\\ bar  tail");
        let italic: Vec<_> = spans
            .iter()
            .filter(|s| s.span.style.add_modifier.contains(Modifier::ITALIC))
            .collect();
        assert_eq!(
            italic
                .iter()
                .map(|s| s.span.content.as_ref())
                .collect::<String>(),
            "foo\\ bar "
        );
        assert_eq!(
            italic.iter().find(|s| s.span.content == "f").unwrap().tags,
            vec![Tag::Command(4)]
        );
        assert_eq!(
            italic.iter().find(|s| s.span.content == "b").unwrap().tags,
            vec![Tag::Command(5)]
        );
        assert!(
            italic
                .iter()
                .filter(|s| !s.span.style.add_modifier.contains(Modifier::BOLD))
                .all(|s| s.tags == vec![Tag::Command(6)])
        );
        let tail = spans.iter().find(|s| s.span.content == "t").unwrap();
        assert_eq!(tail.tags, vec![Tag::Command(7)]);
        let cursor = spans.iter().position(|s| s.is_cursor).unwrap();
        assert_eq!(text(&spans[..cursor]), "cmd foo\\ b");
        assert_eq!(spans.iter().filter(|s| s.is_cursor).count(), 1);
    }

    #[test]
    fn quoted_unicode_target_can_span_syntax_tokens() {
        let spans = render("cmd \"é\" tail", "\"é\"", "\"été deux\" ");
        assert_eq!(text(&spans), "cmd \"été deux\"  tail");
    }

    #[test]
    fn wide_preview_wraps_in_narrow_terminals_without_corrupting_tags() {
        for width in [2, 3, 8, 20] {
            let spans = render("cmd f tail", "f", "界é界");
            let mut content = crate::content::Contents::new(width);
            for drawable in spans {
                content.write_tagged_span_dont_overwrite(
                    &crate::content::TaggedSpan::per_grapheme(drawable.span, drawable.tags),
                );
            }
            let command_tags: Vec<_> = content
                .buf
                .iter()
                .flat_map(|row| row.iter())
                .filter_map(|cell| match cell.tag {
                    Tag::Command(byte) => Some(byte),
                    _ => None,
                })
                .collect();
            assert!(command_tags.contains(&5)); // Preview maps to the real cursor.
            assert!(command_tags.contains(&9)); // Last suffix byte keeps its real offset.
            assert!(command_tags.iter().all(|byte| *byte < "cmd f tail".len()));
        }
    }

    #[test]
    fn refuses_stale_targets_cursor_moves_and_selections() {
        let mut buffer = TextBuffer::new("cmd f tail");
        let target = SubString {
            start: 4,
            s: "f".into(),
        };
        assert!(
            CompletionPreview::new(
                &buffer,
                &target,
                &ProcessedSuggestion::new("foo", "", ""),
                &[]
            )
            .is_none()
        );
        buffer.try_move_cursor_to_byte_pos(5, true);
        let stale = SubString {
            start: 4,
            s: "z".into(),
        };
        assert!(
            CompletionPreview::new(
                &buffer,
                &stale,
                &ProcessedSuggestion::new("foo", "", ""),
                &[]
            )
            .is_none()
        );
        assert!(
            CompletionPreview::new(
                &buffer,
                &target,
                &ProcessedSuggestion::new("foo", "", ""),
                &[]
            )
            .is_some()
        );
        buffer.start_selection_if_none();
        assert!(
            CompletionPreview::new(
                &buffer,
                &target,
                &ProcessedSuggestion::new("foo", "", ""),
                &[]
            )
            .is_none()
        );
    }
}
