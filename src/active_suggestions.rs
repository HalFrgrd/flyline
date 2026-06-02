use crate::content_utils::{
    ansi_string_to_spans, highlight_matching_indices, middle_truncate_spans, style_for_path,
    take_prefix_of_spans, ts_to_timeago_string_5chars, vec_spans_width,
};
use crate::palette::Palette;
use crate::stateful_sliding_window::StatefulSlidingWindow;
use crate::text_buffer::{SubString, TextBuffer};
use crate::{bash_funcs, tab_completion_context};
use itertools::Itertools;
use ratatui::prelude::*;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::vec;

use unicode_width::UnicodeWidthStr;

/// Number of whitespace characters inserted between adjacent columns in the
/// suggestions grid.
pub(crate) const COLUMN_PADDING: usize = 2;

/// Describes what to display alongside a suggestion as a visual suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionDescription {
    /// Pre-processed spans for a single static description.  An empty vec
    /// means no description is shown.
    Static(Vec<Span<'static>>),
    /// A multi-frame animated description.  Frames are cycled at ANIMATION_FRAME_FPS fps.
    /// Each frame is a pre-processed sequence of styled spans.
    Animation(Vec<Vec<Span<'static>>>),
    /// Last-modification time of the associated file (Unix timestamp).
    /// Rendered as a right-aligned, ≤5-character "time ago" string.
    LastMTime(u64),
}

pub const ANIMATION_FRAME_FPS: u64 = 24;

impl SuggestionDescription {
    /// Maximum display width (in terminal columns) across all frames.
    pub fn max_width(&self) -> usize {
        match self {
            SuggestionDescription::Static(spans) => vec_spans_width(spans),
            SuggestionDescription::Animation(frames) => {
                frames.iter().map(|f| vec_spans_width(f)).max().unwrap_or(0)
            }
            SuggestionDescription::LastMTime(_) => 5,
        }
    }

    /// Returns the spans to display for `frame_index`, or `None` when the
    /// description is empty.
    pub fn frame_at(&self, frame_index: usize) -> Option<Vec<Span<'static>>> {
        match self {
            SuggestionDescription::Static(spans) if spans.is_empty() => None,
            SuggestionDescription::Static(spans) => Some(spans.clone()),
            SuggestionDescription::Animation(frames) if frames.is_empty() => None,
            SuggestionDescription::Animation(frames) => {
                Some(frames[frame_index % frames.len()].clone())
            }
            SuggestionDescription::LastMTime(ts) => {
                Some(vec![Span::raw(ts_to_timeago_string_5chars(*ts))])
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedSuggestion {
    pub s: String,
    pub prefix: String,
    pub suffix: String,
    /// Optional display style (e.g. from LS_COLORS) applied when rendering in the completion list.
    pub style: Option<Style>,
    /// Description to display as a visual suffix (not inserted).
    pub description: SuggestionDescription,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionFormatted {
    pub suggestion_idx: usize,
    /// Visual width used for column sizing. Includes the description separator and the
    /// widest description frame so that the column does not resize during animation.
    pub display_width: usize,
    pub spans: Vec<Span<'static>>,
    /// Pre-processed description spans for the current animation frame (empty if there is
    /// no description). Truncation is decided at render time according to the
    /// available column width.
    description_frame: Vec<Span<'static>>,
    /// Style applied as a base when rendering description spans (patched by each
    /// span's own style so that ANSI-encoded colours take precedence).
    description_style: Style,
    /// Width of the current description frame (excluding the separator).
    description_frame_width: usize,
}

impl SuggestionFormatted {
    /// Width of the separator between the suggestion text and its description.
    const DESCRIPTION_SEPARATOR: &'static str = "  ";

    /// Minimum number of terminal columns that must be available for a
    /// description to be shown at all. When the suggestion column has to be
    /// truncated and there are fewer than this many columns left over for the
    /// description (after the suggestion text and the separator), the
    /// description is dropped entirely; otherwise the description is
    /// truncated down to whatever space is available.
    const MIN_DESCRIPTION_WIDTH: usize = 20;

    pub fn new(
        suggestion: &ProcessedSuggestion,
        suggestion_idx: usize,
        matching_indices: Vec<usize>,
        palette: &Palette,
        frame_index: usize,
    ) -> Self {
        let base_style = suggestion.style.unwrap_or(palette.normal_text());
        let lines =
            highlight_matching_indices(palette, &suggestion.s, &matching_indices, base_style);

        let main_spans: Vec<Span<'static>> = lines.into_iter().flat_map(|l| l.spans).collect();
        let main_width = suggestion.s.width();

        // Compute the widest description frame to use for stable column sizing.
        let max_description_frame_width = suggestion.description.max_width();

        // Select the description frame to display for this render cycle.
        let description_style = palette.secondary_text();
        let (description_frame, description_frame_width) =
            match suggestion.description.frame_at(frame_index) {
                None => (vec![], 0),
                Some(frame) => {
                    let width = vec_spans_width(&frame);
                    (frame, width)
                }
            };

        // Column width accounts for the widest frame so the column stays
        // stable across animation frames.
        let display_width = if max_description_frame_width > 0 {
            main_width + Self::DESCRIPTION_SEPARATOR.len() + max_description_frame_width
        } else {
            main_width
        };

        SuggestionFormatted {
            suggestion_idx,
            display_width,
            spans: main_spans,
            description_frame,
            description_style,
            description_frame_width,
        }
    }

    /// Render this suggestion into a sequence of styled [`Span`]s.
    ///
    /// `col_width` is the visual width reserved for this cell (excluding any
    /// trailing padding).  When `col_width` is smaller than the suggestion
    /// text, middle-ellipsis truncation is applied so the text fits exactly
    /// within `col_width` characters.
    pub fn render(&self, col_width: usize, is_selected: bool) -> Vec<Span<'static>> {
        // Determine widths available for the main text and description.
        let main_text_width = vec_spans_width(&self.spans);
        let has_description = !self.description_frame.is_empty();
        let desc_total_width = if has_description {
            Self::DESCRIPTION_SEPARATOR.len() + self.description_frame_width
        } else {
            0
        };

        // Layout policy when the column has to be truncated:
        //   - Look at suggestion width + description width. If it fits, render
        //     everything as-is.
        //   - Otherwise, look at the space left for the description after the
        //     suggestion text and separator:
        //       * If `< MIN_DESCRIPTION_WIDTH`, drop the description entirely
        //         and only then truncate the suggestion text using the
        //         existing middle-ellipsis logic.
        //       * Otherwise, truncate the description down to that available
        //         width and keep the full suggestion text.
        let (main_col_width, desc_render_width) =
            if !has_description || col_width >= main_text_width + desc_total_width {
                (col_width.min(main_text_width), self.description_frame_width)
            } else {
                // Truncation needed.
                let space_after_main =
                    col_width.saturating_sub(main_text_width + Self::DESCRIPTION_SEPARATOR.len());
                if space_after_main < Self::MIN_DESCRIPTION_WIDTH {
                    // Not enough room for a description — drop it and truncate
                    // the suggestion text instead.
                    (col_width.min(main_text_width), 0)
                } else {
                    // Keep the full suggestion text; truncate the description
                    // down to whatever fits.
                    (
                        main_text_width,
                        space_after_main.min(self.description_frame_width),
                    )
                }
            };

        let mut spans: Vec<Span<'static>> = if main_col_width < main_text_width {
            middle_truncate_spans(&self.spans, main_col_width)
        } else {
            self.spans.clone()
        };

        if is_selected {
            spans = spans
                .into_iter()
                .map(|span| Span::styled(span.content, Palette::convert_to_highlighted(span.style)))
                .collect();
        }

        let rendered_main_len = vec_spans_width(&spans);

        let desc_total_render_width = if desc_render_width > 0 {
            Self::DESCRIPTION_SEPARATOR.len() + desc_render_width
        } else {
            0
        };
        let rendered_total = rendered_main_len + desc_total_render_width;
        spans.push(Span::raw(
            " ".repeat(col_width.saturating_sub(rendered_total)),
        ));

        // Append description if there is space for it.
        if desc_render_width > 0 {
            spans.push(Span::raw(Self::DESCRIPTION_SEPARATOR));
            let truncated = take_prefix_of_spans(&self.description_frame, desc_render_width);
            spans.extend(
                truncated.into_iter().map(|span| {
                    Span::styled(span.content, self.description_style.patch(span.style))
                }),
            );
        }

        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middle_truncate_spans_preserves_styles() {
        let a = Style::default().fg(Color::Red);
        let b = Style::default().fg(Color::Blue);

        let spans = vec![
            Span::styled("abcd".to_string(), a),
            Span::styled("EFGH".to_string(), b),
        ];

        let out = middle_truncate_spans(&spans, 5);
        assert_eq!(vec_spans_width(&out), 5);
        assert_eq!(
            out.iter().map(|s| s.content.as_ref()).collect::<String>(),
            "ab…GH"
        );

        // Left piece keeps style a, right piece keeps style b.
        assert_eq!(out[0].style, a);
        assert_eq!(out.last().unwrap().style, b);
    }

    #[test]
    fn middle_truncate_spans_handles_tiny_widths() {
        let s = Style::default().fg(Color::Green);
        let spans = vec![Span::styled("hello".to_string(), s)];

        let out0 = middle_truncate_spans(&spans, 0);
        assert_eq!(out0.len(), 0);

        let out1 = middle_truncate_spans(&spans, 1);
        assert_eq!(vec_spans_width(&out1), 1);
        assert_eq!(out1[0].content.as_ref(), "…");
        assert_eq!(out1[0].style, s);
    }
}

#[cfg(test)]
mod description_tests {
    use super::*;

    #[test]
    fn raw_match_text_strips_description() {
        let item = UnprocessedSuggestion {
            raw_text: "git-commit\tRecord changes".to_string(),
            full_path: None,
            flags: crate::bash_funcs::CompletionFlags::default(),
            word_under_cursor: "git".to_string(),
        };
        assert_eq!(item.match_text(), "git-commit");
    }

    #[test]
    fn raw_match_text_no_tab_unchanged() {
        let item = UnprocessedSuggestion {
            raw_text: "git-commit".to_string(),
            full_path: None,
            flags: crate::bash_funcs::CompletionFlags::default(),
            word_under_cursor: "git".to_string(),
        };
        assert_eq!(item.match_text(), "git-commit");
    }

    #[test]
    fn suggestion_with_description_formatted_omits_description() {
        // formatted() must only include what gets inserted (s + prefix + suffix).
        let sug = ProcessedSuggestion::new("cmd", "", " ").with_description(
            SuggestionDescription::Animation(vec![vec![Span::raw("description text")]]),
        );
        assert_eq!(sug.formatted(), "cmd ");
        assert!(!sug.formatted().contains("description"));
    }

    #[test]
    fn description_frame_cycling() {
        let sug = ProcessedSuggestion::new("x", "", "").with_description(
            SuggestionDescription::Animation(vec![
                vec![Span::raw("a")],
                vec![Span::raw("b")],
                vec![Span::raw("c")],
            ]),
        );
        let palette = crate::palette::Palette::default();

        let f0 = SuggestionFormatted::new(&sug, 0, vec![], &palette, 0);
        let f1 = SuggestionFormatted::new(&sug, 0, vec![], &palette, 1);
        let f2 = SuggestionFormatted::new(&sug, 0, vec![], &palette, 2);
        // Frame 3 wraps back to frame 0.
        let f3 = SuggestionFormatted::new(&sug, 0, vec![], &palette, 3);

        assert_eq!(f0.description_frame, vec![Span::raw("a")]);
        assert_eq!(f1.description_frame, vec![Span::raw("b")]);
        assert_eq!(f2.description_frame, vec![Span::raw("c")]);
        assert_eq!(f3.description_frame, vec![Span::raw("a")]);
    }

    #[test]
    fn display_width_stable_across_frames() {
        let sug = ProcessedSuggestion::new("abc", "", "").with_description(
            SuggestionDescription::Animation(vec![
                vec![Span::raw("short")],
                vec![Span::raw("a much longer description")],
            ]),
        );
        let palette = crate::palette::Palette::default();
        let fw0 = SuggestionFormatted::new(&sug, 0, vec![], &palette, 0).display_width;
        let fw1 = SuggestionFormatted::new(&sug, 0, vec![], &palette, 1).display_width;
        // display_width must not change between frames.
        assert_eq!(fw0, fw1);
        // display_width = "abc".len() + separator(2) + max("short", "a much longer description").len()
        let expected = "abc".len() + 2 + "a much longer description".len();
        assert_eq!(fw0, expected);
    }

    #[test]
    fn no_description_display_width_equals_text_width() {
        let sug = ProcessedSuggestion::new("hello", "", "");
        let palette = crate::palette::Palette::default();
        let fw = SuggestionFormatted::new(&sug, 0, vec![], &palette, 0).display_width;
        assert_eq!(fw, "hello".len());
    }

    #[test]
    fn last_mtime_description_max_width_is_5() {
        let sug = ProcessedSuggestion::new("file.txt", "", " ")
            .with_description(SuggestionDescription::LastMTime(0));
        assert_eq!(sug.description.max_width(), 5);
    }

    #[test]
    fn last_mtime_description_frame_is_nonempty() {
        let sug = ProcessedSuggestion::new("file.txt", "", " ")
            .with_description(SuggestionDescription::LastMTime(0));
        let frame = sug.description.frame_at(0);
        assert!(frame.is_some());
        let spans = frame.unwrap();
        let total_width: usize = spans.iter().map(|s| s.width()).sum();
        assert_eq!(
            total_width, 5,
            "LastMTime frame must be exactly 5 chars wide"
        );
    }

    #[test]
    fn static_empty_description_is_empty() {
        let sug = ProcessedSuggestion::new("foo", "", "");
        assert_eq!(sug.description, SuggestionDescription::Static(vec![]));
        assert_eq!(sug.description.max_width(), 0);
        assert!(sug.description.frame_at(0).is_none());
    }

    #[test]
    fn static_nonempty_description_frame() {
        let sug = ProcessedSuggestion::new("foo", "", "")
            .with_description(SuggestionDescription::Static(vec![Span::raw("hello")]));
        assert_eq!(sug.description.max_width(), 5);
        assert_eq!(sug.description.frame_at(0), Some(vec![Span::raw("hello")]));
        // frame_at is stable for any index
        assert_eq!(sug.description.frame_at(99), Some(vec![Span::raw("hello")]));
    }

    #[test]
    fn test_into_processed_nospace_for_equals_flags() {
        // Case 1: Option ends with = and some_dont_end_in_equal_sign is true
        let mut flags_with_flag = crate::bash_funcs::CompletionFlags::default();
        flags_with_flag.some_dont_end_in_equal_sign = true;

        let sug1 = UnprocessedSuggestion {
            raw_text: "--long-opt=".to_string(),
            full_path: None,
            flags: flags_with_flag,
            word_under_cursor: "".to_string(),
        }
        .into_processed();

        assert_eq!(sug1.s, "--long-opt=");
        assert_eq!(sug1.suffix, ""); // should be empty (no space)

        // Case 2: Option ends with = but some_dont_end_in_equal_sign is false
        let mut flags_no_flag = crate::bash_funcs::CompletionFlags::default();
        flags_no_flag.some_dont_end_in_equal_sign = false;

        let sug2 = UnprocessedSuggestion {
            raw_text: "--long-opt=".to_string(),
            full_path: None,
            flags: flags_no_flag,
            word_under_cursor: "".to_string(),
        }
        .into_processed();

        assert_eq!(sug2.s, "--long-opt=");
        assert_eq!(sug2.suffix, " "); // should have a space because flag is false

        // Case 3: Option does not end with = but some_dont_end_in_equal_sign is true
        let sug3 = UnprocessedSuggestion {
            raw_text: "--foo".to_string(),
            full_path: None,
            flags: flags_with_flag,
            word_under_cursor: "".to_string(),
        }
        .into_processed();

        assert_eq!(sug3.s, "--foo");
        assert_eq!(sug3.suffix, " "); // should still have a space
    }

    #[test]
    fn test_into_list_windowing() {
        let palette = crate::palette::Palette::default();
        let builder = ActiveSuggestionsBuilder {
            processed: vec![
                ProcessedSuggestion::new("sug1", "", ""),
                ProcessedSuggestion::new("sug2", "", ""),
                ProcessedSuggestion::new("sug3", "", ""),
                ProcessedSuggestion::new("sug4", "", ""),
            ],
            unprocessed: std::collections::VecDeque::new(),
            common_prefix: None,
            auto_accept_if_solo: false,
            insert_common_prefix: false,
            comp_type: crate::tab_completion_context::CompType::FirstWord,
        };
        let mut active = ActiveSuggestions::new(
            builder,
            SubString::new("", "").unwrap(),
            std::time::Duration::from_millis(0),
            true, // auto_started
        );

        // Grid width/rows logic, let's call into_list with max_rows = 2
        let list1 = active.into_list(2, &palette);
        assert_eq!(list1.len(), 2);
        assert_eq!(
            list1[0]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "sug1"
        );
        assert_eq!(
            list1[1]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "sug2"
        );

        // Now change selected coordinate and test sliding window movement
        active.selected_coord = Some((0, 2)); // select index 2 ("sug3")
        let list2 = active.into_list(2, &palette);
        assert_eq!(list2.len(), 2);
        assert_eq!(
            list2[0]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "sug2"
        );
        assert_eq!(
            list2[1]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "sug3"
        );

        // Test arrow key movement on list
        active.on_down_arrow(); // should move from index 2 to index 3
        assert_eq!(active.selected_coord, Some((0, 3)));
        active.on_down_arrow(); // should wrap from 3 to 0
        assert_eq!(active.selected_coord, Some((0, 0)));
        active.on_up_arrow(); // should wrap from 0 to 3
        assert_eq!(active.selected_coord, Some((0, 3)));
        active.on_up_arrow(); // should move from 3 to 2
        assert_eq!(active.selected_coord, Some((0, 2)));
    }
}

impl ProcessedSuggestion {
    pub fn new<S: Into<String>, P: Into<String>, X: Into<String>>(
        s: S,
        prefix: P,
        suffix: X,
    ) -> Self {
        ProcessedSuggestion {
            s: s.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
            style: None,
            description: SuggestionDescription::Static(vec![]),
        }
    }

    /// Set the description on this suggestion.
    pub fn with_description(mut self, description: SuggestionDescription) -> Self {
        self.description = description;
        self
    }

    /// Set an optional display style (e.g. derived from `LS_COLORS`) on this suggestion.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn formatted(&self) -> String {
        format!("{}{}{}", self.prefix, self.s, self.suffix)
    }

    pub fn from_string_vec(
        suggestions: Vec<String>,
        prefix: &str,
        suffix: &str,
    ) -> Vec<ProcessedSuggestion> {
        suggestions
            .into_iter()
            .map(|s| {
                let new_suffix = if suffix == " " && s.ends_with(suffix) {
                    "".to_string()
                } else {
                    suffix.to_string()
                };
                ProcessedSuggestion::new(s, prefix.to_string(), new_suffix)
            })
            .collect()
    }
}

impl PartialOrd for ProcessedSuggestion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.s.partial_cmp(&other.s)
    }
}
impl Ord for ProcessedSuggestion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.s.cmp(&other.s)
    }
}

/// Raw completion from bash plus the metadata needed to post-process it into
/// a [`ProcessedSuggestion`] via [`into_processed`].
///
/// The expensive filesystem calls (`is_dir`, `style_for_path`,
/// `fully_expand_path`) only happen when this is converted to a
/// [`ProcessedSuggestion`].
#[derive(Debug, Clone, Default)]
pub struct UnprocessedSuggestion {
    pub raw_text: String,
    pub full_path: Option<PathBuf>,
    pub flags: bash_funcs::CompletionFlags,
    pub word_under_cursor: String,
}

impl UnprocessedSuggestion {
    pub fn match_text(&self) -> &str {
        Self::split_completion_description(&self.raw_text).0
    }

    /// Split a raw completion string into the completion text and description frames.
    ///
    /// Any tab characters in `raw` serve as separators: the text before the first
    /// tab is the value that gets inserted; each subsequent tab-separated segment
    /// is one frame of the animated description.
    fn split_completion_description(raw: &str) -> (&str, Vec<String>) {
        match raw.split_once('\t') {
            None => (raw, vec![]),
            Some((text, rest)) => {
                let frames: Vec<String> = rest.split('\t').map(|s| s.to_owned()).collect();
                (text, frames)
            }
        }
    }

    /// Post-process a single raw completion string into a [`ProcessedSuggestion`].
    ///
    /// This performs quoting, filesystem checks (`is_dir`, `style_for_path`), and
    /// suffix computation.  Expensive for filenames due to syscalls; call lazily.
    ///
    /// If `raw_sug` contains tab characters the text before the first tab is the
    /// completion value; the remaining tab-separated segments are treated as
    /// animation frames for the description (used when no higher-priority
    /// description type applies).
    pub fn into_processed(self) -> ProcessedSuggestion {
        let raw_sug = &self.raw_text;
        let mut path_to_use = self.full_path;
        let comp_result_flags = self.flags;
        let word_under_cursor = &self.word_under_cursor;

        let (sug, desc_frames) = Self::split_completion_description(raw_sug);
        let mut sug = sug.to_string();

        if comp_result_flags.filename_completion_desired {
            if path_to_use.is_none() {
                path_to_use = Some(std::path::PathBuf::from(bash_funcs::fully_expand_path(
                    &sug,
                )));
            }
        }

        let suffix_char = if path_to_use.as_ref().is_some_and(|p| p.is_dir()) {
            sug = format!("{}/", sug);
            None
        } else if comp_result_flags.quote_type.is_some_and(|q| {
            q == bash_funcs::QuoteType::SingleQuote || q == bash_funcs::QuoteType::DoubleQuote
        }) {
            // If we put a space after a filename that is quoted, bash thinks we want a filename ending in a space.
            None
        } else if comp_result_flags.no_suffix_desired {
            None
        } else if comp_result_flags.some_dont_end_in_equal_sign && sug.ends_with('=') {
            // Bash completion specs are run many times normally.
            // So when bash completion spec returns just one value like `--long-opt=`,
            // it sets nospace=true. But since in flyline, we might only run the completion spec once,
            // and get multiple values like `--long-opt=` and `--lolly` (without =), we can't fully rely
            // on nospace=true to decide whether to add a space after `--long-opt=`.
            None
        } else if comp_result_flags.suffix_character == ' ' {
            if sug.ends_with(" ") { None } else { Some(' ') }
        } else {
            Some(comp_result_flags.suffix_character)
        };

        let quoted = if comp_result_flags.filename_quoting_desired
            && comp_result_flags.filename_completion_desired
        {
            if !word_under_cursor.is_empty()
                && let Some(new_suffix) = sug.strip_prefix(word_under_cursor)
            {
                let quoted_suffix = bash_funcs::quoting_function_rust(
                    new_suffix,
                    comp_result_flags.quote_type.unwrap_or_default(),
                    true,
                    false,
                );
                format!("{}{}", word_under_cursor, quoted_suffix)
            } else {
                bash_funcs::quoting_function_rust(
                    &sug,
                    comp_result_flags.quote_type.unwrap_or_default(),
                    true,
                    false,
                )
            }
        } else {
            sug.to_string()
        };

        let (quoted_no_prefix, prefix) = {
            // wuc_prefix does not depend on sug. only wuc
            let wuc_prefix = if comp_result_flags.filename_completion_desired {
                if !word_under_cursor.contains("/") {
                    "".to_string()
                } else if word_under_cursor.ends_with("/") {
                    word_under_cursor.to_string()
                } else {
                    let parent = Path::new(word_under_cursor)
                        .parent()
                        .and_then(|p| p.to_str())
                        .map(|s| {
                            if !s.ends_with("/") {
                                format!("{}/", s)
                            } else {
                                s.to_string()
                            }
                        });

                    if let Some(p) = parent {
                        p
                    } else {
                        "".to_string()
                    }
                }
            } else {
                "".to_string()
            };

            if let Some(quoted_no_prefix) = quoted.strip_prefix(&wuc_prefix) {
                (quoted_no_prefix.to_string(), wuc_prefix)
            } else {
                (quoted.to_string(), "".to_string())
            }
        };

        let style = path_to_use.as_ref().and_then(|p| style_for_path(&p));
        let mtime = path_to_use
            .as_ref()
            .and_then(|p| p.metadata().ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        // Determine description type by priority:
        let description = if let Some(ts) = mtime {
            SuggestionDescription::LastMTime(ts)
        } else if !desc_frames.is_empty() {
            SuggestionDescription::Animation(
                desc_frames
                    .into_iter()
                    .map(|s| ansi_string_to_spans(&s))
                    .collect(),
            )
        } else {
            SuggestionDescription::Static(vec![])
        };

        let suffix_str = suffix_char.map(|f| f.to_string()).unwrap_or_default();
        let suggestion = ProcessedSuggestion::new(quoted_no_prefix, prefix, &suffix_str)
            .with_description(description);
        match style {
            Some(s) => suggestion.with_style(s),
            None => suggestion,
        }
    }
}

const CHUNK_PROCESSING_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// Collects the candidate completions produced by the various `tab_complete_*`
/// helpers before they are handed off to [`ActiveSuggestions`].
///
/// Suggestions are kept in two collections so that already-processed items
/// (env vars, tilde expansion, flyline's own clap completions, ...) are never
/// re-wrapped, while raw bash completions stay queued in `unprocessed` and
/// are converted lazily.  `auto_accept_if_solo` controls whether
/// [`ActiveSuggestions::try_accept`] will silently accept the candidate when
/// there is only one of them (set to `false` for fuzzy filename matching, where
/// the user generally wants to see and confirm the match).
#[derive(Debug, Clone)]
pub struct ActiveSuggestionsBuilder {
    pub processed: Vec<ProcessedSuggestion>,
    pub unprocessed: VecDeque<UnprocessedSuggestion>,
    pub auto_accept_if_solo: bool,
    pub insert_common_prefix: bool,
    pub common_prefix: Option<String>,
    pub comp_type: tab_completion_context::CompType,
}

impl ActiveSuggestionsBuilder {
    /// Create an empty builder with `auto_accept_if_solo = true`, the sensible
    /// default for every completion source other than fuzzy filename matching.
    pub fn new() -> Self {
        Self {
            processed: Vec::new(),
            unprocessed: VecDeque::new(),
            auto_accept_if_solo: true,
            insert_common_prefix: true,
            common_prefix: None,
            comp_type: tab_completion_context::CompType::default(),
        }
    }

    /// Override [`auto_accept_if_solo`].  Set to `false` for fuzzy filename
    /// matching, where the user should confirm the match even when only a
    /// single candidate survives the fuzzy scoring.
    pub fn with_auto_accept_if_solo(mut self, auto_accept_if_solo: bool) -> Self {
        self.auto_accept_if_solo = auto_accept_if_solo;
        self
    }

    pub fn with_insert_common_prefix(mut self, insert_common_prefix: bool) -> Self {
        self.insert_common_prefix = insert_common_prefix;
        self
    }

    pub fn with_comp_type(mut self, comp_type: tab_completion_context::CompType) -> Self {
        self.comp_type = comp_type;
        self
    }

    /// Append a single already-processed suggestion.
    #[allow(dead_code)]
    pub fn push_processed(&mut self, sug: ProcessedSuggestion) {
        self.processed.push(sug);
    }

    /// Append already-processed suggestions in bulk.
    pub fn extend_processed<I: IntoIterator<Item = ProcessedSuggestion>>(&mut self, iter: I) {
        self.processed.extend(iter);
    }

    /// Queue raw suggestions for lazy post-processing.
    pub fn extend_unprocessed<I: IntoIterator<Item = UnprocessedSuggestion>>(&mut self, iter: I) {
        self.unprocessed.extend(iter);
    }

    /// Create a builder pre-populated with already-processed suggestions.
    pub fn from_processed<I: IntoIterator<Item = ProcessedSuggestion>>(iter: I) -> Self {
        let mut builder = Self::new();
        builder.extend_processed(iter);
        builder
    }

    /// Create a builder pre-populated with raw suggestions for lazy post-processing.
    pub fn from_unprocessed<I: IntoIterator<Item = UnprocessedSuggestion>>(iter: I) -> Self {
        let mut builder = Self::new();
        builder.extend_unprocessed(iter);
        builder
    }

    /// `true` when no suggestions of either kind have been collected.
    pub fn is_empty(&self) -> bool {
        self.processed.is_empty() && self.unprocessed.is_empty()
    }

    pub fn len(&self) -> usize {
        self.processed.len() + self.unprocessed.len()
    }

    /// Drain every queued unprocessed suggestion into `processed`, returning
    /// `true` if the work fits within [`CHUNK_PROCESSING_TIMEOUT`] and `false`
    /// if processing was cut short.
    pub fn try_process_all(&mut self) -> bool {
        let start_time = std::time::Instant::now();
        while let Some(raw) = self.unprocessed.pop_front() {
            self.processed.push(raw.into_processed());
            if start_time.elapsed() > CHUNK_PROCESSING_TIMEOUT {
                return self.unprocessed.is_empty();
            }
        }
        true
    }

    pub fn process_all_blocking(&mut self) {
        while let Some(raw) = self.unprocessed.pop_front() {
            self.processed.push(raw.into_processed());
        }
    }

    pub fn set_common_prefix(&mut self) {
        let mut iter = self.processed.iter().map(|s| s.formatted());
        let Some(first) = iter.next() else {
            self.common_prefix = None;
            return;
        };
        let mut prefix_byte_len = first.len();

        for text in iter {
            prefix_byte_len = first
                .chars()
                .zip(text.chars())
                .take_while(|(a, b)| a == b)
                .map(|(c, _)| c.len_utf8())
                .sum::<usize>()
                .min(prefix_byte_len);
            if prefix_byte_len == 0 {
                self.common_prefix = None;
                return;
            }
        }

        if prefix_byte_len == 0 {
            self.common_prefix = None;
        } else {
            self.common_prefix = Some(first[..prefix_byte_len].to_string());
        }
    }
}

/// Lightweight entry in the filtered suggestion list.
///
/// Unlike [`SuggestionFormatted`], this stores only the index, score, and
/// fuzzy-match indices — no precomputed spans or display widths.  The
/// expensive rendering work is done on demand in [`ActiveSuggestions::into_grid`].
///
/// `suggestion_idx` is an index into [`ActiveSuggestions::processed_suggestions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredItem {
    pub suggestion_idx: usize,
    pub score: i64,
    pub matching_indices: Vec<usize>,
}

pub struct ColumnInfo {
    pub global_col_idx: usize,
    pub items: Vec<(SuggestionFormatted, bool)>,
    pub width: usize,
    pub is_selected_col: bool,
}

#[derive(Debug)]
pub struct ActiveSuggestions {
    /// Raw completions waiting to be post-processed.  Drained from the front
    /// in chunks each time [`into_grid`] is called or fuzzy matching runs.
    unprocessed_suggestions: VecDeque<UnprocessedSuggestion>,
    /// Fully post-processed suggestions.  This is the only collection used by
    /// fuzzy matching, rendering, and acceptance logic.
    pub processed_suggestions: Vec<ProcessedSuggestion>,
    pub filtered_suggestions: Vec<FilteredItem>,
    /// 2-D position of the currently-selected suggestion within the grid as
    /// `(selected_col, selected_row)`. `None` means there is no active
    /// selection (used for auto-started suggestions).
    pub selected_coord: Option<(usize, usize)>,
    pub original_word_under_cursor: SubString,
    pub word_under_cursor: SubString,
    /// Number of suggestion rows per column as used in the last rendered
    /// grid.  Kept in sync by [`update_grid_size`].
    last_num_rows_per_col: usize,
    /// Number of columns that were actually visible in the last rendered
    /// grid.  Used to compute the scroll offset.
    pub last_num_visible_cols: usize,
    /// Total number of data columns (all candidates, regardless of how many
    /// fit in the viewport).  Used by the `TabCompletionMultiColAvailable`
    /// context variable so that left/right navigation is enabled whenever
    /// there is more than one column of candidates, even when the terminal
    /// is too narrow to show them all simultaneously.
    pub last_num_data_cols: usize,
    col_window_to_show: StatefulSlidingWindow,
    pub(crate) row_window_to_show: StatefulSlidingWindow,
    fuzzy_matcher: ArinaeMatcher,
    /// How long it took to generate the completions.
    pub load_time: std::time::Duration,
    pub comp_type: tab_completion_context::CompType,
    /// Whether this tab completion was auto-initiated.
    pub auto_started: bool,
}

impl ActiveSuggestions {
    pub fn new<'underlying_buffer>(
        builder: ActiveSuggestionsBuilder,
        word_under_cursor: SubString,
        load_time: std::time::Duration,
        auto_started: bool,
    ) -> Self {
        let ActiveSuggestionsBuilder {
            processed: processed_suggestions,
            unprocessed: unprocessed_suggestions,
            common_prefix: _,
            auto_accept_if_solo: _,
            insert_common_prefix: _,
            comp_type,
        } = builder;
        let sug_len = processed_suggestions.len() + unprocessed_suggestions.len();

        let mut active_sug = ActiveSuggestions {
            unprocessed_suggestions,
            processed_suggestions,
            filtered_suggestions: vec![],
            selected_coord: if auto_started { None } else { Some((0, 0)) },
            original_word_under_cursor: word_under_cursor.clone(),
            word_under_cursor: word_under_cursor.clone(),
            last_num_rows_per_col: 0,
            last_num_visible_cols: 0,
            last_num_data_cols: 0,
            col_window_to_show: StatefulSlidingWindow::new(0, 1, sug_len, Some(1)),
            row_window_to_show: StatefulSlidingWindow::new(0, 1, sug_len, Some(1)),
            fuzzy_matcher: ArinaeMatcher::new(skim::CaseMatching::Smart, true),
            load_time,
            comp_type,
            auto_started,
        };

        active_sug.update_fuzzy_filtered();
        active_sug
    }

    /// Move as many entries as fit within [`CHUNK_PROCESSING_TIMEOUT`] from
    /// `unprocessed_suggestions` into `processed_suggestions`, returning the
    /// range of newly-processed indices in `processed_suggestions`.
    fn process_chunk(&mut self) -> std::ops::Range<usize> {
        let max_to_process = self.unprocessed_suggestions.len();
        if max_to_process == 0 {
            let len = self.processed_suggestions.len();
            return len..len;
        }
        let start = self.processed_suggestions.len();
        // Pop from the front so the original ordering is preserved.  VecDeque
        // makes this O(1) per element.
        let start_time = std::time::Instant::now();
        for _ in 0..max_to_process {
            if let Some(raw) = self.unprocessed_suggestions.pop_front() {
                self.processed_suggestions.push(raw.into_processed());
            }
            if start_time.elapsed() > CHUNK_PROCESSING_TIMEOUT {
                break;
            }
        }
        start..self.processed_suggestions.len()
    }

    pub fn on_tab(&mut self, shift_tab: bool) {
        if shift_tab {
            self.on_up_arrow();
        } else {
            self.on_down_arrow();
        }
    }

    /// Return the flat (1-D) index of the currently-selected suggestion.
    pub fn current_1d_index(&self) -> Option<usize> {
        self.selected_coord.map(|(selected_col, selected_row)| {
            selected_col
                .saturating_mul(self.last_num_rows_per_col)
                .saturating_add(selected_row)
        })
    }

    /// Set the selected position from a flat (1-D) suggestion index.
    pub fn set_selected_by_idx(&mut self, idx: usize) {
        if self.last_num_rows_per_col == 0 {
            self.selected_coord = Some((0, idx));
        } else {
            self.selected_coord = Some((
                idx / self.last_num_rows_per_col,
                idx % self.last_num_rows_per_col,
            ));
        }
        self.clamp_selection();
    }

    /// Ensure the selected position refers to a valid suggestion.
    fn clamp_selection(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 {
            self.selected_coord = None;
            return;
        }
        let Some(current_idx) = self.current_1d_index() else {
            return;
        };
        // If the 2-D position points past the end of `filtered_suggestions`,
        // wrap to index 0.
        if current_idx >= n {
            self.selected_coord = Some((0, 0));
        }
    }

    pub fn on_right_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        let Some((selected_col, selected_row)) = self.selected_coord else {
            self.selected_coord = Some((0, 0));
            return;
        };
        let next_col = selected_col + 1;
        let next_idx = next_col * self.last_num_rows_per_col + selected_row;
        if next_idx < n {
            self.selected_coord = Some((next_col, selected_row));
        } else {
            // No suggestion exists at (selected_row, next_col) → wrap to col 0.
            self.selected_coord = Some((0, selected_row));
        }
    }

    pub fn on_left_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        let Some((selected_col, selected_row)) = self.selected_coord else {
            self.selected_coord = Some((0, 0));
            return;
        };
        if selected_col > 0 {
            self.selected_coord = Some((selected_col - 1, selected_row));
        } else {
            // Wrap to the last column.
            let last_col = (n - 1) / self.last_num_rows_per_col;
            // If (selected_row, last_col) is beyond the last suggestion,
            // clamp the row to the last item in that column.
            let idx = last_col * self.last_num_rows_per_col + selected_row;
            if idx >= n {
                self.selected_coord =
                    Some((last_col, n - 1 - last_col * self.last_num_rows_per_col));
            } else {
                self.selected_coord = Some((last_col, selected_row));
            }
        }
    }

    pub fn on_down_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        let Some((selected_col, selected_row)) = self.selected_coord else {
            self.selected_coord = Some((0, 0));
            return;
        };
        let next_row = selected_row + 1;
        let next_idx = selected_col * self.last_num_rows_per_col + next_row;
        if next_row < self.last_num_rows_per_col && next_idx < n {
            // Normal case: move down within the same column.
            self.selected_coord = Some((selected_col, next_row));
        } else {
            // At the bottom of this column: move to the top of the next column.
            let next_col = selected_col + 1;
            let next_col_start = next_col * self.last_num_rows_per_col;
            if next_col_start < n {
                self.selected_coord = Some((next_col, 0));
            } else {
                // Wrap to the very first suggestion.
                self.selected_coord = Some((0, 0));
            }
        }
    }

    pub fn on_up_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        let Some((selected_col, selected_row)) = self.selected_coord else {
            self.selected_coord = Some((0, 0));
            return;
        };
        if selected_row > 0 {
            // Normal case: move up within the same column.
            self.selected_coord = Some((selected_col, selected_row - 1));
        } else if selected_col > 0 {
            // At the top of this column: move to the bottom of the previous column.
            let prev_col = selected_col - 1;
            let col_start = prev_col * self.last_num_rows_per_col;
            let col_end = (col_start + self.last_num_rows_per_col).min(n);
            self.selected_coord = Some((prev_col, col_end - col_start - 1));
        } else {
            // At the top of column 0: wrap to the last populated row of the last column.
            let last_col = (n - 1) / self.last_num_rows_per_col;
            let col_start = last_col * self.last_num_rows_per_col;
            let col_end = (col_start + self.last_num_rows_per_col).min(n);
            self.selected_coord = Some((last_col, col_end - col_start - 1));
        }
    }

    pub fn on_page_up(&mut self) {
        if self.auto_started {
            // Move selection up by one page
            let n = self.filtered_suggestions.len();
            if n == 0 {
                return;
            }
            let current_idx = self.current_1d_index().unwrap_or(0);
            let page_size = self.row_window_to_show.window_size();
            let next_idx = current_idx.saturating_sub(page_size);
            self.set_selected_by_idx(next_idx);
        } else {
            // Move selection one column to the left
            self.on_left_arrow();
        }
    }

    pub fn on_page_down(&mut self) {
        if self.auto_started {
            // Move selection down by one page
            let n = self.filtered_suggestions.len();
            if n == 0 {
                return;
            }
            let current_idx = self.current_1d_index().unwrap_or(0);
            let page_size = self.row_window_to_show.window_size();
            let next_idx = (current_idx + page_size).min(n.saturating_sub(1));
            self.set_selected_by_idx(next_idx);
        } else {
            // Move selection one column to the right
            self.on_right_arrow();
        }
    }

    pub fn set_selected_by_scrollbar_pos(&mut self, relative_pos: f64) {
        let n = self.filtered_suggestions.len();
        if n == 0 {
            return;
        }

        let target_idx = (relative_pos * (n as f64)).floor() as usize;
        let target_idx = target_idx.min(n.saturating_sub(1));
        self.set_selected_by_idx(target_idx);
    }

    /// Return the portion of the suggestions grid that fits within the given
    /// terminal width, starting from column `col_offset`.
    pub fn into_grid(
        &mut self,
        max_rows: usize,
        max_width: usize,
        palette: &Palette,
        max_num_cols: Option<usize>,
    ) -> Vec<ColumnInfo> {
        // Try to convert another chunk of unprocessed suggestions before
        // rendering so they become available in subsequent frames.
        let newly_processed = self.process_chunk();
        if !newly_processed.is_empty() {
            self.update_fuzzy_filtered();
        }

        let selected_1d = self.current_1d_index();
        let selected_col = self.selected_coord.map(|(c, _)| c).unwrap_or(0);
        let n = self.filtered_suggestions.len();
        if n == 0 || max_rows == 0 {
            self.last_num_data_cols = 0;
            return vec![];
        }

        // Compute the animation frame index at ANIMATION_FRAME_FPS fps from the current wall-clock time.
        let frame_index: usize = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_millis() / (1000 / ANIMATION_FRAME_FPS as u128)) as usize)
            .unwrap_or(0);

        let mut grid: Vec<ColumnInfo> = vec![];
        let mut untruncated_total_width: usize = 0;

        let mut max_col_index = (n - 1) / max_rows;
        if let Some(max_cols) = max_num_cols {
            if max_cols == 0 {
                self.last_num_data_cols = 0;
                return vec![];
            }
            max_col_index = max_col_index.min(max_cols - 1);
        }
        self.last_num_data_cols = max_col_index + 1;

        self.col_window_to_show.update_max_index(max_col_index + 1);
        self.col_window_to_show
            .update_window_size(self.last_num_visible_cols.max(1));
        self.col_window_to_show.move_index_to(selected_col);

        // First round: try and fit as many columns as possible with their full untruncated width.
        for col_idx in self.col_window_to_show.get_window_range().start..=max_col_index {
            let start = col_idx * max_rows;
            let end = (start + max_rows).min(n);

            let col_items: Vec<(SuggestionFormatted, bool)> = (start..end)
                .map(|filtered_idx| {
                    let fi = &self.filtered_suggestions[filtered_idx];
                    let suggestion = &self.processed_suggestions[fi.suggestion_idx];

                    let formatted = SuggestionFormatted::new(
                        suggestion,
                        fi.suggestion_idx,
                        fi.matching_indices.clone(),
                        palette,
                        frame_index,
                    );
                    let is_selected_entry = selected_1d == Some(filtered_idx);

                    (formatted, is_selected_entry)
                })
                .collect();

            let untruncated_col_width = col_items
                .iter()
                .map(|(formatted, _)| formatted.display_width)
                .max()
                .unwrap_or(0);

            untruncated_total_width += if grid.is_empty() {
                untruncated_col_width
            } else {
                COLUMN_PADDING + untruncated_col_width
            };
            grid.push(ColumnInfo {
                global_col_idx: col_idx,
                items: col_items,
                width: untruncated_col_width,
                is_selected_col: col_idx == selected_col,
            });
            if untruncated_total_width > max_width && col_idx >= selected_col {
                break;
            }
        }
        // Second round, try not to truncate the selected column, and truncate other columns if needed to fit within max_width.
        let mut total_width = 0;

        let final_grid = grid
            .into_iter()
            // Truncation priority:
            // 1) selected col
            // 2) columns to the left of selected, moving outward
            // 3) columns to the right of selected, moving outward
            .sorted_by_key(|col_info| {
                let col_idx = col_info.global_col_idx;
                if col_idx == selected_col {
                    (0usize, 0usize)
                } else if col_idx < selected_col {
                    (1usize, selected_col - col_idx)
                } else {
                    (2usize, col_idx - selected_col)
                }
            })
            .enumerate()
            .map(|(num_cols_drawn_so_far, mut col)| {
                let padding_for_col = if num_cols_drawn_so_far == 0 {
                    0
                } else {
                    COLUMN_PADDING
                };

                if col.is_selected_col {
                    // Don't truncate the selected column, so count its full width.
                    col.width = col.width.min(max_width);
                } else {
                    const MIN_COL_WIDTH: usize = 10;

                    let truncated_col_width = if total_width + padding_for_col + col.width
                        > max_width
                    {
                        if max_width.saturating_sub(total_width + padding_for_col) > MIN_COL_WIDTH {
                            // We can still fit MIN_COL_WIDTH chars of this col so it should be alright.
                            max_width - total_width - padding_for_col
                        } else {
                            0
                        }
                    } else {
                        col.width
                    };
                    col.width = truncated_col_width;
                }

                total_width += col.width + padding_for_col;
                col
            })
            .filter(|col_info| col_info.width > 0)
            .sorted_by_key(|col_info| col_info.global_col_idx)
            .collect::<Vec<_>>();

        self.last_num_visible_cols = final_grid.len();

        self.last_num_rows_per_col = max_rows;
        final_grid
    }

    pub fn into_list(&mut self, max_rows: usize, palette: &Palette) -> Vec<SuggestionFormatted> {
        let newly_processed = self.process_chunk();
        if !newly_processed.is_empty() {
            self.update_fuzzy_filtered();
        }

        let selected_row = self.selected_coord.map(|(_, r)| r).unwrap_or(0);
        let n = self.filtered_suggestions.len();
        if n == 0 || max_rows == 0 {
            return vec![];
        }

        self.last_num_data_cols = 1;
        self.last_num_visible_cols = 1;
        self.last_num_rows_per_col = n.max(1);

        let frame_index: usize = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_millis() / (1000 / ANIMATION_FRAME_FPS as u128)) as usize)
            .unwrap_or(0);

        self.row_window_to_show.update_max_index(n);
        self.row_window_to_show.update_window_size(max_rows);
        self.row_window_to_show.move_index_to(selected_row);

        let window_range = self.row_window_to_show.get_window_range();

        window_range
            .map(|filtered_idx| {
                let fi = &self.filtered_suggestions[filtered_idx];
                let suggestion = &self.processed_suggestions[fi.suggestion_idx];
                SuggestionFormatted::new(
                    suggestion,
                    fi.suggestion_idx,
                    fi.matching_indices.clone(),
                    palette,
                    frame_index,
                )
            })
            .collect()
    }

    /// Number of suggestions currently shown (after fuzzy filtering).
    pub fn filtered_suggestions_len(&self) -> usize {
        self.filtered_suggestions.len()
    }

    pub fn all_suggestions_len(&self) -> usize {
        self.processed_suggestions.len() + self.unprocessed_suggestions.len()
    }

    /// Fuzzy match a single processed suggestion against the current pattern.
    fn fuzzy_match_for_processed(
        &self,
        idx: usize,
        sug: &ProcessedSuggestion,
    ) -> Option<FilteredItem> {
        let pattern_with_prefix = &self.word_under_cursor.s;
        let pattern = pattern_with_prefix
            .strip_prefix(&sug.prefix)
            .unwrap_or(pattern_with_prefix);

        // Try the fuzzy matcher first
        if let Some((score, indices)) = crate::content_utils::fuzzy_indices_with_threshold(
            &self.fuzzy_matcher,
            &sug.s,
            pattern,
            crate::content_utils::FuzzyMatchThreshold::High,
        ) {
            return Some(FilteredItem {
                score,
                suggestion_idx: idx,
                matching_indices: indices,
            });
        }

        const MAX_PATTERN_LENGTH: usize = 64;
        // I've noticed that when the pattern is very long, arinae matcher returns None.
        // So here we force it to return a dummy match.
        if pattern.len() > MAX_PATTERN_LENGTH {
            return Some(FilteredItem {
                score: 0,
                suggestion_idx: idx,
                matching_indices: Vec::new(),
            });
        }

        // No match: filter this out
        None
    }

    pub fn update_word_under_cursor(&mut self, new_word_under_cursor: &SubString) {
        self.word_under_cursor = new_word_under_cursor.clone();
        self.update_fuzzy_filtered();
    }

    /// Apply fuzzy search filtering to the suggestions based on the given pattern.
    fn update_fuzzy_filtered(&mut self) {
        let raw_pattern = self.word_under_cursor.s.as_str();
        log::debug!(
            "Applying fuzzy filter with raw_pattern {:?} on {} processed suggestions ({} still unprocessed)",
            raw_pattern,
            self.processed_suggestions.len(),
            self.unprocessed_suggestions.len()
        );

        self.filtered_suggestions = self
            .processed_suggestions
            .iter()
            .enumerate()
            .filter_map(|(idx, sug)| self.fuzzy_match_for_processed(idx, sug))
            // .inspect(|x| log::debug!("Fuzzy match result: idx={}, score={}, matching_indices={:?}", x.suggestion_idx, x.score, x.matching_indices))
            .collect();

        // Sort by score (descending - higher scores are better matches)
        self.filtered_suggestions
            .sort_by(|a, b| b.score.cmp(&a.score));

        // Reset selected position if needed
        if self.filtered_suggestions.is_empty() {
            self.selected_coord = None;
            return;
        }

        if self.current_1d_index().is_none() {
            if !self.auto_started {
                self.selected_coord = Some((0, 0));
            }
            return;
        }

        if self
            .current_1d_index()
            .is_some_and(|idx| idx >= self.filtered_suggestions.len())
        {
            self.selected_coord = if self.auto_started {
                None
            } else {
                Some((0, 0))
            };
        }
    }

    pub fn accept_selected_filtered_item(&mut self, buffer: &mut TextBuffer) {
        let selected_idx = if let Some(selected_idx) = self.current_1d_index() {
            selected_idx
        } else if self.filtered_suggestions.len() == 1 {
            0
        } else {
            log::warn!("No selected suggestion to accept");
            return;
        };

        let Some(filtered_item) = self.filtered_suggestions.get(selected_idx) else {
            log::warn!("No suggestion at selected index {}", selected_idx);
            return;
        };

        let Some(suggestion) = self.processed_suggestions.get(filtered_item.suggestion_idx) else {
            log::warn!(
                "Suggestion index {} out of bounds (processed len={})",
                filtered_item.suggestion_idx,
                self.processed_suggestions.len()
            );
            return;
        };

        if let Err(e) =
            buffer.replace_word_under_cursor(&suggestion.formatted(), &self.word_under_cursor)
        {
            log::error!("Failed to apply suggestion: {}", e);
        }
    }
}
