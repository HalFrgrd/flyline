use crate::text_buffer::{SubString, TextBuffer};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub s: String,
    pub prefix: String,
    pub suffix: String,
}

impl Suggestion {
    pub fn new(s: String, prefix: String, suffix: String) -> Self {
        Suggestion { s, prefix, suffix }
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
    suggestions: Vec<Suggestion>,
    fuzzy_filtered_suggestions: Vec<Suggestion>,
    selected_fuzzy_index: usize,
    pub word_under_cursor: SubString,
    last_grid_size: (usize, usize),
    fuzzy_matcher: SkimMatcherV2,
}

impl std::fmt::Debug for ActiveSuggestions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveSuggestions")
            .field("suggestions", &self.suggestions)
            .field(
                "fuzzy_filtered_suggestions",
                &self.fuzzy_filtered_suggestions,
            )
            .field("selected_fuzzy_index", &self.selected_fuzzy_index)
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

        let fuzzy_filtered_suggestions = suggestions.clone();

        Some(ActiveSuggestions {
            suggestions,
            fuzzy_filtered_suggestions,
            selected_fuzzy_index: 0,
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
        self.selected_fuzzy_index =
            new_index.rem_euclid(self.fuzzy_filtered_suggestions.len() as i32) as usize;
    }

    // TODO arrow keys when not all suggestions are visible
    pub fn on_right_arrow(&mut self) {
        let new_idx: i32 = self.selected_fuzzy_index as i32 + self.last_grid_size.0 as i32;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_left_arrow(&mut self) {
        let new_idx: i32 = self.selected_fuzzy_index as i32 - self.last_grid_size.0 as i32;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_down_arrow(&mut self) {
        let new_idx: i32 = self.selected_fuzzy_index as i32 + 1;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_up_arrow(&mut self) {
        let new_idx: i32 = self.selected_fuzzy_index as i32 - 1;
        self.sanitize_selected_index(new_idx);
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, bool)> {
        // prefix and suffix aren't shown in the suggestion list
        // but are applied when the suggestion is accepted
        self.fuzzy_filtered_suggestions
            .iter()
            .enumerate()
            .map(|(idx, suggestion)| (suggestion.s.as_str(), idx == self.selected_fuzzy_index))
    }

    pub fn into_grid(&self, rows: usize, cols: usize) -> Vec<(Vec<(&str, bool)>, usize)> {
        // Show as many suggestions as will fit in the given rows and columns
        // Each column should be the same width, based on the longest suggestion
        let mut grid = vec![];
        let mut current_col = vec![];
        let mut col_width = 1;
        let mut total_columns = 0;

        for (i, (s, is_selected)) in self.iter().enumerate() {
            current_col.push((s, is_selected));
            col_width = col_width.max(s.len() + 2); // +2 for padding // TODO truncate very long suggestions
            if (i + 1) % rows == 0 {
                if total_columns + col_width > cols {
                    break;
                }
                grid.push((current_col, col_width));
                total_columns += col_width;
                current_col = vec![];
                col_width = 1;
            }
        }
        // TODO say there are more if it doesnt fit

        if !current_col.is_empty() && total_columns + col_width <= cols {
            grid.push((current_col, col_width));
        }
        grid
    }

    pub fn update_grid_size(&mut self, rows: usize, cols: usize) {
        self.last_grid_size = (rows, cols);
    }

    /// Apply fuzzy search filtering to the suggestions based on the given pattern.
    pub fn apply_fuzzy_filter(&mut self, pattern: &str) {
        // Score and filter suggestions using the stored matcher
        let mut scored: Vec<(usize, i64)> = self
            .suggestions
            .iter()
            .enumerate()
            .filter_map(|(idx, suggestion)| {
                self.fuzzy_matcher
                    .fuzzy_match(&suggestion.s, pattern)
                    .map(|score| (idx, score))
            })
            .collect();

        // Sort by score (descending - higher scores are better matches)
        scored.sort_by(|a, b| b.1.cmp(&a.1));

        // Extract matching suggestions in score order
        // This drains and rebuilds the vec, avoiding clone but requires one allocation
        self.fuzzy_filtered_suggestions = scored
            .into_iter()
            .filter_map(|(idx, _)| self.suggestions.get(idx).cloned())
            .collect();

        // Reset selected index if needed
        if self.selected_fuzzy_index >= self.fuzzy_filtered_suggestions.len()
            && !self.fuzzy_filtered_suggestions.is_empty()
        {
            self.selected_fuzzy_index = 0;
        }
    }

    pub fn try_accept(mut self, buffer: &mut TextBuffer) -> Option<Self> {
        match self.fuzzy_filtered_suggestions.as_slice() {
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
                    self.fuzzy_filtered_suggestions
                );
                Some(self)
            }
        }
    }

    pub fn accept_currently_selected(&mut self, buffer: &mut TextBuffer) {
        if let Some(completion) = self
            .fuzzy_filtered_suggestions
            .get(self.selected_fuzzy_index)
        {
            if let Err(e) =
                buffer.replace_word_under_cursor(&completion.formatted(), &self.word_under_cursor)
            {
                log::error!("Error during tab completion: {}", e);
            }
        } else {
            log::error!(
                "Tried to accept suggestion at index {}, but only {} suggestions are available",
                self.selected_fuzzy_index,
                self.fuzzy_filtered_suggestions.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_filter_empty_pattern() {
        let buffer = TextBuffer::new("git");

        let suggestions = vec![
            Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
        ];

        let word = buffer.buffer();
        let mut active = ActiveSuggestions::try_new(suggestions.clone(), word, &buffer).unwrap();

        // Empty pattern should keep all suggestions
        assert!(active.apply_fuzzy_filter(""));
        assert_eq!(active.suggestions.len(), 3);
    }

    #[test]
    fn test_fuzzy_filter_exact_match() {
        let buffer = TextBuffer::new("git co");

        let suggestions = vec![
            Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("config".to_string(), "".to_string(), "".to_string()),
        ];

        // Extract "co" from buffer
        let word = &buffer.buffer()[4..6];
        let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

        // "co" should match commit, checkout, clone, config
        assert!(active.apply_fuzzy_filter("co"));
        assert!(active.suggestions.len() >= 3);

        // All matched suggestions should contain relevant characters
        for suggestion in &active.suggestions {
            assert!(suggestion.s.contains('c') && suggestion.s.contains('o'));
        }
    }

    #[test]
    fn test_fuzzy_filter_partial_match() {
        let buffer = TextBuffer::new("git chk");

        let suggestions = vec![
            Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("cherry-pick".to_string(), "".to_string(), "".to_string()),
        ];

        // Extract "chk" from buffer
        let word = &buffer.buffer()[4..7];
        let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

        // "chk" should fuzzy match "checkout" (c-h-eckout)
        assert!(active.apply_fuzzy_filter("chk"));
        assert!(active.suggestions.len() >= 1);

        // checkout should be in the results
        assert!(active.suggestions.iter().any(|s| s.s == "checkout"));
    }

    #[test]
    fn test_fuzzy_filter_no_matches() {
        let buffer = TextBuffer::new("git xyz");

        let suggestions = vec![
            Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
        ];

        // Extract "xyz" from buffer
        let word = &buffer.buffer()[4..7];
        let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

        // "xyz" should not match any git commands
        assert!(!active.apply_fuzzy_filter("xyz"));
        assert_eq!(active.suggestions.len(), 0);
    }

    #[test]
    fn test_fuzzy_filter_resets_selected_index() {
        let buffer = TextBuffer::new("git c");

        let suggestions = vec![
            Suggestion::new("add".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("clone".to_string(), "".to_string(), "".to_string()),
        ];

        // Extract "c" from buffer
        let word = &buffer.buffer()[4..5];
        let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

        // Move selection to last item
        active.selected_index = 3;

        // Filter to only "commit", "checkout", "clone" (3 items)
        assert!(active.apply_fuzzy_filter("c"));

        // Selected index should be reset to 0 since old index 3 might be out of bounds
        assert!(active.selected_index < active.suggestions.len());
    }

    #[test]
    fn test_fuzzy_filter_prioritizes_better_matches() {
        let buffer = TextBuffer::new("git ch");

        let suggestions = vec![
            Suggestion::new("commit".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("cherry-pick".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("checkout".to_string(), "".to_string(), "".to_string()),
            Suggestion::new("branch".to_string(), "".to_string(), "".to_string()),
        ];

        // Extract "ch" from buffer
        let word = &buffer.buffer()[4..6];
        let mut active = ActiveSuggestions::try_new(suggestions, word, &buffer).unwrap();

        // "ch" should match cherry-pick and checkout better than others
        assert!(active.apply_fuzzy_filter("ch"));

        // The top matches should start with "ch"
        assert!(active.suggestions[0].s.starts_with("ch"));
    }
}
