use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

pub struct FrameBuilder {
    buf: Buffer,
    cursor_pos_x: usize,
    cursor_pos_y: usize,
}

impl FrameBuilder {
    /// Create a new FrameBuilder with an empty buffer for the given area
    pub fn new(area: Rect) -> Self {
        FrameBuilder {
            buf: Buffer::empty(area),
            cursor_pos_x: 0,
            cursor_pos_y: 0,
        }
    }

    /// Get the current cursor position (x, y)
    pub fn cursor_position(&self) -> (usize, usize) {
        (self.cursor_pos_x, self.cursor_pos_y)
    }

    /// Get a mutable reference to the internal buffer
    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buf
    }

    /// Consume the FrameBuilder and return the buffer
    pub fn into_buffer(self) -> Buffer {
        self.buf
    }

    pub fn insert_blank_rows_at_top(&mut self, count: u16) {
        let area = self.buf.area();
        let blank_raw = vec![Cell::default(); area.width as usize * count as usize];
        self.buf.content.splice(0..0, blank_raw);
    }

    /// Write a single span at the current cursor position
    pub fn write_span(&mut self, span: &Span) {
        let graphemes = span.styled_graphemes(span.style);
        for graph in graphemes {
            let w = graph.symbol.width();
            if w + self.cursor_pos_x >= self.buf.area().width as usize {
                self.cursor_pos_y += 1;
                self.cursor_pos_x = 0;
            }
            assert!(w + self.cursor_pos_x < self.buf.area().width as usize);

            self.buf.set_stringn(
                self.cursor_pos_x.try_into().unwrap_or(0),
                self.cursor_pos_y.try_into().unwrap_or(0),
                graph.symbol,
                w,
                graph.style,
            );
            self.cursor_pos_x += w;
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
}
