use anyhow::{anyhow, bail};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::prelude::*;
use ratatui::text::Text;

use crate::table::{TableAccum, compute_natural_col_widths, render_table};

/// A single AI-suggested command with a human-readable description.
#[derive(Debug, Clone)]
pub struct AiSuggestion {
    pub command: String,
    pub description: String,
}

/// Raw parsed output from the AI: the list of suggestions plus the prose that
/// surrounded the JSON array in the model's response.
#[derive(Debug)]
pub struct AiOutputParsed {
    pub suggestions: Vec<AiSuggestion>,
    /// Text from the AI output that appeared before the JSON array.
    pub header: String,
    /// Text from the AI output that appeared after the JSON array.
    pub footer: String,
}

/// Tracks the currently selected index inside the AI output selection list.
/// Constructed from an [`AiOutputParsed`]; strips any outer code-fence lines
/// from the header/footer and converts the prose to styled [`Text`] using
/// pulldown-cmark.
#[derive(Debug)]
pub struct AiOutputSelection {
    pub suggestions: Vec<AiSuggestion>,
    pub selected_idx: usize,
    /// Rendered markdown of the prose before the suggestions.
    pub header_text: Text<'static>,
    /// Rendered markdown of the prose after the suggestions.
    pub footer_text: Text<'static>,
}

/// Strip the last line of `s` when it starts with three backticks.
fn strip_trailing_fence(s: &str) -> &str {
    match s.rsplit_once('\n') {
        Some((rest, last)) if last.starts_with("```") => rest,
        _ => {
            if s.starts_with("```") {
                ""
            } else {
                s
            }
        }
    }
}

/// Strip the first line of `s` when it starts with three backticks.
fn strip_leading_fence(s: &str) -> &str {
    match s.split_once('\n') {
        Some((first, rest)) if first.starts_with("```") => rest,
        _ => {
            if s.starts_with("```") {
                ""
            } else {
                s
            }
        }
    }
}

/// Convert a markdown string to a ratatui [`Text`] object.
///
/// Renders basic markdown constructs (headings, paragraphs, bold, italic,
/// inline code, code blocks, list items, block quotes, and tables) as
/// styled spans.  Uses ratatui's own types so there is no external crate
/// version conflict.
fn markdown_to_text(markdown: &str, palette: &crate::palette::Palette) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();

    let mut bold = false;
    let mut italic = false;
    let mut heading_level: Option<u8> = None;
    let mut list_depth: u32 = 0;
    let mut in_code_block = false;
    // When `Some`, we are inside a markdown table and accumulating its data.
    let mut table_accum: Option<TableAccum> = None;

    let heading1_style = palette.markdown_heading1();
    let heading2_style = palette.markdown_heading2();
    let heading3_style = palette.markdown_heading3();
    let code_style = palette.markdown_code();

    let style_from_markdown_state =
        move |bold: bool, italic: bool, code: bool, heading: Option<u8>| -> Style {
            if code {
                return code_style;
            }
            if let Some(level) = heading {
                return match level {
                    1 => heading1_style,
                    2 => heading2_style,
                    _ => heading3_style,
                };
            }
            let mut style = Style::default();
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            if italic {
                style = style.add_modifier(Modifier::ITALIC);
            }
            style
        };

    let finalize_line =
        |lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>, list_depth: u32| {
            if list_depth > 0 && !spans.is_empty() {
                spans.insert(0, Span::raw("  ".repeat(list_depth as usize - 1) + "• "));
            }
            lines.push(Line::from(std::mem::take(spans)));
        };

    let parser = Parser::new_ext(markdown, Options::all());
    let mut prev_event = None;
    for event in parser {
        log::info!("Markdown event: {:?}", event);
        match event.clone() {
            // ── Table events ──────────────────────────────────────────────
            Event::Start(Tag::Table(alignments)) => {
                // Flush any preceding inline content as a line before the table.
                if !current_spans.is_empty() {
                    finalize_line(&mut lines, &mut current_spans, list_depth);
                }
                table_accum = Some(TableAccum::new(alignments));
            }
            Event::End(TagEnd::Table) => {
                if let Some(accum) = table_accum.take() {
                    let col_widths = compute_natural_col_widths(&accum);
                    lines.extend(render_table(&accum, &col_widths));
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(ref mut accum) = table_accum {
                    accum.in_header = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(ref mut accum) = table_accum {
                    accum.header_cells = std::mem::take(&mut accum.current_cells);
                    accum.in_header = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(ref mut accum) = table_accum {
                    accum.current_cells.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(ref mut accum) = table_accum {
                    accum
                        .body_rows
                        .push(std::mem::take(&mut accum.current_cells));
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(ref mut accum) = table_accum {
                    accum.current_cell_buf.clear();
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(ref mut accum) = table_accum {
                    accum
                        .current_cells
                        .push(std::mem::take(&mut accum.current_cell_buf));
                }
            }
            // ── Non-table events ──────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                heading_level = Some(level as u8);
            }
            Event::End(TagEnd::Heading(_)) => {
                finalize_line(&mut lines, &mut current_spans, 0);
                heading_level = None;
            }
            Event::Start(Tag::Paragraph) => {
                if !matches!(prev_event, Some(Event::End(TagEnd::Heading { .. }))) {
                    finalize_line(&mut lines, &mut current_spans, list_depth);
                }
            }
            Event::End(TagEnd::Paragraph) => {
                finalize_line(&mut lines, &mut current_spans, list_depth);
            }
            Event::Start(Tag::Strong) => {
                bold = true;
            }
            Event::End(TagEnd::Strong) => {
                bold = false;
            }
            Event::Start(Tag::Emphasis) => {
                italic = true;
            }
            Event::End(TagEnd::Emphasis) => {
                italic = false;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                finalize_line(&mut lines, &mut current_spans, 0);
            }
            Event::Start(Tag::List(_)) => {
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {}
            Event::End(TagEnd::Item) => {
                finalize_line(&mut lines, &mut current_spans, list_depth);
            }
            Event::Start(Tag::BlockQuote(_)) | Event::End(TagEnd::BlockQuote(_)) => {}
            Event::Code(text) => {
                if let Some(ref mut accum) = table_accum {
                    accum.current_cell_buf.push_str(&text);
                } else {
                    let is_code = true;
                    let style = style_from_markdown_state(bold, italic, is_code, heading_level);
                    current_spans.push(Span::styled(text.into_string(), style));
                }
            }
            Event::Text(text) => {
                if let Some(ref mut accum) = table_accum {
                    accum.current_cell_buf.push_str(&text);
                } else {
                    let is_code = in_code_block;
                    let style = style_from_markdown_state(bold, italic, is_code, heading_level);
                    current_spans.push(Span::styled(text.into_string(), style));
                }
            }
            Event::SoftBreak => {
                if table_accum.is_none() {
                    current_spans.push(Span::raw(" "));
                }
            }
            Event::HardBreak | Event::Rule => {
                if table_accum.is_none() {
                    finalize_line(&mut lines, &mut current_spans, list_depth);
                }
            }
            _ => {}
        }
        prev_event = Some(event);
    }

    // Flush any remaining content.
    if !current_spans.is_empty() {
        finalize_line(&mut lines, &mut current_spans, list_depth);
    }

    Text::from(lines)
}

impl AiOutputSelection {
    pub fn new(parsed: AiOutputParsed, palette: &crate::palette::Palette) -> Self {
        let header_md = strip_trailing_fence(parsed.header.as_str()).to_string();
        let footer_md = strip_leading_fence(parsed.footer.as_str()).to_string();
        AiOutputSelection {
            suggestions: parsed.suggestions,
            selected_idx: 0,
            header_text: markdown_to_text(&header_md, palette),
            footer_text: markdown_to_text(&footer_md, palette),
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

/// Intermediate struct for deserializing a single suggestion from JSON.
#[derive(serde::Deserialize)]
struct AiSuggestionRaw {
    #[serde(default)]
    command: String,
    #[serde(default)]
    description: String,
}

/// Parse AI command output into an [`AiOutputParsed`].
///
/// The output may contain prose before and/or after the JSON array.
/// We look for the first `[` at the start of a line and use a streaming
/// JSON deserializer to parse the array, stopping after the first complete
/// array without scanning the rest of the text.
/// Text before the array is stored as the `header` and text after as the
/// `footer`.
/// Returns `Err` if no valid JSON array with at least one non-empty command
/// can be found.  The raw output is always logged at DEBUG level.
pub fn parse_ai_output(raw: &str) -> anyhow::Result<AiOutputParsed> {
    log::debug!("AI raw output: {}", raw);

    // Find the first `[` that appears at the start of a line (position 0 or
    // immediately after a `\n`).
    let start = raw
        .starts_with('[')
        .then_some(0)
        .or_else(|| raw.find("\n[").map(|i| i + 1))
        .ok_or_else(|| {
            log::warn!("AI output contained no JSON array (no '[' at start of line)");
            anyhow!("AI output contained no JSON array")
        })?;

    // Use a streaming deserializer: parse exactly one JSON array starting at
    // `start` and stop as soon as it is complete, without reading further.
    let substr = &raw[start..];
    let mut json_stream =
        serde_json::Deserializer::from_str(substr).into_iter::<Vec<AiSuggestionRaw>>();
    let items: Vec<AiSuggestionRaw> = json_stream
        .next()
        .ok_or_else(|| {
            log::warn!("AI output contained no JSON array");
            anyhow!("AI output contained no JSON array")
        })?
        .map_err(|e| {
            log::warn!("Failed to parse AI output as JSON: {}", e);
            anyhow!("Failed to parse AI output as JSON: {}", e)
        })?;
    let array_end = start + json_stream.byte_offset();

    let header = raw[..start].trim().to_string();
    let footer = raw[array_end..].trim().to_string();

    let suggestions: Vec<AiSuggestion> = items
        .into_iter()
        .filter(|s| !s.command.is_empty())
        .map(|s| AiSuggestion {
            command: s.command,
            description: s.description,
        })
        .collect();

    if suggestions.is_empty() {
        bail!("AI output JSON array contained no valid commands");
    }

    Ok(AiOutputParsed {
        suggestions,
        header,
        footer,
    })
}

const EXAMPLE_AGENT_MODE: &str = include_str!("../examples/agent_mode.sh");

/// Extract the command executable name from a `--command '...'` argument in a
/// `flyline set-agent-mode` command string.  Returns `None` if the pattern is
/// not found or the value is not properly closed with a single quote.  The
/// parsing is intentionally simple: it looks for the literal substring
/// `--command '`, finds the closing `'`, and returns the first
/// whitespace-delimited word from the content between the quotes.
fn extract_command_name(flyline_cmd: &str) -> Option<String> {
    let marker = "--command '";
    let start = flyline_cmd.find(marker)?;
    let after = &flyline_cmd[start + marker.len()..];
    let end = after.find('\'')?;
    let cmd_str = &after[..end];
    cmd_str.split_whitespace().next().map(|s| s.to_string())
}

/// Parse [`EXAMPLE_AGENT_MODE`] (embedded at compile time from
/// `examples/agent_mode.sh`) and return a list of
/// `(command_executable_name, full_flyline_set_agent_mode_command)` pairs —
/// one entry per `flyline set-agent-mode` block found in the file.
///
/// Each multi-line continuation block (lines ending with ` \`) is joined into
/// a single command string.  Lines that begin with `#` are skipped.
pub fn parse_example_agent_commands() -> Vec<(String, String)> {
    let lines: Vec<&str> = EXAMPLE_AGENT_MODE.lines().collect();
    let mut results = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("flyline set-agent-mode") {
            // Collect continuation lines (ending with ' \').
            let mut block: Vec<&str> = Vec::new();
            loop {
                block.push(lines[i]);
                if lines[i].trim_end().ends_with('\\') {
                    i += 1;
                    if i >= lines.len() {
                        break;
                    }
                } else {
                    break;
                }
            }
            // Join: strip trailing '\' and surrounding whitespace from each piece.
            let cmd_str = block
                .iter()
                .map(|l| l.trim_end_matches('\\').trim())
                .collect::<Vec<_>>()
                .join(" ");

            if let Some(cmd_name) = extract_command_name(&cmd_str) {
                results.push((cmd_name, cmd_str));
            }
        }
        i += 1;
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_selection(suggestions: Vec<AiSuggestion>) -> AiOutputSelection {
        let palette = crate::palette::Palette::default();
        AiOutputSelection::new(
            AiOutputParsed {
                suggestions,
                header: String::new(),
                footer: String::new(),
            },
            &palette,
        )
    }

    #[test]
    fn test_parse_clean_json() {
        let raw = r#"[{"command": "ls -la", "description": "List all files"}, {"command": "pwd", "description": "Print working directory"}]"#;
        let parsed = parse_ai_output(raw).unwrap();
        assert_eq!(parsed.suggestions.len(), 2);
        assert_eq!(parsed.suggestions[0].command, "ls -la");
        assert_eq!(parsed.suggestions[0].description, "List all files");
        assert_eq!(parsed.suggestions[1].command, "pwd");
        assert_eq!(parsed.suggestions[1].description, "Print working directory");
        assert_eq!(parsed.header, "");
        assert_eq!(parsed.footer, "");
    }

    #[test]
    fn test_parse_with_preamble() {
        let raw = r#"Here are some suggestions:
[{"command": "grep -r foo .", "description": "Search recursively"}]
That should help!"#;
        let parsed = parse_ai_output(raw).unwrap();
        assert_eq!(parsed.suggestions.len(), 1);
        assert_eq!(parsed.suggestions[0].command, "grep -r foo .");
        assert_eq!(parsed.header, "Here are some suggestions:");
        assert_eq!(parsed.footer, "That should help!");
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
        let parsed = parse_ai_output(raw).unwrap();
        assert_eq!(parsed.suggestions.len(), 1);
        assert_eq!(parsed.suggestions[0].command, "echo hi");
    }

    #[test]
    fn test_strip_trailing_fence() {
        assert_eq!(strip_trailing_fence("hello\n```json"), "hello");
        assert_eq!(strip_trailing_fence("hello\n```"), "hello");
        assert_eq!(strip_trailing_fence("hello\nworld"), "hello\nworld");
        assert_eq!(strip_trailing_fence("```json"), "");
        assert_eq!(strip_trailing_fence("no fence"), "no fence");
    }

    #[test]
    fn test_strip_leading_fence() {
        assert_eq!(strip_leading_fence("```json\nhello"), "hello");
        assert_eq!(strip_leading_fence("```\nhello"), "hello");
        assert_eq!(strip_leading_fence("hello\nworld"), "hello\nworld");
        assert_eq!(strip_leading_fence("```json"), "");
        assert_eq!(strip_leading_fence("no fence"), "no fence");
    }

    #[test]
    fn test_ai_output_selection_strips_fences() {
        let parsed = AiOutputParsed {
            suggestions: vec![AiSuggestion {
                command: "ls".to_string(),
                description: "list".to_string(),
            }],
            header: "Here are commands:\n```json".to_string(),
            footer: "```\nDone.".to_string(),
        };
        let palette = crate::palette::Palette::default();
        let sel = AiOutputSelection::new(parsed, &palette);
        // header_text should be rendered from "Here are commands:" (fence stripped)
        // footer_text should be rendered from "Done." (fence stripped)
        assert!(!sel.header_text.lines.is_empty());
        assert!(!sel.footer_text.lines.is_empty());
    }

    #[test]
    fn test_markdown_table_rendering() {
        let palette = crate::palette::Palette::default();
        // Basic two-column table
        let md = "\
| Command | Description       |
|---------|-------------------|
| ls      | List files        |
| pwd     | Print working dir |
";
        let text = markdown_to_text(md, &palette);
        // Should produce 3 lines: header, separator, 2 body rows
        assert_eq!(
            text.lines.len(),
            6,
            "expected 6 lines (top border + header + sep + 2 rows + bottom border), got {}: {:#?}",
            text.lines.len(),
            text.lines
        );

        // Flatten each line to a plain string for easy assertion.
        let plain: Vec<String> = text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        // Header row contains the column names.
        assert!(plain[0].contains("╭─"), "top_border: {}", plain[0]);
        assert!(plain[1].contains("Command"), "header: {}", plain[1]);
        assert!(plain[1].contains("Description"), "header: {}", plain[1]);
        // Separator row contains box-drawing dashes.
        assert!(plain[2].contains('─'), "separator: {}", plain[2]);
        // Body rows contain cell values.
        assert!(plain[3].contains("ls"), "row1: {}", plain[3]);
        assert!(plain[3].contains("List files"), "row1: {}", plain[3]);
        assert!(plain[4].contains("pwd"), "row2: {}", plain[4]);
        assert!(plain[4].contains("Print working dir"), "row2: {}", plain[4]);
    }

    #[test]
    fn test_markdown_table_with_alignment() {
        let palette = crate::palette::Palette::default();
        // Table with left/center/right alignment hints
        let md = "\
| Left   | Center | Right |
|:-------|:------:|------:|
| a      | b      | c     |
";
        let text = markdown_to_text(md, &palette);
        // 3 lines: header + separator + 1 body row
        assert_eq!(
            text.lines.len(),
            5,
            "expected 5 lines (header + sep + 1 body row + top border + bottom border), got {}: {:#?}",
            text.lines.len(),
            text.lines
        );

        let sep: String = text.lines[2]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        // Separator encodes alignment markers
        assert!(
            sep.contains(':'),
            "separator should have ':' for alignment: {sep}"
        );
    }

    #[test]
    fn test_markdown_table_empty() {
        let palette = crate::palette::Palette::default();
        // A table with a header but no body rows.
        let md = "\
| A | B |
|---|---|
";
        let text = markdown_to_text(md, &palette);
        // header + separator
        assert_eq!(
            text.lines.len(),
            4,
            "expected 4 lines (top border + header + sep + bottom border), got {}: {:#?}",
            text.lines.len(),
            text.lines
        );
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
        let mut sel = make_selection(suggestions);
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

    #[test]
    fn test_extract_command_name() {
        assert_eq!(
            extract_command_name("flyline set-agent-mode --command 'claude --effort low --prompt'"),
            Some("claude".to_string())
        );
        assert_eq!(
            extract_command_name(
                "flyline set-agent-mode --command 'copilot --reasoning-effort low --prompt'"
            ),
            Some("copilot".to_string())
        );
        assert_eq!(
            extract_command_name(
                "flyline set-agent-mode --system-prompt 'x' --command 'codex -a never'"
            ),
            Some("codex".to_string())
        );
        assert_eq!(extract_command_name("flyline set-agent-mode --help"), None);
    }

    #[test]
    fn test_parse_example_agent_commands() {
        let results = parse_example_agent_commands();
        // The example file contains copilot, claude, and codex entries (plus one with a trigger prefix).
        assert!(
            results.len() >= 3,
            "expected at least 3 entries, got {}",
            results.len()
        );
        let cmd_names: Vec<&str> = results.iter().map(|(name, _)| name.as_str()).collect();
        assert!(
            cmd_names.contains(&"copilot"),
            "expected 'copilot' in {cmd_names:?}"
        );
        assert!(
            cmd_names.contains(&"claude"),
            "expected 'claude' in {cmd_names:?}"
        );
        assert!(
            cmd_names.contains(&"codex"),
            "expected 'codex' in {cmd_names:?}"
        );
        // Each full command string should start with "flyline set-agent-mode"
        for (_, cmd) in &results {
            assert!(
                cmd.starts_with("flyline set-agent-mode"),
                "unexpected command: {cmd}"
            );
        }
    }
}
