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
                let new_suffix = if suffix == " " && s.ends_with(' ') {
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
    pub suggestions: Vec<Suggestion>,
    selected_index: usize,
    pub word_under_cursor: SubString,
}

impl ActiveSuggestions {
    pub fn new<'underlying_buffer>(
        suggestions: Vec<Suggestion>,
        word_under_cursor: &'underlying_buffer str,
        buffer: &'underlying_buffer TextBuffer,
    ) -> Self {
        let word_under_cursor = SubString::new(buffer.buffer(), word_under_cursor).unwrap();

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
            let wrapped_index = un_wrapped_index.rem_euclid(self.suggestions.len() as i64);
            self.selected_index = wrapped_index as usize;
        } else {
            self.selected_index = (self.selected_index + 1) % self.suggestions.len();
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, bool)> {
        // prefix and suffix aren't shown in the suggestion list
        // but are applied when the suggestion is accepted
        self.suggestions
            .iter()
            .enumerate()
            .map(|(idx, suggestion)| (suggestion.s.as_str(), idx == self.selected_index))
    }

    pub fn try_accept(self, buffer: &mut TextBuffer) -> Option<Self> {
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

    pub fn accept_currently_selected(self, buffer: &mut TextBuffer) {
        let completion = &self.suggestions[self.selected_index];

        if let Err(e) =
            buffer.replace_word_under_cursor(&completion.formatted(), &self.word_under_cursor)
        {
            log::error!("Error during tab completion: {}", e);
        }
    }
}
