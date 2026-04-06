use crate::bash_funcs;
use crate::palette::Palette;
use crate::stateful_sliding_window::StatefulSlidingWindow;
use crate::text_buffer::{SubString, TextBuffer};
use ratatui::prelude::*;
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;
use std::path::PathBuf;
use std::vec;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Number of whitespace characters inserted between adjacent columns in the
/// suggestions grid.
pub(crate) const COLUMN_PADDING: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub s: String,
    pub prefix: String,
    pub suffix: String,
    /// Optional display style (e.g. from LS_COLORS) applied when rendering in the completion list.
    pub style: Option<Style>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionFormatted {
    pub suggestion_idx: usize,
    pub display_width: usize,
    pub spans: Vec<Span<'static>>,
}

fn vec_spans_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|s| s.width()).sum()
}

fn take_prefix_of_spans(spans: &[Span<'static>], mut n: usize) -> Vec<Span<'static>> {
    if n == 0 {
        return vec![];
    }

    let mut out: Vec<Span<'static>> = Vec::new();

    for span in spans {
        if n == 0 {
            break;
        }
        let span_width = span.width();
        if span_width <= n {
            out.push(span.clone());
            n -= span_width;
        } else {
            span.content
                .graphemes(true)
                .take_while(|g| {
                    let g_width = g.width();
                    if g_width <= n {
                        n -= g_width;
                        true
                    } else {
                        false
                    }
                })
                .for_each(|g| out.push(Span::styled(g.to_owned(), span.style)));

            break;
        }
    }
    out
}

fn take_suffix_of_spans(spans: &[Span<'static>], mut n: usize) -> Vec<Span<'static>> {
    if n == 0 {
        return vec![];
    }

    let mut out: Vec<Span<'static>> = Vec::new();

    for span in spans.iter().rev() {
        if n == 0 {
            break;
        }
        let span_width = span.width();
        if span_width <= n {
            out.push(span.clone());
            n -= span_width;
        } else {
            span.content
                .graphemes(true)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .take_while(|g| {
                    let g_width = g.width();
                    if g_width <= n {
                        n -= g_width;
                        true
                    } else {
                        false
                    }
                })
                .for_each(|g| out.push(Span::styled(g.to_owned(), span.style)));

            break;
        }
    }
    out.reverse();
    out
}

/// Truncate `spans` to at most `max_chars` Unicode characters using middle
/// ellipsis (e.g. `"very_long_name"` → `"very…ame"`), preserving span styles.
fn middle_truncate_spans(spans: &[Span<'static>], max_chars: usize) -> Vec<Span<'static>> {
    let total = vec_spans_width(spans);
    if total <= max_chars {
        return spans.to_vec();
    }
    if max_chars == 0 {
        return vec![];
    }
    if max_chars == 1 {
        let style = spans.first().map(|s| s.style).unwrap_or_default();
        return vec![Span::styled("…".to_string(), style)];
    }

    // Reserve 1 char for the ellipsis.
    let keep = max_chars - 1;
    let left = keep / 2;
    let right = keep - left;

    let mut out: Vec<Span<'static>> = Vec::new();
    let mut left_spans = take_prefix_of_spans(spans, left);
    let right_spans = take_suffix_of_spans(spans, right);

    let ellipsis_style = left_spans
        .last()
        .map(|s| s.style)
        .or_else(|| right_spans.first().map(|s| s.style))
        .unwrap_or_default();

    out.append(&mut left_spans);
    out.push(Span::styled("…".to_string(), ellipsis_style));
    out.extend(right_spans);
    out
}

impl SuggestionFormatted {
    pub fn new(
        suggestion: &Suggestion,
        suggestion_idx: usize,
        matching_indices: Vec<usize>,
        palette: &Palette,
    ) -> Self {
        let base_style = suggestion.style.unwrap_or(palette.normal_text());
        let lines = palette.highlight_maching_indices(&suggestion.s, &matching_indices, base_style);

        SuggestionFormatted {
            suggestion_idx,
            display_width: suggestion.s.width(),
            spans: lines.into_iter().flat_map(|l| l.spans).collect(),
        }
    }

    /// Render this suggestion into a sequence of styled [`Span`]s.
    ///
    /// `col_width` is the visual width reserved for this cell (excluding any
    /// trailing padding).  When `col_width` is smaller than the suggestion
    /// text, middle-ellipsis truncation is applied so the text fits exactly
    /// within `col_width` characters.
    pub fn render(&self, col_width: usize, is_selected: bool) -> Vec<Span<'static>> {
        let mut spans: Vec<Span<'static>> = if col_width < self.display_width {
            middle_truncate_spans(&self.spans, col_width)
        } else {
            self.spans.clone()
        };

        if is_selected {
            spans = spans
                .into_iter()
                .map(|span| Span::styled(span.content, Palette::convert_to_selected(span.style)))
                .collect();
        }

        let rendered_len = vec_spans_width(&spans);

        let mut result = spans;
        result.push(Span::raw(
            " ".repeat(col_width.saturating_sub(rendered_len)),
        ));
        result
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

impl Suggestion {
    pub fn new<S: Into<String>, P: Into<String>, X: Into<String>>(
        s: S,
        prefix: P,
        suffix: X,
    ) -> Self {
        Suggestion {
            s: s.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
            style: None,
        }
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
    ) -> Vec<Suggestion> {
        suggestions
            .into_iter()
            .map(|s| {
                let new_suffix = if suffix == " " && s.ends_with(suffix) {
                    "".to_string()
                } else {
                    suffix.to_string()
                };
                Suggestion::new(s, prefix.to_string(), new_suffix)
            })
            .collect()
    }
}

impl PartialOrd for Suggestion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.s.partial_cmp(&other.s)
    }
}
impl Ord for Suggestion {
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
pub enum UnprocessedSuggestion {
    Ready(Suggestion),
    Raw {
        raw_text: String,
        expanded_path: Option<PathBuf>,
        flags: bash_funcs::CompletionFlags,
        word_under_cursor: String,
    },
}

impl UnprocessedSuggestion {
    /// The text used for fuzzy matching and sorting.
    pub fn match_text(&self) -> &str {
        match self {
            UnprocessedSuggestion::Ready(s) => &s.s,
            UnprocessedSuggestion::Raw { raw_text, .. } => raw_text,
        }
    }

    /// Produce the fully processed [`Suggestion`], running post-processing
    /// for `Raw` items or returning the existing suggestion for `Ready` items.
    pub fn to_suggestion(&self) -> Suggestion {
        match self {
            UnprocessedSuggestion::Ready(s) => s.clone(),
            UnprocessedSuggestion::Raw {
                raw_text,
                expanded_path,
                flags,
                word_under_cursor,
            } => post_process_completion(
                raw_text,
                expanded_path.as_deref(),
                *flags,
                word_under_cursor,
            ),
        }
    }
}

/// Post-process a single raw completion string into a [`Suggestion`].
///
/// This performs quoting, filesystem checks (`is_dir`, `style_for_path`), and
/// suffix computation.  Expensive for filenames due to syscalls; call lazily.
pub fn post_process_completion(
    sug: &str,
    path_to_use: Option<&std::path::Path>,
    comp_resultflags: bash_funcs::CompletionFlags,
    word_under_cursor: &str,
) -> Suggestion {
    let quoted = if comp_resultflags.filename_quoting_desired
        && comp_resultflags.filename_completion_desired
    {
        if !word_under_cursor.is_empty()
            && let Some(new_suffix) = sug.strip_prefix(word_under_cursor)
        {
            let quoted_suffix = bash_funcs::quote_function_rust(
                new_suffix,
                comp_resultflags.quote_type.unwrap_or_default(),
            );
            format!("{}{}", word_under_cursor, quoted_suffix)
        } else {
            bash_funcs::quote_function_rust(sug, comp_resultflags.quote_type.unwrap_or_default())
        }
    } else {
        sug.to_string()
    };

    let suffix = if comp_resultflags.no_suffix_desired {
        None
    } else if comp_resultflags.suffix_character == ' ' {
        if sug.ends_with(" ") { None } else { Some(' ') }
    } else {
        Some(comp_resultflags.suffix_character)
    };

    let (appended, suffix, ls_style) = if comp_resultflags.filename_completion_desired {
        let owned_path;
        let path = match path_to_use {
            Some(p) => p,
            None => {
                owned_path = std::path::PathBuf::from(bash_funcs::fully_expand_path(sug));
                &owned_path
            }
        };

        let appended = if path.is_dir() {
            (format!("{}/", quoted), None)
        } else {
            (quoted, suffix)
        };
        let ls_style = bash_funcs::style_for_path(path);
        (appended.0, appended.1, ls_style)
    } else {
        (quoted, suffix, None)
    };

    let suffix_str = suffix.map(|f| f.to_string()).unwrap_or_default();
    let suggestion = Suggestion::new(appended, "", &suffix_str);
    match ls_style {
        Some(style) => suggestion.with_style(style),
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
    matching_indices: Vec<usize>,
}

pub struct ActiveSuggestions {
    all_unprocessed_suggestions: Vec<UnprocessedSuggestion>,
    filtered_suggestions: Vec<FilteredItem>,
    /// 2-D position of the currently-selected suggestion within the grid.
    /// `selected_col * last_num_rows_per_col + selected_row` gives the 1-D
    /// index into `filtered_suggestions`.
    selected_row: usize,
    selected_col: usize,
    pub word_under_cursor: SubString,
    /// Number of suggestion rows per column as used in the last rendered
    /// grid.  Kept in sync by [`update_grid_size`].
    last_num_rows_per_col: usize,
    /// Number of columns that were actually visible in the last rendered
    /// grid.  Used to compute the scroll offset.
    last_num_visible_cols: usize,
    col_window_to_show: StatefulSlidingWindow,
    fuzzy_matcher: ArinaeMatcher,
}

impl std::fmt::Debug for ActiveSuggestions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveSuggestions")
            .field(
                "all_suggestions_len",
                &self.all_unprocessed_suggestions.len(),
            )
            .field("filtered_suggestions_len", &self.filtered_suggestions.len())
            .field("selected_row", &self.selected_row)
            .field("selected_col", &self.selected_col)
            .field("word_under_cursor", &self.word_under_cursor)
            .field("last_num_rows_per_col", &self.last_num_rows_per_col)
            .field("last_num_visible_cols", &self.last_num_visible_cols)
            .field("col_window_to_show", &self.col_window_to_show)
            .finish()
    }
}

impl ActiveSuggestions {
    pub fn try_new<'underlying_buffer>(
        suggestions: Vec<UnprocessedSuggestion>,
        word_under_cursor: &'underlying_buffer str,
        buffer: &'underlying_buffer TextBuffer,
    ) -> Option<Self> {
        let word_under_cursor = SubString::new(buffer.buffer(), word_under_cursor).ok()?;

        let filtered_suggestions = vec![];
        let sug_len = suggestions.len();

        let mut active_sug = ActiveSuggestions {
            all_unprocessed_suggestions: suggestions,
            filtered_suggestions,
            selected_row: 0,
            selected_col: 0,
            word_under_cursor: word_under_cursor.clone(),
            last_num_rows_per_col: 0,
            last_num_visible_cols: 0,
            col_window_to_show: StatefulSlidingWindow::new(0, 1, sug_len),
            fuzzy_matcher: ArinaeMatcher::new(skim::CaseMatching::Smart, true, false),
        };

        active_sug.apply_fuzzy_filter(word_under_cursor);
        Some(active_sug)
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
    ) -> Vec<(Vec<(SuggestionFormatted, bool)>, usize)> {
        let selected_1d = self.current_1d_index();
        let n = self.filtered_suggestions.len();
        if n == 0 || max_rows == 0 {
            return vec![];
        }

        let mut grid: Vec<(Vec<(SuggestionFormatted, bool)>, usize, bool)> = vec![];
        let mut total_width: usize = 0;

        let max_col_index = (n - 1) / max_rows;
        let mut first_col_len = max_rows;

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
                    let fi: &FilteredItem = &self.filtered_suggestions[filtered_idx];
                    let unprocessed_suggestion =
                        &self.all_unprocessed_suggestions[fi.suggestion_idx];
                    let suggestion = unprocessed_suggestion.to_suggestion();
                    let formatted = SuggestionFormatted::new(
                        &suggestion,
                        fi.suggestion_idx,
                        fi.matching_indices.clone(),
                        palette,
                    );
                    let is_selected = filtered_idx == selected_1d;
                    (formatted, is_selected)
                })
                .collect();

            let untruncated_col_width = col_items
                .iter()
                .map(|(formatted, _)| formatted.display_width)
                .max()
                .unwrap_or(0);

            total_width += if grid.is_empty() {
                untruncated_col_width
            } else {
                COLUMN_PADDING + untruncated_col_width
            };
            grid.push((
                col_items,
                untruncated_col_width,
                col_idx == self.selected_col,
            ));
            if total_width > max_width {
                break;
            }
        }

        // Second round,  try not to truncate the selected column, and truncate other columns if needed to fit within max_width.

        let mut local_col_idx_to_grid = vec![0; grid.len()];

        let mut total_width = 0;
        for local_idx in 0..local_col_idx_to_grid.len() {
            let is_selected = grid[local_idx].2;
            let untruncated_col_width = grid[local_idx].1;
            if is_selected {
                // Don't truncate the selected column, so count its full width.
                local_col_idx_to_grid[local_idx] = untruncated_col_width.min(max_width);
            } else {
                const MIN_COL_WIDTH: usize = 10;
                let truncated_col_width =
                    if total_width + COLUMN_PADDING + untruncated_col_width > max_width {
                        if max_width.saturating_sub(total_width + COLUMN_PADDING) > MIN_COL_WIDTH {
                            // We can still fit MIN_COL_WIDTH chars of this col so it should be alright.
                            max_width - total_width - COLUMN_PADDING
                        } else {
                            break;
                        }
                    } else {
                        untruncated_col_width
                    };
                local_col_idx_to_grid[local_idx] = truncated_col_width;
            }

            total_width += if local_idx == 0 {
                local_col_idx_to_grid[local_idx]
            } else {
                COLUMN_PADDING + local_col_idx_to_grid[local_idx]
            };
        }

        let mut final_grid: Vec<(Vec<(SuggestionFormatted, bool)>, usize)> = vec![];

        for (local_idx, (col_items, _, _)) in grid.into_iter().enumerate() {
            if local_idx == 0 {
                first_col_len = col_items.len();
            }
            let col_width = local_col_idx_to_grid[local_idx];
            if col_width == 0 {
                break;
            }
            final_grid.push((col_items, col_width));
        }

        self.last_num_visible_cols = final_grid.len();
        self.last_num_rows_per_col = max_rows.min(first_col_len);
        final_grid
    }

    /// Number of suggestions currently shown (after fuzzy filtering).
    pub fn filtered_suggestions_len(&self) -> usize {
        self.filtered_suggestions.len()
    }

    /// Apply fuzzy search filtering to the suggestions based on the given pattern.
    pub fn apply_fuzzy_filter(&mut self, new_word_under_cursor: SubString) {
        self.word_under_cursor = new_word_under_cursor.clone();

        // Score and filter suggestions using the stored matcher
        let mut scored: Vec<(i64, FilteredItem)> = self
            .all_unprocessed_suggestions
            .iter()
            .enumerate()
            .filter_map(|(idx, item): (usize, &UnprocessedSuggestion)| {
                self.fuzzy_matcher
                    .fuzzy_indices(item.match_text(), &new_word_under_cursor.s)
                    .map(|(score, indices)| {
                        (
                            score,
                            FilteredItem {
                                suggestion_idx: idx,
                                matching_indices: indices,
                            },
                        )
                    })
            })
            .collect();

        // Sort by score (descending - higher scores are better matches)
        scored.sort_by(|a, b| b.0.cmp(&a.0));

        self.filtered_suggestions = scored.into_iter().map(|(_score, item)| item).collect();

        // Reset selected position if needed
        if self.current_1d_index() >= self.filtered_suggestions.len()
            && !self.filtered_suggestions.is_empty()
        {
            self.selected_row = 0;
            self.selected_col = 0;
        }
    }

    pub fn try_accept(mut self, buffer: &mut TextBuffer) -> Option<Self> {
        match self.filtered_suggestions.as_slice() {
            [] => {
                log::debug!("No completions found");
                None
            }
            [_] => {
                self.accept_currently_selected(buffer);
                log::debug!("Only one completion found for first word: auto-accepted");
                None
            }
            _ => {
                log::debug!(
                    "Multiple completions available: {:?}",
                    self.filtered_suggestions
                );
                Some(self)
            }
        }
    }

    pub fn accept_currently_selected(&mut self, buffer: &mut TextBuffer) {
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

        let completion_item = match self
            .all_unprocessed_suggestions
            .get(filtered_item.suggestion_idx)
        {
            Some(s) => s,
            None => {
                log::warn!(
                    "Suggestion index {} out of bounds (len={})",
                    filtered_item.suggestion_idx,
                    self.all_unprocessed_suggestions.len()
                );
                return;
            }
        };

        let suggestion = completion_item.to_suggestion();
        if let Err(e) =
            buffer.replace_word_under_cursor(&suggestion.formatted(), &self.word_under_cursor)
        {
            log::error!("Failed to apply suggestion: {}", e);
        }
    }
}
