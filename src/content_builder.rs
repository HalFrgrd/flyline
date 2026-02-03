use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, StyledGrapheme};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Normal,
    Ps1Prompt,
    Ps2Prompt,
    CommandFirstWord,
    CommandOther,
    TabSuggestion,
    HistorySuggestion,
    FuzzySearch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedCell {
    pub cell: Cell,
    pub tag: Tag,
}

impl Default for TaggedCell {
    fn default() -> Self {
        TaggedCell {
            cell: Cell::default(),
            tag: Tag::Normal,
        }
    }
}

impl TaggedCell {
    pub fn update(&mut self, graph: &StyledGrapheme, tag: Tag) {
        self.cell.set_symbol(&graph.symbol).set_style(graph.style);
        self.tag = tag;
    }
}

pub struct Contents {
    pub buf: Vec<Vec<TaggedCell>>, // each inner Vec is a row of Cells of width `width`
    pub width: u16,
    cursor_vis_col: u16,
    cursor_vis_row: u16,              // visual cursor position with line wrapping
    logical_row_to_vis_row: Vec<u16>, // mapping from logical row to visual row
    pub edit_cursor_pos: Option<(u16, u16)>,
}

impl Contents {
    // All the line wrapping logic is handled here.
    // So app::ui just handles lines according to the edit buffer

    /// Create a new Content with an empty buffer for the given area
    pub fn new(width: u16) -> Self {
        Contents {
            buf: vec![],
            width,
            cursor_vis_col: 0,
            cursor_vis_row: 0,
            logical_row_to_vis_row: vec![0],
            edit_cursor_pos: None,
        }
    }

    /// Get the current cursor position (x, y)
    pub fn cursor_position(&self) -> (u16, u16) {
        (self.cursor_vis_col, self.cursor_vis_row)
    }

    pub fn increase_buf_single_row(&mut self) {
        let blank_row = vec![TaggedCell::default(); self.width as usize];
        self.buf.push(blank_row);
    }

    pub fn height(&self) -> u16 {
        self.buf.len() as u16
    }

    /// Write a single span at the current cursor position
    /// Will automatically wrap to the next line if necessary
    pub fn write_span(&mut self, span: &Span, tag: Tag) {
        let graphemes = span.styled_graphemes(span.style);
        for graph in graphemes {
            let graph_w = graph.symbol.width() as u16;
            if graph_w + self.cursor_vis_col > self.width {
                self.cursor_vis_row += 1;
                self.cursor_vis_col = 0;
            }

            let next_graph_x = self.cursor_vis_col + graph_w;
            if next_graph_x > self.width {
                // If the grapheme is still too wide after wrapping, skip it
                // We probably start at cursor_pos_x=0 here, so very unlikely to happen
                log::warn!(
                    "Grapheme too wide for line: '{}' (width {})",
                    graph.symbol,
                    graph_w
                );
                continue;
            }

            for _ in self.buf.len()..=self.cursor_vis_row as usize {
                self.increase_buf_single_row();
            }
            self.buf[self.cursor_vis_row as usize][self.cursor_vis_col as usize]
                .update(&graph, tag);
            self.cursor_vis_col += 1;
            // Reset following cells if multi-width (they would be hidden by the grapheme),
            while self.cursor_vis_col < next_graph_x {
                self.buf[self.cursor_vis_row as usize][self.cursor_vis_col as usize]
                    .cell
                    .reset();
                self.cursor_vis_col += 1;
            }
        }
    }

    /// Write a line at the current cursor position
    /// If insert_new_line is true, moves to the next line after writing
    pub fn write_line(&mut self, line: &Line, insert_new_line: bool, tag: Tag) {
        for span in &line.spans {
            self.write_span(span, tag);
        }
        if insert_new_line {
            self.newline();
        }
    }

    /// Move to the next line (carriage return + line feed)
    pub fn newline(&mut self) {
        self.cursor_vis_row += 1;
        self.logical_row_to_vis_row.push(self.cursor_vis_row);
        self.cursor_vis_col = 0;
    }

    fn set_style(&mut self, area: Rect, style: ratatui::style::Style) {
        for _ in self.buf.len()..area.bottom() as usize {
            self.increase_buf_single_row();
        }

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(row) = self.buf.get_mut(y as usize) {
                    if let Some(tagged_cell) = row.get_mut(x as usize) {
                        tagged_cell.cell.set_style(style);
                    }
                }
            }
        }
    }

    pub fn cursor_logical_row(&self) -> u16 {
        self.logical_row_to_vis_row.len().saturating_sub(1) as u16
    }

    pub fn cursor_logical_to_visual(&self, logical_row: u16, logical_col: u16) -> (u16, u16) {
        // takes coordinates in the formatted edit buffer
        // and returns the coordinates in the visual buffer with line wrapping.
        // These correspond to the coordinates on the terminal screen up to row translation by layout manager
        let wrapped_row_offset_from_this_line = logical_col / self.width;
        let mut wrapped_cursor_row = self
            .logical_row_to_vis_row
            .get(logical_row as usize)
            .cloned()
            .unwrap_or(0);
        wrapped_cursor_row += wrapped_row_offset_from_this_line as u16;
        let wrapped_cursor_col = logical_col % self.width;
        (wrapped_cursor_row, wrapped_cursor_col)
    }

    pub fn set_edit_cursor_style(
        &mut self,
        vis_row: u16,
        vis_col: u16,
        style: ratatui::style::Style,
    ) {
        self.edit_cursor_pos = Some((vis_col, vis_row));
        self.set_style(Rect::new(vis_col, vis_row, 1, 1), style);
    }

    pub fn get_row_range_to_show(&self, height: u16) -> (u16, u16) {
        // Returns the range of visual rows to show given the available height
        let total_rows = self.height();
        if total_rows <= height {
            (0, total_rows)
        } else if let Some((_, vis_row)) = self.edit_cursor_pos {
            let bottom = std::cmp::min(vis_row.saturating_add(1), total_rows);
            let top = bottom.saturating_sub(height);
            (top, bottom)
        } else {
            // Show the final rows
            (total_rows.saturating_sub(height), total_rows)
        }
    }
}
