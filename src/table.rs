use crate::content_builder::split_line_to_terminal_rows;
use pulldown_cmark::Alignment;
use ratatui::prelude::*;

/// Accumulated data for a single table being built.
pub struct TableAccum {
    pub alignments: Vec<Alignment>,
    /// Cells of the header row.
    pub header_cells: Vec<String>,
    /// Body rows (each row is a list of cell strings).
    pub body_rows: Vec<Vec<String>>,
    /// Cells of the row currently being built.
    pub current_cells: Vec<String>,
    /// Text content being accumulated for the current cell.
    pub current_cell_buf: String,
    /// True while processing the header row.
    pub in_header: bool,
}

impl TableAccum {
    pub fn new(alignments: Vec<Alignment>) -> Self {
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

impl Default for TableAccum {
    fn default() -> Self {
        Self::new(vec![])
    }
}

/// Compute column widths from the natural (content-length) widths of a
/// [`TableAccum`].  Each column is at least 3 characters wide so that
/// separator dashes look reasonable.
pub fn compute_natural_col_widths(accum: &TableAccum) -> Vec<usize> {
    let ncols = accum.header_cells.len();
    let mut col_widths: Vec<usize> = accum.header_cells.iter().map(|s| s.len()).collect();
    for row in &accum.body_rows {
        for (j, cell) in row.iter().enumerate() {
            if j < ncols {
                col_widths[j] = col_widths[j].max(cell.len());
            }
        }
    }
    for w in &mut col_widths {
        *w = (*w).max(3);
    }
    col_widths
}

/// Wrap a cell string to fit within `col_width` terminal columns.
/// Returns one string per wrapped display row.
fn wrap_cell(cell: &str, col_width: usize) -> Vec<String> {
    if col_width == 0 {
        return vec![String::new()];
    }
    let line = Line::from(Span::raw(cell.to_string()));
    split_line_to_terminal_rows(&line, col_width as u16)
        .into_iter()
        .map(|row| {
            row.spans
                .into_iter()
                .map(|s| s.content.into_owned())
                .collect()
        })
        .collect()
}

/// Render a collected [`TableAccum`] into ratatui [`Line`]s using the given
/// column widths.  Cells wider than their column are wrapped with
/// [`split_line_to_terminal_rows`].
///
/// Produces an ASCII-box table:
/// ```text
/// ╭────────┬──────────╮
/// │ Header │ Header2  │
/// ├────────┼──────────┤
/// │ cell   │ cell     │
/// ╰────────┴──────────╯
/// ```
pub fn render_table(accum: &TableAccum, col_widths: &[usize]) -> Vec<Line<'static>> {
    let ncols = accum.header_cells.len();
    if ncols == 0 {
        return Vec::new();
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

    // Render a logical table row (whose cells may wrap) into one or more
    // display lines. The first display line uses the given `bold` style;
    // subsequent continuation lines are always plain.
    let build_multiline_row = |cells: &[String], bold: bool| -> Vec<Line<'static>> {
        // Wrap each cell to its column width.
        let wrapped: Vec<Vec<String>> = cells
            .iter()
            .enumerate()
            .map(|(j, cell)| {
                let w = col_widths.get(j).copied().unwrap_or(0);
                wrap_cell(cell, w)
            })
            .collect();

        let max_lines = wrapped.iter().map(|c| c.len()).max().unwrap_or(1);

        (0..max_lines)
            .map(|line_idx| {
                // For each column, pick the wrapped line at this index or an
                // empty string if the cell wrapped to fewer lines.
                let row_cells: Vec<String> = (0..ncols)
                    .map(|j| {
                        let w = col_widths.get(j).copied().unwrap_or(0);
                        let s = wrapped.get(j).and_then(|c| c.get(line_idx));
                        // Pad to the column width.
                        format!("{:<w$}", s.map(|x| x.as_str()).unwrap_or(""), w = w)
                    })
                    .collect();
                // Only apply bold on the first wrapped line of the header row.
                build_row(&row_cells, bold && line_idx == 0)
            })
            .collect()
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(build_top_border());
    lines.extend(build_multiline_row(&accum.header_cells, true));
    lines.push(build_separator());
    for row in &accum.body_rows {
        // Pad row to the expected number of columns.
        let mut padded = row.clone();
        while padded.len() < ncols {
            padded.push(String::new());
        }
        lines.extend(build_multiline_row(&padded, false));
    }
    lines.push(build_bottom_border());
    lines
}
