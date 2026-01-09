use crate::text_buffer::SubString;

pub struct ActiveSuggestions {
    pub suggestions: Vec<String>,
    selected_index: usize,
    pub word_under_cursor: SubString,
}

impl ActiveSuggestions {
    pub fn new(suggestions: Vec<String>, word_under_cursor: SubString) -> Self {
        assert!(
            suggestions.len() >= 2,
            "ActiveSuggestions requires at least two suggestions"
        );
        ActiveSuggestions {
            suggestions,
            selected_index: 0,
            word_under_cursor,
        }
    }

    pub fn on_tab(&mut self, shift_tab: bool) {
        // Logic to handle tab key when active suggestions are present
        log::info!("Active suggestions: {:?}", self.suggestions);
        if shift_tab {
            let un_wrapped_index = self.selected_index as i64 - 1;
            log::info!("Unwrapped index: {}", un_wrapped_index);
            let wrapped_index = un_wrapped_index.rem_euclid(self.suggestions.len() as i64);
            log::info!("Wrapped index: {}", wrapped_index);
            self.selected_index = wrapped_index as usize;
        } else {
            self.selected_index = (self.selected_index + 1) % self.suggestions.len();
        }
        log::info!(
            "Selected suggestion: {}",
            self.suggestions[self.selected_index]
        );
    }

    pub fn on_enter(&self) -> (String, SubString) {
        // Logic to handle enter key when active suggestions are present
        (
            self.suggestions[self.selected_index].clone(),
            self.word_under_cursor.clone(),
        )
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&String, bool)> {
        self.suggestions
            .iter()
            .enumerate()
            .map(move |(idx, suggestion)| (suggestion, idx == self.selected_index))
    }
}
