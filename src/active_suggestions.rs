use crate::palette::Palette;
use crate::text_buffer::{SubString, TextBuffer};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub s: String,
    pub prefix: String,
    pub suffix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionFormatted {
    pub suggestion_idx: usize,
    pub display_len: usize,
    pub spans: Vec<Span<'static>>,
    pub spans_selected: Vec<Span<'static>>,
}

impl SuggestionFormatted {
    pub fn new(
        suggestion: &Suggestion,
        suggestion_idx: usize,
        matching_indices: Vec<usize>,
    ) -> Self {
        let mut spans = Vec::new();
        let mut spans_selected = Vec::new();

        for (idx, ch) in suggestion.s.chars().enumerate() {
            let is_match = matching_indices.contains(&idx);
            let char_style = if is_match {
                Palette::matched_character()
            } else {
                Palette::normal_text()
            };
            let selected_style = if is_match {
                Palette::selected_matching_char()
            } else {
                Palette::selection_style()
            };

            spans.push(Span::styled(ch.to_string(), char_style));
            spans_selected.push(Span::styled(ch.to_string(), selected_style));
        }

        SuggestionFormatted {
            suggestion_idx,
            display_len: suggestion.s.len() + 2,
            spans,
            spans_selected,
        }
    }

    pub fn render(&self, col_width: usize, is_selected: bool) -> Vec<Span<'static>> {
        let mut spans = if is_selected {
            self.spans_selected.clone()
        } else {
            self.spans.clone()
        };

        if self.display_len < col_width {
            spans.push(Span::raw(" ".repeat(col_width - self.display_len)));
        }

        spans
    }
}

impl Suggestion {
    pub fn new(s: String, prefix: String, suffix: String) -> Self {
        Suggestion { s, prefix, suffix }
    }

    pub fn formatted(&self) -> String {
        format!("{}{}{}", self.prefix, self.s.replace(' ', "\\ "), self.suffix)
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
    selected_filtered_index: usize,
    pub word_under_cursor: SubString,
    last_grid_size: (usize, usize),
    fuzzy_matcher: SkimMatcherV2,
}

impl std::fmt::Debug for ActiveSuggestions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveSuggestions")
            .field("all_suggestions", &self.all_suggestions)
            .field("filtered_suggestions", &self.filtered_suggestions)
            .field("selected_filtered_index", &self.selected_filtered_index)
            .field("word_under_cursor", &self.word_under_cursor)
            .field("last_grid_size", &self.last_grid_size)
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
            selected_filtered_index: 0,
            word_under_cursor,
            last_grid_size: (0, 0),
            fuzzy_matcher: SkimMatcherV2::default(),
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

    pub fn sanitize_selected_index(&mut self, new_index: i32) {
        if self.filtered_suggestions.is_empty() {
            self.selected_filtered_index = 0;
            return;
        }
        self.selected_filtered_index =
            new_index.rem_euclid(self.filtered_suggestions.len() as i32) as usize;
    }

    // TODO arrow keys when not all suggestions are visible
    pub fn on_right_arrow(&mut self) {
        let new_idx: i32 = self.selected_filtered_index as i32 + self.last_grid_size.0 as i32;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_left_arrow(&mut self) {
        let new_idx: i32 = self.selected_filtered_index as i32 - self.last_grid_size.0 as i32;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_down_arrow(&mut self) {
        let new_idx: i32 = self.selected_filtered_index as i32 + 1;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_up_arrow(&mut self) {
        let new_idx: i32 = self.selected_filtered_index as i32 - 1;
        self.sanitize_selected_index(new_idx);
    }

    pub fn set_selected_by_idx(&mut self, idx: usize) {
        self.sanitize_selected_index(idx as i32);
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (usize, &SuggestionFormatted, bool)> {
        // prefix and suffix aren't shown in the suggestion list
        // but are applied when the suggestion is accepted
        self.filtered_suggestions
            .iter()
            .enumerate()
            .map(|(idx, formatted_suggestion)| {
                (
                    idx,
                    formatted_suggestion,
                    idx == self.selected_filtered_index,
                )
            })
    }

    pub fn into_grid(
        &self,
        rows: usize,
        cols: usize,
    ) -> Vec<(Vec<(&SuggestionFormatted, bool)>, usize)> {
        // Show as many suggestions as will fit in the given rows and columns
        // Each column should be the same width, based on the longest suggestion
        let mut grid = vec![];
        let mut current_col = vec![];
        let mut col_width = 1;
        let mut total_columns = 0;

        for (filtered_idx, formatted, is_selected) in self.iter() {
            current_col.push((formatted, is_selected));
            col_width = col_width.max(formatted.display_len); // +2 for padding // TODO truncate very long suggestions
            if (filtered_idx + 1) % rows == 0 {
                if total_columns + col_width > cols {
                    break;
                }
                grid.push((current_col, col_width));
                total_columns += col_width;
                current_col = vec![];
                col_width = 1;
            }
        }

        if !current_col.is_empty() && total_columns + col_width <= cols {
            grid.push((current_col, col_width));
        }
        grid
    }

    pub fn update_grid_size(&mut self, rows: usize, cols: usize) {
        self.last_grid_size = (rows, cols);
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

        // Reset selected index if needed
        if self.selected_filtered_index >= self.filtered_suggestions.len()
            && !self.filtered_suggestions.is_empty()
        {
            self.selected_filtered_index = 0;
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
        let formatted_completion = match self.filtered_suggestions.get(self.selected_filtered_index)
        {
            Some(s) => s,
            None => {
                log::warn!(
                    "No suggestion at selected index {}",
                    self.selected_filtered_index
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
