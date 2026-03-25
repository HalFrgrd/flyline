use picojson::{SliceParser, Event, PullParser};

/// A single AI-suggested command with a human-readable description.
#[derive(Debug, Clone)]
pub struct AiSuggestion {
    pub command: String,
    pub description: String,
}

/// Tracks the currently selected index inside the AI output selection list.
#[derive(Debug)]
pub struct AiOutputSelection {
    pub suggestions: Vec<AiSuggestion>,
    pub selected_idx: usize,
}

impl AiOutputSelection {
    pub fn new(suggestions: Vec<AiSuggestion>) -> Self {
        AiOutputSelection {
            suggestions,
            selected_idx: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_idx + 1 < self.suggestions.len() {
            self.selected_idx += 1;
        }
    }

    pub fn set_selected_by_idx(&mut self, idx: usize) {
        if idx < self.suggestions.len() {
            self.selected_idx = idx;
        }
    }

    /// Return the currently selected command string, if any.
    pub fn selected_command(&self) -> Option<&str> {
        self.suggestions
            .get(self.selected_idx)
            .map(|s| s.command.as_str())
    }
}

/// Parse AI command output into a list of [`AiSuggestion`]s.
///
/// The output may contain prose before and/or after the JSON array.
/// We use a regex to locate the start of the JSON array, bracket-match to
/// find its end, then parse the extracted slice with the `json` crate.
/// The raw output is always logged at DEBUG level.
pub fn parse_ai_output(raw: &str) -> Vec<AiSuggestion> {
    log::debug!("AI raw output: {}", raw);

    // Find the first `[` that begins a JSON array.
    let start = match raw.find('[') {
        Some(idx) => idx,
        None => {
            log::warn!("AI output contained no JSON array (no '[' found)");
            return vec![];
        }
    };

    println!("AI output starts with JSON array at index {}", start);

    let mut parser = SliceParser::new(&raw[start..]);

    match parser.next_event() {
        Ok(Event::StartArray) => {
            let mut suggestions = Vec::new();
            while let Ok(event) = parser.next_event() {
                match event {
                    Event::EndArray => break,
                    Event::StartObject => {
                        let mut command = String::new();
                        let mut description = String::new();
                        while let Ok(event) = parser.next_event().clone() {
                            match event {
                                Event::EndObject => break,
                                Event::String(key) => {
                                    if let Ok(Event::String(value)) = parser.next_event() {
                                        match key.as_str() {
                                            "command" => command = value.as_str().to_string(),
                                            "description" => description = value.as_str().to_string(),
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        if !command.is_empty() {
                            suggestions.push(AiSuggestion { command, description });
                        }
                    }
                    _ => {}
                }
            }
            suggestions
        }
        Ok(_) => {
            println!("AI output JSON was not an array");
            vec![]
        }
        Err(e) => {
            println!("Failed to parse AI output as JSON: {}", e);
            vec![]
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clean_json() {
        let raw = r#"[{"command": "ls -la", "description": "List all files"}, {"command": "pwd", "description": "Print working directory"}]"#;
        let suggestions = parse_ai_output(raw);
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].command, "ls -la");
        assert_eq!(suggestions[0].description, "List all files");
        assert_eq!(suggestions[1].command, "pwd");
        assert_eq!(suggestions[1].description, "Print working directory");
    }

    #[test]
    fn test_parse_with_preamble() {
        let raw = r#"Here are some suggestions:
[{"command": "grep -r foo .", "description": "Search recursively"}]
That should help!"#;
        let suggestions = parse_ai_output(raw);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "grep -r foo .");
    }

    #[test]
    fn test_parse_no_json_array() {
        let suggestions = parse_ai_output("Sorry, I cannot help with that.");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let suggestions = parse_ai_output("[{bad json}]");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_parse_empty_command_skipped() {
        let raw = r#"[{"command": "", "description": "empty"}, {"command": "echo hi", "description": "hello"}]"#;
        let suggestions = parse_ai_output(raw);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "echo hi");
    }

    #[test]
    fn test_ai_output_selection_navigation() {
        let suggestions = vec![
            AiSuggestion {
                command: "cmd1".to_string(),
                description: "desc1".to_string(),
            },
            AiSuggestion {
                command: "cmd2".to_string(),
                description: "desc2".to_string(),
            },
            AiSuggestion {
                command: "cmd3".to_string(),
                description: "desc3".to_string(),
            },
        ];
        let mut sel = AiOutputSelection::new(suggestions);
        assert_eq!(sel.selected_idx, 0);
        assert_eq!(sel.selected_command(), Some("cmd1"));

        sel.move_down();
        assert_eq!(sel.selected_idx, 1);
        assert_eq!(sel.selected_command(), Some("cmd2"));

        sel.move_up();
        assert_eq!(sel.selected_idx, 0);

        // Can't go below 0
        sel.move_up();
        assert_eq!(sel.selected_idx, 0);

        // Can't go past the end
        sel.move_down();
        sel.move_down();
        assert_eq!(sel.selected_idx, 2);
        sel.move_down();
        assert_eq!(sel.selected_idx, 2);

        sel.set_selected_by_idx(1);
        assert_eq!(sel.selected_idx, 1);
        // Out of bounds is ignored
        sel.set_selected_by_idx(100);
        assert_eq!(sel.selected_idx, 1);
    }
}
