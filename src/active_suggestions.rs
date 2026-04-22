use crate::bash_funcs;
use crate::content_utils::{
    ansi_string_to_spans, easing_animation_frames, highlight_matching_indices,
    middle_truncate_spans, take_prefix_of_spans, ts_to_timeago_string_5chars, vec_spans_width,
};
use crate::cursor::CursorEasing;
use crate::palette::Palette;
use crate::stateful_sliding_window::StatefulSlidingWindow;
use crate::text_buffer::{SubString, TextBuffer};
use itertools::Itertools;
use ratatui::prelude::*;
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;
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
pub struct ProcssedSuggestion {
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
        suggestion: &ProcssedSuggestion,
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
                .map(|span| Span::styled(span.content, Palette::convert_to_selected(span.style)))
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
    fn split_no_tab() {
        let (text, frames) = split_completion_description("hello");
        assert_eq!(text, "hello");
        assert!(frames.is_empty());
    }

    #[test]
    fn split_single_tab_gives_one_frame() {
        let (text, frames) = split_completion_description("hello\tworld");
        assert_eq!(text, "hello");
        assert_eq!(frames, vec!["world".to_string()]);
    }

    #[test]
    fn split_multiple_tabs_give_multiple_frames() {
        let (text, frames) = split_completion_description("opt\tframe1\tframe2\tframe3");
        assert_eq!(text, "opt");
        assert_eq!(
            frames,
            vec![
                "frame1".to_string(),
                "frame2".to_string(),
                "frame3".to_string()
            ]
        );
    }

    #[test]
    fn match_text_raw_strips_description() {
        let item = MaybeProcessedSuggestion::Raw {
            raw_text: "git-commit\tRecord changes".to_string(),
            full_path: None,
            flags: crate::bash_funcs::CompletionFlags::default(),
            word_under_cursor: "git".to_string(),
        };
        assert_eq!(item.match_text(), "git-commit");
    }

    #[test]
    fn match_text_raw_no_tab_unchanged() {
        let item = MaybeProcessedSuggestion::Raw {
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
        let sug = ProcssedSuggestion::new("cmd", "", " ").with_description(
            SuggestionDescription::Animation(vec![vec![Span::raw("description text")]]),
        );
        assert_eq!(sug.formatted(), "cmd ");
        assert!(!sug.formatted().contains("description"));
    }

    #[test]
    fn description_frame_cycling() {
        let sug = ProcssedSuggestion::new("x", "", "").with_description(
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
        let sug = ProcssedSuggestion::new("abc", "", "").with_description(
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
        let sug = ProcssedSuggestion::new("hello", "", "");
        let palette = crate::palette::Palette::default();
        let fw = SuggestionFormatted::new(&sug, 0, vec![], &palette, 0).display_width;
        assert_eq!(fw, "hello".len());
    }

    #[test]
    fn last_mtime_description_max_width_is_5() {
        let sug = ProcssedSuggestion::new("file.txt", "", " ")
            .with_description(SuggestionDescription::LastMTime(0));
        assert_eq!(sug.description.max_width(), 5);
    }

    #[test]
    fn last_mtime_description_frame_is_nonempty() {
        let sug = ProcssedSuggestion::new("file.txt", "", " ")
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
        let sug = ProcssedSuggestion::new("foo", "", "");
        assert_eq!(sug.description, SuggestionDescription::Static(vec![]));
        assert_eq!(sug.description.max_width(), 0);
        assert!(sug.description.frame_at(0).is_none());
    }

    #[test]
    fn static_nonempty_description_frame() {
        let sug = ProcssedSuggestion::new("foo", "", "")
            .with_description(SuggestionDescription::Static(vec![Span::raw("hello")]));
        assert_eq!(sug.description.max_width(), 5);
        assert_eq!(sug.description.frame_at(0), Some(vec![Span::raw("hello")]));
        // frame_at is stable for any index
        assert_eq!(sug.description.frame_at(99), Some(vec![Span::raw("hello")]));
    }
}

impl ProcssedSuggestion {
    pub fn new<S: Into<String>, P: Into<String>, X: Into<String>>(
        s: S,
        prefix: P,
        suffix: X,
    ) -> Self {
        ProcssedSuggestion {
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
    ) -> Vec<ProcssedSuggestion> {
        suggestions
            .into_iter()
            .map(|s| {
                let new_suffix = if suffix == " " && s.ends_with(suffix) {
                    "".to_string()
                } else {
                    suffix.to_string()
                };
                ProcssedSuggestion::new(s, prefix.to_string(), new_suffix)
            })
            .collect()
    }
}

impl PartialOrd for ProcssedSuggestion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.s.partial_cmp(&other.s)
    }
}
impl Ord for ProcssedSuggestion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.s.cmp(&other.s)
    }
}

/// A completion that may or may not have been post-processed yet.
///
/// The `Ready` variant holds a fully processed [`Suggestion`] (used by code
/// paths that produce suggestions directly, e.g. env-var or tilde expansion).
///
/// The `Raw` variant holds a raw completion string from bash together with the
/// metadata needed to produce a [`Suggestion`] on demand via
/// [`post_process_completion`].  The expensive filesystem calls (`is_dir`,
/// `style_for_path`, `fully_expand_path`) are deferred until the item is
/// actually rendered or accepted.
#[derive(Debug, Clone)]
pub enum MaybeProcessedSuggestion {
    Ready(ProcssedSuggestion),
    Raw {
        raw_text: String,
        full_path: Option<PathBuf>,
        flags: bash_funcs::CompletionFlags,
        word_under_cursor: String,
    },
}

impl MaybeProcessedSuggestion {
    /// The text used for fuzzy matching and sorting.
    ///
    /// For `Raw` items, only the text up to the first tab character is considered
    /// (the remainder is a display-only description).
    pub fn match_text(&self) -> &str {
        match self {
            MaybeProcessedSuggestion::Ready(s) => &s.s,
            MaybeProcessedSuggestion::Raw { raw_text, .. } => raw_text
                .split_once('\t')
                .map(|(text, _)| text)
                .unwrap_or(raw_text),
        }
    }

    /// Produce the fully processed [`Suggestion`], running post-processing
    /// for `Raw` items or returning the existing suggestion for `Ready` items.
    pub fn to_suggestion(&mut self) -> ProcssedSuggestion {
        match self {
            MaybeProcessedSuggestion::Ready(s) => s.clone(),
            MaybeProcessedSuggestion::Raw {
                raw_text,
                full_path,
                flags,
                word_under_cursor,
            } => {
                let processed =
                    post_process_completion(raw_text, full_path.clone(), *flags, word_under_cursor);
                *self = MaybeProcessedSuggestion::Ready(processed.clone());
                processed
            }
        }
    }
}

/// Split a raw completion string into the completion text and description frames.
///
/// Any tab characters in `raw` serve as separators: the text before the first
/// tab is the value that gets inserted; each subsequent tab-separated segment
/// is one frame of the animated description.
pub(crate) fn split_completion_description(raw: &str) -> (&str, Vec<String>) {
    match raw.split_once('\t') {
        None => (raw, vec![]),
        Some((text, rest)) => {
            let frames: Vec<String> = rest.split('\t').map(|s| s.to_owned()).collect();
            (text, frames)
        }
    }
}

/// Post-process a single raw completion string into a [`Suggestion`].
///
/// This performs quoting, filesystem checks (`is_dir`, `style_for_path`), and
/// suffix computation.  Expensive for filenames due to syscalls; call lazily.
///
/// If `raw_sug` contains tab characters the text before the first tab is the
/// completion value; the remaining tab-separated segments are treated as
/// animation frames for the description (used when no higher-priority
/// description type applies).
pub fn post_process_completion(
    raw_sug: &str,
    mut path_to_use: Option<std::path::PathBuf>,
    comp_result_flags: bash_funcs::CompletionFlags,
    word_under_cursor: &str,
) -> ProcssedSuggestion {
    let (sug, desc_frames) = split_completion_description(raw_sug);
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

    let prefix = if comp_result_flags.filename_completion_desired {
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

    let quoted_no_prefix = quoted.strip_prefix(&prefix).unwrap_or(&quoted).to_string();

    // TODO: get rid of logging after verifying it works well
    log::debug!(
        "Post-processing completion: raw_sug={:?}, prefix={:?}, word_under_cursor={:?}, quoted_no_prefix={:?},suffix_char={:?}",
        raw_sug,
        prefix,
        word_under_cursor,
        quoted_no_prefix,
        &suffix_char
    );

    let style = path_to_use
        .as_ref()
        .and_then(|p| bash_funcs::style_for_path(p));
    let mtime = path_to_use
        .as_ref()
        .and_then(|p| p.metadata().ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    // Determine description type by priority:
    let description = if let Some(ts) = mtime {
        SuggestionDescription::LastMTime(ts)
    } else if let Some(easing) = CursorEasing::try_from_value_name(&sug) {
        SuggestionDescription::Animation(easing_animation_frames(easing))
    } else if !desc_frames.is_empty() {
        SuggestionDescription::Animation(
            desc_frames
                .into_iter()
                .map(|f| ansi_string_to_spans(&f))
                .collect(),
        )
    } else {
        SuggestionDescription::Static(vec![])
    };

    let suffix_str = suffix_char.map(|f| f.to_string()).unwrap_or_default();
    let suggestion = ProcssedSuggestion::new(quoted_no_prefix, prefix, &suffix_str)
        .with_description(description);
    match style {
        Some(s) => suggestion.with_style(s),
        None => suggestion,
    }
}

/// Lightweight entry in the filtered suggestion list.
///
/// Unlike [`SuggestionFormatted`], this stores only the index, score, and
/// fuzzy-match indices — no precomputed spans or display widths.  The
/// expensive rendering work is done on demand in [`ActiveSuggestions::into_grid`].
#[derive(Debug, Clone)]
struct FilteredItem {
    suggestion_idx: usize,
    score: i64,
    matching_indices: Vec<usize>,
    was_for_raw: bool,
}

pub struct ActiveSuggestions {
    all_maybe_processed_suggestions: Vec<MaybeProcessedSuggestion>,
    filtered_suggestions: Vec<FilteredItem>,
    /// 2-D position of the currently-selected suggestion within the grid.
    /// `selected_col * last_num_rows_per_col + selected_row` gives the 1-D
    /// index into `filtered_suggestions`.
    selected_row: usize,
    selected_col: usize,
    pub word_under_cursor: SubString,
    word_under_cursor_dequoted: String,
    /// Number of suggestion rows per column as used in the last rendered
    /// grid.  Kept in sync by [`update_grid_size`].
    last_num_rows_per_col: usize,
    /// Number of columns that were actually visible in the last rendered
    /// grid.  Used to compute the scroll offset.
    last_num_visible_cols: usize,
    col_window_to_show: StatefulSlidingWindow,
    fuzzy_matcher: ArinaeMatcher,
    /// How long it took to generate the completions.
    pub load_time: std::time::Duration,
    should_fuzzy_match: bool,
}

impl std::fmt::Debug for ActiveSuggestions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveSuggestions")
            .field(
                "all_suggestions_len",
                &self.all_maybe_processed_suggestions.len(),
            )
            .field("filtered_suggestions_len", &self.filtered_suggestions.len())
            .field("selected_row", &self.selected_row)
            .field("selected_col", &self.selected_col)
            .field("word_under_cursor", &self.word_under_cursor)
            .field(
                "word_under_cursor_dequoted",
                &self.word_under_cursor_dequoted,
            )
            .field("last_num_rows_per_col", &self.last_num_rows_per_col)
            .field("last_num_visible_cols", &self.last_num_visible_cols)
            .field("col_window_to_show", &self.col_window_to_show)
            .finish()
    }
}
pub struct ColumnInfo {
    pub global_col_idx: usize,
    pub items: Vec<(SuggestionFormatted, bool)>,
    pub width: usize,
    pub is_selected_col: bool,
}
impl ActiveSuggestions {
    pub fn new<'underlying_buffer>(
        suggestions: Vec<MaybeProcessedSuggestion>,
        word_under_cursor: SubString,
        load_time: std::time::Duration,
    ) -> Self {
        let filtered_suggestions = vec![];
        let sug_len = suggestions.len();

        let mut active_sug = ActiveSuggestions {
            all_maybe_processed_suggestions: suggestions,
            filtered_suggestions,
            selected_row: 0,
            selected_col: 0,
            word_under_cursor: word_under_cursor.clone(),
            word_under_cursor_dequoted: bash_funcs::dequoting_function_rust(&word_under_cursor.s),
            last_num_rows_per_col: 0,
            last_num_visible_cols: 0,
            col_window_to_show: StatefulSlidingWindow::new(0, 1, sug_len, Some(1)),
            fuzzy_matcher: ArinaeMatcher::new(skim::CaseMatching::Smart, true),
            load_time,
            should_fuzzy_match: true,
        };

        active_sug.update_word_under_cursor(word_under_cursor);

        if active_sug.filtered_suggestions_len() == 0 {
            active_sug.should_fuzzy_match = false;
        }
        active_sug
    }

    pub fn on_tab(&mut self, shift_tab: bool) {
        // Logic to handle tab key when active suggestions are present
        if shift_tab {
            self.on_up_arrow();
        } else {
            self.on_down_arrow();
        }
    }

    /// Return the flat (1-D) index of the currently-selected suggestion.
    fn current_1d_index(&self) -> usize {
        self.selected_col
            .saturating_mul(self.last_num_rows_per_col)
            .saturating_add(self.selected_row)
    }

    /// Set the selected position from a flat (1-D) suggestion index.
    fn set_from_1d_index(&mut self, idx: usize) {
        if self.last_num_rows_per_col == 0 {
            self.selected_row = idx;
            self.selected_col = 0;
        } else {
            self.selected_col = idx / self.last_num_rows_per_col;
            self.selected_row = idx % self.last_num_rows_per_col;
        }
        self.clamp_selection();
    }

    /// Ensure the selected position refers to a valid suggestion.
    fn clamp_selection(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 {
            self.selected_row = 0;
            self.selected_col = 0;
            return;
        }
        // If the 2-D position points past the end of `filtered_suggestions`,
        // wrap to index 0.
        if self.current_1d_index() >= n {
            self.selected_row = 0;
            self.selected_col = 0;
        }
    }

    // TODO arrow keys when not all suggestions are visible
    pub fn on_right_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        let next_col = self.selected_col + 1;
        let next_idx = next_col * self.last_num_rows_per_col + self.selected_row;
        if next_idx < n {
            self.selected_col = next_col;
        } else {
            // No suggestion exists at (selected_row, next_col) → wrap to col 0.
            self.selected_col = 0;
        }
    }

    pub fn on_left_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        if self.selected_col > 0 {
            self.selected_col -= 1;
        } else {
            // Wrap to the last column.
            let last_col = (n - 1) / self.last_num_rows_per_col;
            self.selected_col = last_col;
            // If (selected_row, last_col) is beyond the last suggestion,
            // clamp the row to the last item in that column.
            let idx = last_col * self.last_num_rows_per_col + self.selected_row;
            if idx >= n {
                self.selected_row = n - 1 - last_col * self.last_num_rows_per_col;
            }
        }
    }

    pub fn on_down_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        let next_row = self.selected_row + 1;
        let next_idx = self.selected_col * self.last_num_rows_per_col + next_row;
        if next_row < self.last_num_rows_per_col && next_idx < n {
            self.selected_row = next_row;
        } else {
            // Wrap to row 0 within this column.
            self.selected_row = 0;
        }
    }

    pub fn on_up_arrow(&mut self) {
        let n = self.filtered_suggestions.len();
        if n == 0 || self.last_num_rows_per_col == 0 {
            return;
        }
        if self.selected_row > 0 {
            self.selected_row -= 1;
        } else {
            // Wrap to the last populated row in this column.
            let col_start = self.selected_col * self.last_num_rows_per_col;
            let col_end = (col_start + self.last_num_rows_per_col).min(n);
            self.selected_row = col_end - col_start - 1;
        }
    }

    pub fn set_selected_by_idx(&mut self, idx: usize) {
        self.set_from_1d_index(idx);
    }

    /// Return the portion of the suggestions grid that fits within the given
    /// terminal width, starting from column `col_offset`.
    pub fn into_grid(
        &mut self,
        max_rows: usize,
        max_width: usize,
        palette: &Palette,
    ) -> Vec<ColumnInfo> {
        let selected_1d = self.current_1d_index();
        let n = self.filtered_suggestions.len();
        if n == 0 || max_rows == 0 {
            return vec![];
        }

        // Compute the animation frame index at ANIMATION_FRAME_FPS fps from the current wall-clock time.
        let frame_index: usize = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_millis() / (1000 / ANIMATION_FRAME_FPS as u128)) as usize)
            .unwrap_or(0);

        let mut grid: Vec<ColumnInfo> = vec![];
        let mut untruncated_total_width: usize = 0;

        let max_col_index = (n - 1) / max_rows;

        self.col_window_to_show.update_max_index(max_col_index + 1);
        self.col_window_to_show
            .update_window_size(self.last_num_visible_cols.max(1));
        self.col_window_to_show.move_index_to(self.selected_col);

        // First round: try and fit as many columns as possible with their full untruncated width.
        for col_idx in self.col_window_to_show.get_window_range().start..=max_col_index {
            // Build the column, processing each item lazily.
            let start = col_idx * max_rows;
            let end = (start + max_rows).min(n);

            let col_items: Vec<(SuggestionFormatted, bool)> = (start..end)
                .map(|filtered_idx| {
                    {
                        let fi: &FilteredItem = &self.filtered_suggestions[filtered_idx];
                        self.all_maybe_processed_suggestions[fi.suggestion_idx].to_suggestion();

                        let unprocessed_suggestion =
                            &self.all_maybe_processed_suggestions[fi.suggestion_idx];

                        if fi.was_for_raw {
                            if let Some(new_if) = self.fuzzy_match_for_suggestion(
                                fi.suggestion_idx,
                                unprocessed_suggestion,
                            ) {
                                self.filtered_suggestions[filtered_idx] = new_if;
                            }
                            self.filtered_suggestions[filtered_idx].was_for_raw = false;
                        }
                    }

                    let fi = &self.filtered_suggestions[filtered_idx];
                    let suggestion =
                        self.all_maybe_processed_suggestions[fi.suggestion_idx].to_suggestion();

                    let formatted = SuggestionFormatted::new(
                        &suggestion,
                        fi.suggestion_idx,
                        fi.matching_indices.clone(),
                        palette,
                        frame_index,
                    );
                    let is_selected_entry = filtered_idx == selected_1d;

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
                is_selected_col: col_idx == self.selected_col,
            });
            if untruncated_total_width > max_width && col_idx >= self.selected_col {
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
                if col_idx == self.selected_col {
                    (0usize, 0usize)
                } else if col_idx < self.selected_col {
                    (1usize, self.selected_col - col_idx)
                } else {
                    (2usize, col_idx - self.selected_col)
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

    /// Number of suggestions currently shown (after fuzzy filtering).
    pub fn filtered_suggestions_len(&self) -> usize {
        self.filtered_suggestions.len()
    }

    pub fn all_suggestions_len(&self) -> usize {
        self.all_maybe_processed_suggestions.len()
    }

    fn fuzzy_match_for_suggestion(
        &self,
        idx: usize,
        item: &MaybeProcessedSuggestion,
    ) -> Option<FilteredItem> {
        let was_for_raw = matches!(item, MaybeProcessedSuggestion::Raw { .. });

        if !self.should_fuzzy_match {
            return Some(FilteredItem {
                score: 0,
                suggestion_idx: idx,
                matching_indices: vec![],
                was_for_raw,
            });
        }

        let pattern = match item {
            MaybeProcessedSuggestion::Raw { .. } => &self.word_under_cursor_dequoted,
            MaybeProcessedSuggestion::Ready(sug) => {
                let pattern_with_prefix = &self.word_under_cursor.s;
                pattern_with_prefix
                    .strip_prefix(&sug.prefix)
                    .unwrap_or(pattern_with_prefix)
            }
        };

        self.fuzzy_matcher
            .fuzzy_indices(item.match_text(), pattern)
            .map(|(score, indices)| FilteredItem {
                score,
                suggestion_idx: idx,
                matching_indices: indices,
                was_for_raw,
            })
    }

    /// Apply fuzzy search filtering to the suggestions based on the given pattern.
    pub fn update_word_under_cursor(&mut self, new_word_under_cursor: SubString) {
        self.word_under_cursor = new_word_under_cursor.clone();

        let raw_pattern = self.word_under_cursor.s.as_str();
        let dequoted_pattern = bash_funcs::dequoting_function_rust(&self.word_under_cursor.s);
        log::debug!(
            "Applying fuzzy filter with raw_pattern {:?} and dequoted_pattern {:?} on {} suggestions",
            raw_pattern,
            dequoted_pattern,
            self.all_maybe_processed_suggestions.len()
        );

        // Score and filter suggestions using the stored matcher
        self.filtered_suggestions = self
            .all_maybe_processed_suggestions
            .iter()
            .enumerate()
            .filter_map(|(idx, item): (usize, &MaybeProcessedSuggestion)| {
                self.fuzzy_match_for_suggestion(idx, item)
            })
            .collect();

        // Sort by score (descending - higher scores are better matches)
        self.filtered_suggestions
            .sort_by(|a, b| b.score.cmp(&a.score));

        // Reset selected position if needed
        if self.current_1d_index() >= self.filtered_suggestions.len()
            && !self.filtered_suggestions.is_empty()
        {
            self.selected_row = 0;
            self.selected_col = 0;
        }
    }

    pub fn try_accept(mut self, buffer: &mut TextBuffer) -> Option<Self> {
        match self.all_maybe_processed_suggestions.as_mut_slice() {
            [] => {
                log::debug!("No completions found. all_maybe_processed_suggestions is empty");
                return Some(self);
            }
            [single_suggestion] => {
                let suggestion = single_suggestion.to_suggestion();
                self.accept_item(&suggestion, buffer);
                log::debug!(
                    "Only one completion found: auto-accepted '{:?}'",
                    suggestion
                );
                return None;
            }
            _ => {}
        }

        match self.filtered_suggestions.as_slice() {
            [] => {
                log::debug!("No completions found. filtered_suggestions is empty");
                log::debug!(
                    "all_maybe_processed_suggestions: {:#?}",
                    self.all_maybe_processed_suggestions
                );
                return Some(self);
            }
            [_filtered_item] => {
                self.accept_selected_filtered_item(buffer);
                log::debug!("Only one completion found for first word: auto-accepted");
                None
            }
            _ => Some(self),
        }
    }

    pub fn accept_selected_filtered_item(&mut self, buffer: &mut TextBuffer) {
        let filtered_item = match self.filtered_suggestions.get(self.current_1d_index()) {
            Some(s) => s,
            None => {
                log::warn!(
                    "No suggestion at selected index {}",
                    self.current_1d_index()
                );
                return;
            }
        };

        match self
            .all_maybe_processed_suggestions
            .get_mut(filtered_item.suggestion_idx)
        {
            Some(s) => {
                let suggestion = s.to_suggestion();
                self.accept_item(&suggestion, buffer);
            }
            None => {
                log::warn!(
                    "Suggestion index {} out of bounds (len={})",
                    filtered_item.suggestion_idx,
                    self.all_maybe_processed_suggestions.len()
                );
                return;
            }
        };
    }

    fn accept_item(&self, item: &ProcssedSuggestion, buffer: &mut TextBuffer) {
        if let Err(e) = buffer.replace_word_under_cursor(&item.formatted(), &self.word_under_cursor)
        {
            log::error!("Failed to apply suggestion: {}", e);
        }
    }
}
