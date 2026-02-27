use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, StyledGrapheme};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coord {
    pub row: u16,
    pub col: u16,
}

impl Coord {
    pub fn new(row: u16, col: u16) -> Self {
        Coord { row, col }
    }

    pub fn abs_diff(&self, other: &Coord) -> usize {
        self.col.abs_diff(other.col) as usize + self.row.abs_diff(other.row) as usize
    }

    pub fn interpolate(&self, other: &Coord, factor: f32) -> Coord {
        // factor = 0.0 => self
        // factor = 1.0 => other
        let col = self.col as f32 + (other.col as f32 - self.col as f32) * factor;
        let row = self.row as f32 + (other.row as f32 - self.row as f32) * factor;
        Coord::new(row as u16, col as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Blank,
    Ps1Prompt,
    Ps2Prompt,
    Command(usize),
    TabSuggestion,
    Suggestion(usize),
    HistorySuggestion,
    FuzzySearch,
    HistoryResult(usize),
    Tooltip,
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
            tag: Tag::Blank,
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
    cursor_pos: Coord, // visual cursor position with line wrapping
    pub edit_cursor_pos: Option<Coord>,
}

impl Contents {
    // All the line wrapping logic is handled here.
    // So app::ui just handles lines according to the edit buffer

    /// Create a new Content with an empty buffer for the given area
    pub fn new(width: u16) -> Self {
        Contents {
            buf: vec![],
            width,
            cursor_pos: Coord::new(0, 0),
            edit_cursor_pos: None,
        }
    }

    /// Get the current cursor position (x, y)
    pub fn cursor_position(&self) -> Coord {
        self.cursor_pos
    }

    pub fn increase_buf_single_row(&mut self) {
        let blank_row = vec![TaggedCell::default(); self.width as usize];
        self.buf.push(blank_row);
    }

    pub fn height(&self) -> u16 {
        self.buf.len() as u16
    }

    pub fn move_to_next_insertion_point(&mut self, graph: &StyledGrapheme, overwrite: bool) {
        let graph_w = graph.symbol.width() as u16;
        loop {
            if self.cursor_pos.row >= self.buf.len() as u16 {
                self.increase_buf_single_row();
            } else if self.cursor_pos.col as usize + graph_w as usize > self.width as usize {
                // log::debug!("Wrapping line for grapheme of width {}", graph_w);
                self.cursor_pos.row += 1;
                self.cursor_pos.col = 0;
            } else if !overwrite
                && self.buf[self.cursor_pos.row as usize][(self.cursor_pos.col as usize)
                    ..(self.cursor_pos.col as usize + graph_w as usize)]
                    .iter()
                    .all(|cell| cell.tag == Tag::Blank)
            {
                break;
            } else if overwrite {
                break;
            } else {
                self.cursor_pos.col += 1;
            }
        }
    }

    /// Write a single span at the current cursor position
    /// Will automatically wrap to the next line if necessary
    fn write_span_internal(&mut self, span: &Span, mut tag: Tag, overwrite: bool) {
        let graphemes = span.styled_graphemes(span.style);
        for graph in graphemes {
            let graph_w = graph.symbol.width() as u16;

            self.move_to_next_insertion_point(&graph, overwrite);

            let next_graph_x = self.cursor_pos.col + graph_w;
            if next_graph_x > self.width {
                // cold_path();
                // If the grapheme is still too wide after wrapping, skip it
                // We probably start at cursor_pos_x=0 here, so very unlikely to happen
                log::warn!(
                    "Grapheme too wide for line: '{}' (width {})",
                    graph.symbol,
                    graph_w
                );
                continue;
            }

            self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                .update(&graph, tag);
            self.cursor_pos.col += 1;
            // Reset following cells if multi-width (they would be hidden by the grapheme),
            while self.cursor_pos.col < next_graph_x {
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                    .cell
                    .reset();
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize].tag = tag;
                self.cursor_pos.col += 1;
            }
            if let Tag::Command(byte_start) = tag {
                tag = Tag::Command(byte_start + graph.symbol.len());
            }
        }
    }

    pub fn write_span_dont_overwrite(&mut self, span: &Span, tag: Tag) {
        self.write_span_internal(span, tag, false);
    }

    pub fn write_span(&mut self, span: &Span, tag: Tag) {
        self.write_span_internal(span, tag, true);
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

    pub fn write_line_lrjustified(
        &mut self,
        l_line: &Line,
        fill_span: &Span,
        r_line: &Line,
        tag: Tag,
        leave_cursor_after_l_line: bool,
    ) {
        let r_width = r_line.width() as u16;
        let starting_row = self.cursor_pos.row;
        self.write_line(l_line, false, tag);

        let cursor_after_l_line = self.cursor_pos.col;

        if self.cursor_pos.row == starting_row {
            if fill_span.content.width() != 1 {
                log::warn!(
                    "Fill span content '{}' is not width 1, defaulting to space",
                    fill_span.content
                );
                // If the fill char is not width 1, treat it as a space
                self.cursor_pos.col = self.width.saturating_sub(r_width);
            } else if fill_span.content == " "
                && fill_span.style == ratatui::style::Style::default()
            {
                // If filling with unstyled spaces, we can just move the cursor to the right position without writing fill chars
                self.cursor_pos.col = self.width.saturating_sub(r_width);
            } else {
                for _ in self.cursor_pos.col..self.width.saturating_sub(r_width) {
                    self.write_span(fill_span, tag);
                }
            }
        }
        if r_width > 0 {
            self.write_line(r_line, false, tag);
        }

        if leave_cursor_after_l_line {
            self.cursor_pos.row = starting_row;
            self.cursor_pos.col = cursor_after_l_line;
        }
    }

    /// Fill the rest of the current row with spaces tagged with the given tag
    pub fn fill_line(&mut self, tag: Tag) {
        let remaining = self.width.saturating_sub(self.cursor_pos.col) as usize;
        if remaining > 0 {
            self.write_span(&Span::raw(" ".repeat(remaining)), tag);
        }
    }

    /// Move to the next line (carriage return + line feed)
    pub fn newline(&mut self) {
        self.cursor_pos.row += 1;
        self.cursor_pos.col = 0;
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

    pub fn set_edit_cursor_style(
        &mut self,
        cursor: Coord,
        style: ratatui::style::Style,
    ) {
        self.edit_cursor_pos = Some(cursor);
        self.set_style(Rect::new(cursor.col, cursor.row, 1, 1), style);
    }

    pub fn get_row_range_to_show(&self, height: u16) -> (u16, u16) {
        // Returns the range of visual rows to show given the available height
        let total_rows = self.height();
        if total_rows <= height {
            (0, total_rows)
        } else if let Some(cursor) = self.edit_cursor_pos {
            let bottom = std::cmp::min(cursor.row.saturating_add(1), total_rows);
            let top = bottom.saturating_sub(height);
            (top, bottom)
        } else {
            // Show the final rows
            (total_rows.saturating_sub(height), total_rows)
        }
    }

    pub fn get_tagged_cell(
        &self,
        term_em_x: u16,
        term_em_y: u16,
        term_em_offset: i16,
    ) -> Option<(Tag, bool)> {
        // log::debug!(
        //     "Getting tagged cell at terminal em coords ({}, {}), offset {}",
        //     term_em_x,
        //     term_em_y,
        //     term_em_offset
        // );
        if term_em_offset > term_em_y as i16 {
            // log::debug!(
            //     "Offset {} is greater than term_em_y {}, returning None",
            //     term_em_offset,
            //     term_em_y
            // );
            return None;
        }

        let direct_contact = self
            .buf
            .get(term_em_y.saturating_sub_signed(term_em_offset) as usize)
            .and_then(|row| row.get(term_em_x as usize));

        if direct_contact.is_some_and(|cell| {
            matches!(
                cell.tag,
                Tag::Command(_) | Tag::Suggestion(_) | Tag::HistoryResult(_)
            )
        }) {
            return direct_contact.map(|cell| (cell.tag, true));
        }

        self.buf
            .get(term_em_y.saturating_sub_signed(term_em_offset) as usize)
            .and_then(|row| {
                row.iter().enumerate().rev().find(|(col_idx, tagged_cell)| {
                    *col_idx <= term_em_x as usize && matches!(tagged_cell.tag, Tag::Command(_))
                })
            })
            .map(|(_, cell)| (cell.tag, false))
    }
}
