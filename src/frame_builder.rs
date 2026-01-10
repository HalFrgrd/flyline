use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

pub struct FrameBuilder {
    pub buf: Vec<Vec<Cell>>,
    pub width: u16,
    cursor_pos_x: u16,
    cursor_pos_y: u16,
}

impl FrameBuilder {
    /// Create a new FrameBuilder with an empty buffer for the given area
    pub fn new(width: u16) -> Self {
        FrameBuilder {
            buf: vec![],
            width,
            cursor_pos_x: 0,
            cursor_pos_y: 0,
        }
    }

    /// Get the current cursor position (x, y)
    pub fn cursor_position(&self) -> (u16, u16) {
        (self.cursor_pos_x, self.cursor_pos_y)
    }

    /// Get a mutable reference to the internal buffer
    // pub fn buffer_mut(&mut self) -> &mut Buffer {
    //     &mut self.buf
    // }

    // /// Consume the FrameBuilder and return the buffer
    // pub fn into_buffer(self) -> Buffer {
    //     self.buf
    // }

    // pub fn insert_blank_rows_at_top(&mut self, count: u16) {
    //     let area = self.buf.area();
    //     let blank_raw = vec![Cell::default(); area.width as usize * count as usize];
    //     self.buf.content.splice(0..0, blank_raw);
    // }

    // pub fn insert_blank_rows_at_bottom(&mut self, count: u16) {
    //     let area = self.buf.area();
    //     let blank_raw = vec![Cell::default(); area.width as usize * count as usize];
    //     self.buf.content.extend(blank_raw);
    // }

    pub fn append_blank_row(&mut self) {
        let blank_row = vec![Cell::default(); self.width as usize];
        self.buf.push(blank_row);
    }

    pub fn height(&self) -> u16 {
        self.buf.len() as u16
    }

    /// Write a single span at the current cursor position
    /// Will automatically wrap to the next line if necessary
    pub fn write_span(&mut self, span: &Span) {
        let graphemes = span.styled_graphemes(span.style);
        for graph in graphemes {
            let graph_w = graph.symbol.width() as u16;
            if graph_w + self.cursor_pos_x > self.width {
                self.cursor_pos_y += 1;
                self.cursor_pos_x = 0;
            }

            let next_graph_x = self.cursor_pos_x + graph_w;
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

            for _ in self.buf.len()..=self.cursor_pos_y as usize {
                self.append_blank_row();
            }
            self.buf[self.cursor_pos_y as usize][self.cursor_pos_x as usize]
                .set_symbol(&graph.symbol)
                .set_style(graph.style);
            self.cursor_pos_x += 1;
            // Reset following cells if multi-width (they would be hidden by the grapheme),
            while self.cursor_pos_x < next_graph_x {
                self.buf[self.cursor_pos_y as usize][self.cursor_pos_x as usize].reset();
                self.cursor_pos_x += 1;
            }
        }
    }

    /// Write a line at the current cursor position
    /// If insert_new_line is true, moves to the next line after writing
    pub fn write_line(&mut self, line: &Line, insert_new_line: bool) {
        for span in &line.spans {
            self.write_span(span);
        }
        if insert_new_line {
            self.cursor_pos_y += 1;
            self.cursor_pos_x = 0;
        }
    }

    /// Move to the next line (carriage return + line feed)
    pub fn newline(&mut self) {
        self.cursor_pos_y += 1;
        self.cursor_pos_x = 0;
    }

    pub fn set_style(&mut self, area: Rect, style: ratatui::style::Style) {
        for _ in self.buf.len()..area.bottom() as usize {
            self.append_blank_row();
        }

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(row) = self.buf.get_mut(y as usize) {
                    if let Some(cell) = row.get_mut(x as usize) {
                        cell.set_style(style);
                    }
                }
            }
        }
    }
}
