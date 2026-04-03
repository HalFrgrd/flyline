use anyhow::{anyhow, bail};
use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use ratatui::prelude::*;
use ratatui::text::Text;
use regex::Regex;
use std::sync::OnceLock;

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

/// Accumulated data for a single markdown table being parsed.
struct TableAccum {
    alignments: Vec<Alignment>,
    /// Cells of the header row.
    header_cells: Vec<String>,
    /// Body rows (each row is a list of cell strings).
    body_rows: Vec<Vec<String>>,
    /// Cells of the row currently being built.
    current_cells: Vec<String>,
    /// Text content being accumulated for the current cell.
    current_cell_buf: String,
    /// True while processing the `<thead>` row.
    in_header: bool,
}

impl TableAccum {
    fn new(alignments: Vec<Alignment>) -> Self {
        TableAccum {
            alignments,
            header_cells: Vec::new(),
            body_rows: Vec::new(),
            current_cells: Vec::new(),
            current_cell_buf: String::new(),
            in_header: false,
        }
    }
}

/// Render a collected [`TableAccum`] into ratatui [`Line`]s.
///
/// Produces an ASCII-box table:
/// ```text
/// | Header1   | Header2   |
/// |-----------|-----------|
/// | cell      | cell      |
/// ```
fn render_table(accum: &TableAccum) -> Vec<Line<'static>> {
    let ncols = accum.header_cells.len();
    if ncols == 0 {
        return Vec::new();
    }

    // Calculate minimum column widths from cell contents.
    let mut col_widths: Vec<usize> = accum.header_cells.iter().map(|s| s.len()).collect();
    for row in &accum.body_rows {
        for (j, cell) in row.iter().enumerate() {
            if j < ncols {
                col_widths[j] = col_widths[j].max(cell.len());
            }
        }
    }
    // Ensure a minimum column width of 3 so short separator dashes look reasonable.
    for w in &mut col_widths {
        *w = (*w).max(3);
    }

    let build_top_border = || -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::raw("╭─"));
        for (j, &width) in col_widths.iter().enumerate() {
            spans.push(Span::raw("─".repeat(width)));
            if j + 1 < col_widths.len() {
                spans.push(Span::raw("─┬─"));
            }
        }
        spans.push(Span::raw("─╮"));
        Line::from(spans)
    };

    let build_bottom_border = || -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::raw("╰─"));
        for (j, &width) in col_widths.iter().enumerate() {
            spans.push(Span::raw("─".repeat(width)));
            if j + 1 < col_widths.len() {
                spans.push(Span::raw("─┴─"));
            }
        }
        spans.push(Span::raw("─╯"));
        Line::from(spans)
    };

    let build_row = |cells: &[String], bold: bool| -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::raw("│ "));
        for (j, cell) in cells.iter().enumerate() {
            let width = col_widths.get(j).copied().unwrap_or(0);
            let padded = format!("{:<width$}", cell, width = width);
            if bold {
                spans.push(Span::styled(
                    padded,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw(padded));
            }
            spans.push(Span::raw(" │ "));
        }
        // Remove the trailing " │ " so the line ends with " │".
        if spans.len() > 1 {
            let last = spans.pop().unwrap();
            // last is " │ " — push " │" instead
            let trimmed = last.content.trim_end().to_string();
            spans.push(Span::raw(trimmed));
        }
        Line::from(spans)
    };

    let build_separator = || -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::raw("├─"));
        for (j, &width) in col_widths.iter().enumerate() {
            let dashes = match accum.alignments.get(j) {
                Some(Alignment::Center) => {
                    let inner = "─".repeat(width.saturating_sub(2));
                    format!(":{inner}:")
                }
                Some(Alignment::Right) => {
                    let inner = "─".repeat(width.saturating_sub(1));
                    format!("{inner}:")
                }
                Some(Alignment::Left) => {
                    let inner = "─".repeat(width.saturating_sub(1));
                    format!(":{inner}")
                }
                _ => "─".repeat(width),
            };
            spans.push(Span::raw(dashes));
            if j + 1 < col_widths.len() {
                spans.push(Span::raw("─┼─"));
            }
        }
        spans.push(Span::raw("─┤"));
        Line::from(spans)
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(build_top_border());
    lines.push(build_row(&accum.header_cells, true));
    lines.push(build_separator());
    for row in &accum.body_rows {
        // Pad row to the expected number of columns.
        let mut padded = row.clone();
        while padded.len() < ncols {
            padded.push(String::new());
        }
        lines.push(build_row(&padded, false));
    }
    lines.push(build_bottom_border());
    lines
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
                    lines.extend(render_table(&accum));
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

/// Parse AI command output into an [`AiOutputParsed`].
///
/// The output may contain prose before and/or after the JSON array.
/// We use a regex to locate the start of the JSON array and then attempt to
/// parse it with `serde_json`.  Text before the array is stored as the
/// `header` and text after the array is stored as the `footer`.
/// Returns `Err` if no valid JSON array with at least one non-empty command
/// can be found.  The raw output is always logged at DEBUG level.
pub fn parse_ai_output(raw: &str) -> anyhow::Result<AiOutputParsed> {
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
            Ok(AiOutputParsed {
                suggestions,
                header,
                footer,
            })
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
            4,
            "expected 4 lines (header + sep + 2 rows), got {}: {:#?}",
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
        assert!(plain[0].contains("Command"), "header: {}", plain[0]);
        assert!(plain[0].contains("Description"), "header: {}", plain[0]);
        // Separator row contains box-drawing dashes.
        assert!(plain[1].contains('─'), "separator: {}", plain[1]);
        // Body rows contain cell values.
        assert!(plain[2].contains("ls"), "row1: {}", plain[2]);
        assert!(plain[2].contains("List files"), "row1: {}", plain[2]);
        assert!(plain[3].contains("pwd"), "row2: {}", plain[3]);
        assert!(plain[3].contains("Print working dir"), "row2: {}", plain[3]);
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
            3,
            "expected 3 lines, got {}: {:#?}",
            text.lines.len(),
            text.lines
        );

        let sep: String = text.lines[1]
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
            2,
            "expected 2 lines (header + sep), got {}: {:#?}",
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
}
