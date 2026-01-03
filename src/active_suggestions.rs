

pub struct ActiveSuggestions{
    pub suggestions: Vec<String>,
    selected_index: usize,
}

impl ActiveSuggestions {
    pub fn new(suggestions: Vec<String>) -> Self {
        assert!(suggestions.len() >= 2, "ActiveSuggestions requires at least two suggestions");
        ActiveSuggestions { suggestions, selected_index: 0 }
    }

    pub fn on_tab(&mut self) {
        // Logic to handle tab key when active suggestions are present
        log::info!("Active suggestions: {:?}", self.suggestions);
        self.selected_index = (self.selected_index + 1) % self.suggestions.len();
        log::info!("Selected suggestion: {}", self.suggestions[self.selected_index]);
    }

    pub fn on_enter(&self) -> String {
        // Logic to handle enter key when active suggestions are present
        self.suggestions[self.selected_index].clone()
    }
}