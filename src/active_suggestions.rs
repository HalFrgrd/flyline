use crate::text_buffer::{SubString, TextBuffer};

pub struct ActiveSuggestions {
    pub suggestions: Vec<String>,
    selected_index: usize,
    pub word_under_cursor: SubString,
}

impl ActiveSuggestions {
    pub fn try_new(
        suggestions: Vec<String>,
        word_under_cursor: SubString,
        buffer: &mut TextBuffer,
    ) -> Option<Self> {
        let active_suggestions = ActiveSuggestions {
            suggestions,
            selected_index: 0,
            word_under_cursor,
        };

        match active_suggestions.suggestions.as_slice() {
            [] => {
                log::debug!("No completions found");
                None
            }
            [_] => {
                active_suggestions.accept(buffer);
                log::debug!("Only one completion found for first word: auto-accepted");
                None
            }
            _ => {
                log::debug!(
                    "Multiple completions available: {:?}",
                    active_suggestions.suggestions
                );
                Some(active_suggestions)
            }
        }
    }

    pub fn on_tab(&mut self, shift_tab: bool) {
        // Logic to handle tab key when active suggestions are present
        log::info!("Active suggestions: {:?}", self.suggestions);
        if shift_tab {
            let un_wrapped_index = self.selected_index as i64 - 1;
            let wrapped_index = un_wrapped_index.rem_euclid(self.suggestions.len() as i64);
            self.selected_index = wrapped_index as usize;
        } else {
            self.selected_index = (self.selected_index + 1) % self.suggestions.len();
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&String, bool)> {
        self.suggestions
            .iter()
            .enumerate()
            .map(move |(idx, suggestion)| (suggestion, idx == self.selected_index))
    }

    pub fn accept(self, buffer: &mut TextBuffer) {
        let completion = &self.suggestions[self.selected_index];
        let res = buffer.replace_word_under_cursor(&completion, &self.word_under_cursor);
        match res {
            Ok(_) => buffer.insert_char(' '),
            Err(e) => {
                log::error!("Error during tab completion: {}", e);
            }
        }
    }
}
