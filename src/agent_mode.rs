use anyhow::{anyhow, bail};
use regex::Regex;
use std::sync::OnceLock;

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
    /// Text from the AI output that appeared before the JSON array.
    pub header: String,
    /// Text from the AI output that appeared after the JSON array.
    pub footer: String,
}

impl AiOutputSelection {
    pub(crate) fn new(suggestions: Vec<AiSuggestion>, header: String, footer: String) -> Self {
        AiOutputSelection {
            suggestions,
            selected_idx: 0,
            header,
            footer,
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

    /// Return the currently selected command string, if any.w
    pub fn selected_command(&self) -> Option<&str> {
        self.suggestions
            .get(self.selected_idx)
            .map(|s| s.command.as_str())
    }
}

/// Parse AI command output into an [`AiOutputSelection`].
///
/// The output may contain prose before and/or after the JSON array.
/// We use a regex to locate the start of the JSON array and then attempt to
/// parse it with `serde_json`.  Text before the array is stored as the
/// `header` and text after the array is stored as the `footer`.
/// Returns `Err` if no valid JSON array with at least one non-empty command
/// can be found.  The raw output is always logged at DEBUG level.
pub fn parse_ai_output(raw: &str) -> anyhow::Result<AiOutputSelection> {
    log::debug!("AI raw output: {}", raw);

    // Find the first `[` that begins a JSON array using a regex.
    static JSON_ARRAY_RE: OnceLock<Regex> = OnceLock::new();
    let re = JSON_ARRAY_RE.get_or_init(|| Regex::new(r"\[").expect("static regex is valid"));
    let start = match re.find(raw) {
        Some(m) => m.start(),
        None => {
            log::warn!("AI output contained no JSON array (no '[' found)");
            bail!("AI output contained no JSON array");
        }
    };

    // Walk forward from `start` to find the matching closing `]`, respecting
    // nested array brackets and JSON string literals (so brackets inside strings
    // are not counted).
    let bytes = raw.as_bytes();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let mut end = None;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if escape_next {
            escape_next = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escape_next = true,
                b'"' => in_string = false,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    let array_end = match end {
        Some(e) => e,
        None => {
            log::warn!("AI output JSON array is not terminated");
            bail!("AI output JSON array is not terminated");
        }
    };

    let candidate = &raw[start..array_end];
    let header = raw[..start].trim().to_string();
    let footer = raw[array_end..].trim().to_string();

    match serde_json::from_str::<serde_json::Value>(candidate) {
        Ok(serde_json::Value::Array(arr)) => {
            let mut suggestions = Vec::with_capacity(arr.len());
            for item in arr {
                let command = item
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let description = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !command.is_empty() {
                    suggestions.push(AiSuggestion {
                        command,
                        description,
                    });
                }
            }
            if suggestions.is_empty() {
                bail!("AI output JSON array contained no valid commands");
            }
            Ok(AiOutputSelection::new(suggestions, header, footer))
        }
        Ok(_) => {
            log::warn!("AI output JSON was not an array");
            Err(anyhow!("AI output JSON was not an array"))
        }
        Err(e) => {
            log::warn!("Failed to parse AI output as JSON: {}", e);
            Err(anyhow!("Failed to parse AI output as JSON: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clean_json() {
        let raw = r#"[{"command": "ls -la", "description": "List all files"}, {"command": "pwd", "description": "Print working directory"}]"#;
        let sel = parse_ai_output(raw).unwrap();
        assert_eq!(sel.suggestions.len(), 2);
        assert_eq!(sel.suggestions[0].command, "ls -la");
        assert_eq!(sel.suggestions[0].description, "List all files");
        assert_eq!(sel.suggestions[1].command, "pwd");
        assert_eq!(sel.suggestions[1].description, "Print working directory");
        assert_eq!(sel.header, "");
        assert_eq!(sel.footer, "");
    }

    #[test]
    fn test_parse_with_preamble() {
        let raw = r#"Here are some suggestions:
[{"command": "grep -r foo .", "description": "Search recursively"}]
That should help!"#;
        let sel = parse_ai_output(raw).unwrap();
        assert_eq!(sel.suggestions.len(), 1);
        assert_eq!(sel.suggestions[0].command, "grep -r foo .");
        assert_eq!(sel.header, "Here are some suggestions:");
        assert_eq!(sel.footer, "That should help!");
    }

    #[test]
    fn test_parse_no_json_array() {
        assert!(parse_ai_output("Sorry, I cannot help with that.").is_err());
    }

    #[test]
    fn test_parse_invalid_json() {
        assert!(parse_ai_output("[{bad json}]").is_err());
    }

    #[test]
    fn test_parse_empty_command_skipped() {
        let raw = r#"[{"command": "", "description": "empty"}, {"command": "echo hi", "description": "hello"}]"#;
        let sel = parse_ai_output(raw).unwrap();
        assert_eq!(sel.suggestions.len(), 1);
        assert_eq!(sel.suggestions[0].command, "echo hi");
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
        let mut sel = AiOutputSelection::new(suggestions, String::new(), String::new());
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
