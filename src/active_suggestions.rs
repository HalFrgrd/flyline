use crate::text_buffer::{SubString, TextBuffer};

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

#[derive(Debug)]
pub struct ActiveSuggestions {
    pub suggestions: Vec<Suggestion>,
    selected_index: usize,
    pub word_under_cursor: SubString,
    last_grid_size: (usize, usize),
}

impl ActiveSuggestions {
    pub fn try_new<'underlying_buffer>(
        suggestions: Vec<Suggestion>,
        word_under_cursor: &'underlying_buffer str,
        buffer: &'underlying_buffer TextBuffer,
    ) -> Option<Self> {
        let word_under_cursor = SubString::new(buffer.buffer(), word_under_cursor).ok()?;

        Some(ActiveSuggestions {
            suggestions,
            selected_index: 0,
            word_under_cursor,
            last_grid_size: (0, 0),
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
        self.selected_index = new_index.rem_euclid(self.suggestions.len() as i32) as usize;
    }

    // TODO arrow keys when not all suggestions are visible
    pub fn on_right_arrow(&mut self) {
        let new_idx: i32 = self.selected_index as i32 + self.last_grid_size.0 as i32;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_left_arrow(&mut self) {
        let new_idx: i32 = self.selected_index as i32 - self.last_grid_size.0 as i32;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_down_arrow(&mut self) {
        let new_idx: i32 = self.selected_index as i32 + 1;
        self.sanitize_selected_index(new_idx);
    }

    pub fn on_up_arrow(&mut self) {
        let new_idx: i32 = self.selected_index as i32 - 1;
        self.sanitize_selected_index(new_idx);
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, bool)> {
        // prefix and suffix aren't shown in the suggestion list
        // but are applied when the suggestion is accepted
        self.suggestions
            .iter()
            .enumerate()
            .map(|(idx, suggestion)| (suggestion.s.as_str(), idx == self.selected_index))
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

    pub fn try_accept(mut self, buffer: &mut TextBuffer) -> Option<Self> {
        match self.suggestions.as_slice() {
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
                log::debug!("Multiple completions available: {:?}", self.suggestions);
                Some(self)
            }
        }
    }

    pub fn accept_currently_selected(&mut self, buffer: &mut TextBuffer) {
        if let Some(completion) = self.suggestions.get(self.selected_index) {
            if let Err(e) =
                buffer.replace_word_under_cursor(&completion.formatted(), &self.word_under_cursor)
            {
                log::error!("Error during tab completion: {}", e);
            }
        } else {
            log::error!(
                "Tried to accept suggestion at index {}, but only {} suggestions are available",
                self.selected_index,
                self.suggestions.len()
            );
        }
    }
}
