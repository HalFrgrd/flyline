use crate::palette::Palette;
use crate::text_buffer::{SubString, TextBuffer};
use ratatui::prelude::*;
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;

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
    pub display_len: usize,
    pub spans: Vec<Span<'static>>,
}

/// Truncate `text` to at most `max_chars` Unicode characters using middle
/// ellipsis (e.g. `"very_long_name"` → `"very…ame"`).
fn middle_truncate(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    // Reserve 1 char for the ellipsis.
    let keep = max_chars - 1;
    let left = keep / 2;
    let right = keep - left;
    let left_str: String = chars[..left].iter().collect();
    let right_str: String = chars[chars.len() - right..].iter().collect();
    format!("{}…{}", left_str, right_str)
}

impl SuggestionFormatted {
    pub fn new(
        suggestion: &Suggestion,
        suggestion_idx: usize,
        matching_indices: Vec<usize>,
    ) -> Self {
        let base_style = suggestion.style.unwrap_or(Palette::normal_text());
        let lines =
            Palette::highlight_maching_indices(&suggestion.s, &matching_indices, base_style);

        SuggestionFormatted {
            suggestion_idx,
            display_len: suggestion.s.len() + 2,
            spans: lines.into_iter().flat_map(|l| l.spans).collect(),
        }
    }

    /// Render this suggestion into a sequence of styled [`Span`]s.
    ///
    /// `col_width` is the visual width reserved for this cell (including
    /// trailing padding).  When `col_width` is smaller than the suggestion
    /// text, middle-ellipsis truncation is applied so the text fits exactly
    /// within `col_width` characters.
    pub fn render(&self, col_width: usize, is_selected: bool) -> Vec<Span<'static>> {
        let text_len = self.display_len.saturating_sub(2);

        // If the column is narrower than the suggestion text, rebuild the
        // spans from a middle-truncated version of the plain text.
        let (spans, rendered_len): (Vec<Span<'static>>, usize) = if col_width < text_len {
            let plain: String = self.spans.iter().map(|s| s.content.as_ref()).collect();
            let truncated = middle_truncate(&plain, col_width);
            let len = truncated.chars().count();
            let base_style = self
                .spans
                .first()
                .map(|s| s.style)
                .unwrap_or(Style::default());
            let style = if is_selected {
                Palette::convert_to_selected(base_style)
            } else {
                base_style
            };
            (vec![Span::styled(truncated, style)], len)
        } else {
            let spans = if is_selected {
                self.spans
                    .iter()
                    .map(|span| {
                        Span::styled(
                            span.content.clone(),
                            Palette::convert_to_selected(span.style),
                        )
                    })
                    .collect()
            } else {
                self.spans.clone()
            };
            (spans, text_len)
        };

        let mut result = spans;
        result.push(Span::raw(
            " ".repeat(col_width.saturating_sub(rendered_len)),
        ));
        result
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

pub struct ActiveSuggestions {
    all_suggestions: Vec<Suggestion>,
    filtered_suggestions: Vec<SuggestionFormatted>,
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
    /// Index of the first column that is shown during rendering (0-based).
    /// Non-zero when the selected column has scrolled out of the default view.
    col_scroll_offset: usize,
    fuzzy_matcher: ArinaeMatcher,
}

impl std::fmt::Debug for ActiveSuggestions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveSuggestions")
            .field("all_suggestions", &self.all_suggestions)
            .field("filtered_suggestions", &self.filtered_suggestions)
            .field("selected_row", &self.selected_row)
            .field("selected_col", &self.selected_col)
            .field("word_under_cursor", &self.word_under_cursor)
            .field("last_num_rows_per_col", &self.last_num_rows_per_col)
            .field("last_num_visible_cols", &self.last_num_visible_cols)
            .field("col_scroll_offset", &self.col_scroll_offset)
            .finish()
    }
}

impl ActiveSuggestions {
    pub fn try_new<'underlying_buffer>(
        suggestions: Vec<Suggestion>,
        word_under_cursor: &'underlying_buffer str,
        buffer: &'underlying_buffer TextBuffer,
    ) -> Option<Self> {
        let word_under_cursor = SubString::new(buffer.buffer(), word_under_cursor).ok()?;

        let filtered_suggestions = suggestions
            .iter()
            .enumerate()
            .map(|(idx, s)| SuggestionFormatted::new(s, idx, vec![]))
            .collect();

        Some(ActiveSuggestions {
            all_suggestions: suggestions,
            filtered_suggestions,
            selected_row: 0,
            selected_col: 0,
            word_under_cursor,
            last_num_rows_per_col: 0,
            last_num_visible_cols: 0,
            col_scroll_offset: 0,
            fuzzy_matcher: ArinaeMatcher::new(skim::CaseMatching::Smart, true),
        })
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
        self.adjust_col_scroll_offset();
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

    /// Adjust `col_scroll_offset` so that `selected_col` is always within
    /// the visible column range `[col_scroll_offset, col_scroll_offset +
    /// last_num_visible_cols)`.
    fn adjust_col_scroll_offset(&mut self) {
        if self.last_num_visible_cols == 0 {
            return;
        }
        if self.selected_col < self.col_scroll_offset {
            self.col_scroll_offset = self.selected_col;
        } else if self.selected_col >= self.col_scroll_offset + self.last_num_visible_cols {
            self.col_scroll_offset = self.selected_col + 1 - self.last_num_visible_cols;
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
            self.col_scroll_offset = 0;
            // Row 0 of col 0 always exists (n > 0).
        }
        self.adjust_col_scroll_offset();
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
        self.adjust_col_scroll_offset();
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

    pub fn iter(&self) -> impl Iterator<Item = (usize, &SuggestionFormatted, bool)> {
        // prefix and suffix aren't shown in the suggestion list
        // but are applied when the suggestion is accepted
        let selected_idx = self.current_1d_index();
        self.filtered_suggestions
            .iter()
            .enumerate()
            .map(move |(idx, formatted_suggestion)| {
                (idx, formatted_suggestion, idx == selected_idx)
            })
    }

    /// Return the portion of the suggestions grid that fits within the given
    /// terminal width, starting from column `col_offset`.
    ///
    /// Each element of the returned `Vec` is `(column_items, col_width)`.
    /// For the very first visible column, `col_width` is capped at `cols`
    /// so that an unusually long suggestion triggers middle-ellipsis
    /// rendering inside [`SuggestionFormatted::render`] rather than being
    /// dropped entirely.
    pub fn into_grid(
        &self,
        rows: usize,
        cols: usize,
        col_offset: usize,
    ) -> Vec<(Vec<(&SuggestionFormatted, bool)>, usize)> {
        // Show as many suggestions as will fit in the given rows and columns
        // Each column should be the same width, based on the longest suggestion
        let mut grid: Vec<(Vec<(&SuggestionFormatted, bool)>, usize)> = vec![];
        let mut current_col = vec![];
        let mut col_width = 1;
        let mut total_columns = 0;
        let mut abs_col_idx: usize = 0; // absolute column index (before offset)

        /// Push `col` to `grid` and update `total_columns`.  For the first
        /// visible column the width is capped at the terminal width to
        /// trigger middle-ellipsis rendering.  Returns `false` when the
        /// column would overflow the terminal and should be discarded.
        fn push_col<'a>(
            grid: &mut Vec<(Vec<(&'a SuggestionFormatted, bool)>, usize)>,
            total_columns: &mut usize,
            col: Vec<(&'a SuggestionFormatted, bool)>,
            col_width: usize,
            term_cols: usize,
        ) -> bool {
            let is_first = grid.is_empty();
            if is_first {
                // Cap width so long suggestions are truncated rather than dropped.
                let effective = col_width.min(term_cols);
                grid.push((col, effective));
                *total_columns += effective;
                true
            } else if *total_columns + col_width > term_cols {
                false
            } else {
                grid.push((col, col_width));
                *total_columns += col_width;
                true
            }
        }

        for (filtered_idx, formatted, is_selected) in self.iter() {
            current_col.push((formatted, is_selected));
            col_width = col_width.max(formatted.display_len); // +2 for padding // TODO truncate very long suggestions
            if (filtered_idx + 1) % rows == 0 {
                // Skip columns before col_offset.
                if abs_col_idx < col_offset {
                    abs_col_idx += 1;
                    current_col = vec![];
                    col_width = 1;
                    continue;
                }
                let col = std::mem::take(&mut current_col);
                let added = push_col(&mut grid, &mut total_columns, col, col_width, cols);
                abs_col_idx += 1;
                col_width = 1;
                if !added {
                    break;
                }
            }
        }

        // Handle the last, possibly-incomplete column.
        if !current_col.is_empty() && abs_col_idx >= col_offset {
            push_col(&mut grid, &mut total_columns, current_col, col_width, cols);
        }
        grid
    }

    pub fn update_grid_size(&mut self, num_rows_for_suggestions: usize, num_visible_cols: usize) {
        self.last_num_rows_per_col = num_rows_for_suggestions;
        self.last_num_visible_cols = num_visible_cols;
        // Keep the selected column visible.
        self.adjust_col_scroll_offset();
    }

    /// Number of suggestions currently shown (after fuzzy filtering).
    pub fn filtered_suggestions_len(&self) -> usize {
        self.filtered_suggestions.len()
    }

    /// Column index from which the rendered grid should start (scroll offset).
    pub fn col_scroll_offset(&self) -> usize {
        self.col_scroll_offset
    }

    /// Apply fuzzy search filtering to the suggestions based on the given pattern.
    pub fn apply_fuzzy_filter(&mut self, new_word_under_cursor: SubString) {
        self.word_under_cursor = new_word_under_cursor.clone();

        // Score and filter suggestions using the stored matcher
        let mut scored: Vec<(i64, SuggestionFormatted)> = self
            .all_suggestions
            .iter()
            .enumerate()
            .filter_map(|(idx, suggestion)| {
                self.fuzzy_matcher
                    .fuzzy_indices(&suggestion.s, &new_word_under_cursor.s)
                    .map(|(score, indices)| {
                        (score, SuggestionFormatted::new(suggestion, idx, indices))
                    })
            })
            .collect();

        // Sort by score (descending - higher scores are better matches)
        scored.sort_by(|a, b| b.0.cmp(&a.0));

        self.filtered_suggestions = scored
            .into_iter()
            .map(|(_score, formatted)| formatted)
            .collect();

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
        let formatted_completion = match self.filtered_suggestions.get(self.current_1d_index()) {
            Some(s) => s,
            None => {
                log::warn!(
                    "No suggestion at selected index {}",
                    self.current_1d_index()
                );
                return;
            }
        };

        let suggestion = match self
            .all_suggestions
            .get(formatted_completion.suggestion_idx)
        {
            Some(s) => s,
            None => {
                log::warn!(
                    "Suggestion index {} out of bounds (len={})",
                    formatted_completion.suggestion_idx,
                    self.all_suggestions.len()
                );
                return;
            }
        };

        if let Err(e) =
            buffer.replace_word_under_cursor(&suggestion.formatted(), &self.word_under_cursor)
        {
            log::error!("Failed to apply suggestion: {}", e);
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_fuzzy_filter_empty_pattern() {
//         let buffer = TextBuffer::new("git");

//         let suggestions = vec![
//             Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
//         ];

//         let word = buffer.buffer();
//         let mut active = ActiveSuggestions::try_new(suggestions.clone(), word, &buffer).unwrap();

//         // Empty pattern should keep all suggestions
//         assert!(active.apply_fuzzy_filter(""));
//         assert_eq!(active.all_suggestions.len(), 3);
//     }

//     #[test]
//     fn test_fuzzy_filter_exact_match() {
//         let buffer = TextBuffer::new("git co");

//         let suggestions = vec![
//             Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("config".to_string(), "".to_string(), "".to_string()),
//         ];

//         // Extract "co" from buffer
//         let word = &buffer.buffer()[4..6];
//         let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

//         // "co" should match commit, checkout, clone, config
//         assert!(active.apply_fuzzy_filter("co"));
//         assert!(active.all_suggestions.len() >= 3);

//         // All matched suggestions should contain relevant characters
//         for suggestion in &active.all_suggestions {
//             assert!(suggestion.s.contains('c') && suggestion.s.contains('o'));
//         }
//     }

//     #[test]
//     fn test_fuzzy_filter_partial_match() {
//         let buffer = TextBuffer::new("git chk");

//         let suggestions = vec![
//             Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("cherry-pick".to_string(), "".to_string(), "".to_string()),
//         ];

//         // Extract "chk" from buffer
//         let word = &buffer.buffer()[4..7];
//         let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

//         // "chk" should fuzzy match "checkout" (c-h-eckout)
//         assert!(active.apply_fuzzy_filter("chk"));
//         assert!(active.all_suggestions.len() >= 1);

//         // checkout should be in the results
//         assert!(active.all_suggestions.iter().any(|s| s.s == "checkout"));
//     }

//     #[test]
//     fn test_fuzzy_filter_no_matches() {
//         let buffer = TextBuffer::new("git xyz");

//         let suggestions = vec![
//             Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
//         ];

//         // Extract "xyz" from buffer
//         let word = &buffer.buffer()[4..7];
//         let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

//         // "xyz" should not match any git commands
//         assert!(!active.apply_fuzzy_filter("xyz"));
//         assert_eq!(active.all_suggestions.len(), 0);
//     }

//     #[test]
//     fn test_fuzzy_filter_resets_selected_index() {
//         let buffer = TextBuffer::new("git c");

//         let suggestions = vec![
//             Suggestion::new("add".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
//         ];

//         // Extract "c" from buffer
//         let word = &buffer.buffer()[4..5];
//         let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

//         // Move selection to last item
//         active.selected_index = 3;

//         // Filter to only "commit", "checkout", "clone" (3 items)
//         assert!(active.apply_fuzzy_filter("c"));

//         // Selected index should be reset to 0 since old index 3 might be out of bounds
//         assert!(active.selected_index < active.all_suggestions.len());
//     }

//     #[test]
//     fn test_fuzzy_filter_prioritizes_better_matches() {
//         let buffer = TextBuffer::new("git ch");

//         let suggestions = vec![
//             Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("cherry-pick".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
//             Suggestion::new("branch".to_string(), "".to_string(), "".to_string()),
//         ];

//         // Extract "ch" from buffer
//         let word = &buffer.buffer()[4..6];
//         let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

//         // "ch" should match cherry-pick and checkout better than others
//         assert!(active.apply_fuzzy_filter("ch"));

//         // The top matches should start with "ch"
//         assert!(active.all_suggestions[0].s.starts_with("ch"));
//     }
// }
