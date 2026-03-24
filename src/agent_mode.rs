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

    /// Return the currently selected command string, if any.w
    pub fn selected_command(&self) -> Option<&str> {
        self.suggestions
            .get(self.selected_idx)
            .map(|s| s.command.as_str())
    }
}

fn skip_whitespace(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\n' | b'\r') {
        *pos += 1;
    }
}

/// Parse a JSON string at `pos` (which must point at `"`).
/// Advances `pos` past the closing `"` and returns the decoded string.
fn parse_json_string(bytes: &[u8], pos: &mut usize) -> Option<String> {
    skip_whitespace(bytes, pos);
    if *pos >= bytes.len() || bytes[*pos] != b'"' {
        return None;
    }
    *pos += 1;
    let mut result = Vec::new();
    loop {
        if *pos >= bytes.len() {
            return None;
        }
        match bytes[*pos] {
            b'"' => {
                *pos += 1;
                return String::from_utf8(result).ok();
            }
            b'\\' => {
                *pos += 1;
                if *pos >= bytes.len() {
                    return None;
                }
                match bytes[*pos] {
                    b'"' => result.push(b'"'),
                    b'\\' => result.push(b'\\'),
                    b'/' => result.push(b'/'),
                    b'n' => result.push(b'\n'),
                    b'r' => result.push(b'\r'),
                    b't' => result.push(b'\t'),
                    b'b' => result.push(8u8),
                    b'f' => result.push(12u8),
                    b'u' => {
                        // Decode \uXXXX escape.
                        *pos += 1;
                        if *pos + 4 > bytes.len() {
                            return None;
                        }
                        let decoded = std::str::from_utf8(&bytes[*pos..*pos + 4])
                            .ok()
                            .and_then(|hex_str| u32::from_str_radix(hex_str, 16).ok())
                            .and_then(char::from_u32);
                        match decoded {
                            Some(ch) => {
                                let mut buf = [0u8; 4];
                                result.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                            }
                            None => {
                                log::warn!("AI output contained invalid \\uXXXX escape");
                                result.push(b'?');
                            }
                        }
                        *pos += 3; // outer += 1 below advances past the 4th hex digit
                    }
                    // Unknown escape sequences: output the character without the backslash
                    // (lenient behavior for AI-generated output).
                    c => result.push(c),
                }
                *pos += 1;
            }
            b => {
                result.push(b);
                *pos += 1;
            }
        }
    }
}

/// Skip over a complete JSON value at `pos`. Returns `false` on parse failure.
fn skip_json_value(bytes: &[u8], pos: &mut usize) -> bool {
    skip_whitespace(bytes, pos);
    if *pos >= bytes.len() {
        return false;
    }
    match bytes[*pos] {
        b'"' => parse_json_string(bytes, pos).is_some(),
        b'{' => skip_json_object(bytes, pos),
        b'[' => {
            *pos += 1;
            skip_whitespace(bytes, pos);
            if *pos < bytes.len() && bytes[*pos] == b']' {
                *pos += 1;
                return true;
            }
            loop {
                if !skip_json_value(bytes, pos) {
                    return false;
                }
                skip_whitespace(bytes, pos);
                if *pos >= bytes.len() {
                    return false;
                }
                match bytes[*pos] {
                    b',' => *pos += 1,
                    b']' => {
                        *pos += 1;
                        return true;
                    }
                    _ => return false,
                }
            }
        }
        _ => {
            // number, true, false, null
            while *pos < bytes.len()
                && !matches!(
                    bytes[*pos],
                    b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'
                )
            {
                *pos += 1;
            }
            true
        }
    }
}

fn skip_json_object(bytes: &[u8], pos: &mut usize) -> bool {
    if *pos >= bytes.len() || bytes[*pos] != b'{' {
        return false;
    }
    *pos += 1;
    skip_whitespace(bytes, pos);
    if *pos < bytes.len() && bytes[*pos] == b'}' {
        *pos += 1;
        return true;
    }
    loop {
        skip_whitespace(bytes, pos);
        if parse_json_string(bytes, pos).is_none() {
            return false;
        }
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() || bytes[*pos] != b':' {
            return false;
        }
        *pos += 1;
        if !skip_json_value(bytes, pos) {
            return false;
        }
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() {
            return false;
        }
        match bytes[*pos] {
            b',' => *pos += 1,
            b'}' => {
                *pos += 1;
                return true;
            }
            _ => return false,
        }
    }
}

/// Parse a single JSON object `{…}` into an [`AiSuggestion`].
/// Returns `None` if parsing fails or if the object has no non-empty `"command"`.
fn parse_suggestion_object(bytes: &[u8], pos: &mut usize) -> Option<AiSuggestion> {
    skip_whitespace(bytes, pos);
    if *pos >= bytes.len() || bytes[*pos] != b'{' {
        return None;
    }
    *pos += 1;
    let mut command = String::new();
    let mut description = String::new();
    skip_whitespace(bytes, pos);
    if *pos < bytes.len() && bytes[*pos] == b'}' {
        *pos += 1;
        return None;
    }
    loop {
        skip_whitespace(bytes, pos);
        let key = parse_json_string(bytes, pos)?;
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() || bytes[*pos] != b':' {
            return None;
        }
        *pos += 1;
        match key.as_str() {
            "command" => command = parse_json_string(bytes, pos)?,
            "description" => description = parse_json_string(bytes, pos)?,
            _ => {
                if !skip_json_value(bytes, pos) {
                    return None;
                }
            }
        }
        skip_whitespace(bytes, pos);
        if *pos >= bytes.len() {
            return None;
        }
        match bytes[*pos] {
            b',' => *pos += 1,
            b'}' => {
                *pos += 1;
                break;
            }
            _ => return None,
        }
    }
    if command.is_empty() {
        return None;
    }
    Some(AiSuggestion {
        command,
        description,
    })
}

/// Parse AI command output into a list of [`AiSuggestion`]s.
///
/// The output may contain prose before and/or after the JSON array.
/// We locate the first `[` and its matching `]`, then parse the contained
/// objects without any external JSON library.  The raw output is always
/// logged at DEBUG level.
pub fn parse_ai_output(raw: &str) -> Vec<AiSuggestion> {
    log::debug!("AI raw output: {}", raw);

    // Find the first `[` that begins a JSON array.
    let start = match raw.find('[') {
        Some(s) => s,
        None => {
            log::warn!("AI output contained no JSON array (no '[' found)");
            return vec![];
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

    let end = match end {
        Some(e) => e,
        None => {
            log::warn!("AI output JSON array is not terminated");
            return vec![];
        }
    };

    // Parse the extracted array without any external JSON library.
    let arr_bytes = &bytes[start..end];
    let mut pos = 0usize;

    // Consume the opening `[`.
    skip_whitespace(arr_bytes, &mut pos);
    if pos >= arr_bytes.len() || arr_bytes[pos] != b'[' {
        return vec![];
    }
    pos += 1;

    let mut suggestions = Vec::new();
    loop {
        skip_whitespace(arr_bytes, &mut pos);
        if pos >= arr_bytes.len() || arr_bytes[pos] == b']' {
            break;
        }
        if arr_bytes[pos] == b',' {
            pos += 1;
            continue;
        }
        let saved_pos = pos;
        match parse_suggestion_object(arr_bytes, &mut pos) {
            Some(s) => suggestions.push(s),
            None => {
                // On parse error, try to skip the malformed value structurally.
                // We attempt skip_json_value from the saved position; if that
                // also fails (truly malformed input), fall back to byte-scanning.
                let mut skip_pos = saved_pos;
                if skip_json_value(arr_bytes, &mut skip_pos) {
                    pos = skip_pos;
                } else {
                    while pos < arr_bytes.len() && !matches!(arr_bytes[pos], b',' | b']') {
                        pos += 1;
                    }
                }
            }
        }
    }
    suggestions
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
