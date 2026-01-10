use crate::content_builder::Contents;
use crossterm;
use ratatui::prelude::*;

#[derive(Debug)]
pub struct LayoutManager {
    terminal_height: u16,
    // terminal_width: u16,
    drawing_row_start: u16,
    drawing_row_end: u16,
}

impl LayoutManager {
    pub fn new(terminal_area: Rect) -> Self {
        let starting_cursor_position = crossterm::cursor::position().unwrap();

        let layout_manager = LayoutManager {
            terminal_height: terminal_area.height,
            // terminal_width: terminal_area.width,
            drawing_row_start: starting_cursor_position.1, // we can draw from here downwards
            drawing_row_end: starting_cursor_position.1, // used to keep track of how far down the terminal we've drawn, exclusive
        };
        layout_manager
    }

    fn scroll_by(&self, lines_to_scroll: u16) {
        if lines_to_scroll == 0 {
            return;
        }
        log::debug!("Scrolling by {}", lines_to_scroll,);
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::ScrollUp(lines_to_scroll),
        )
        .unwrap();
    }

    pub fn fit_content_to_frame(&mut self, content: &Contents, frame: &mut Frame) {
        let frame_area = frame.area();
        assert!(content.width == frame_area.width);
        let content_num_rows = content.height();

        assert!(self.terminal_height == frame_area.height);

        let lines_to_scroll =
            (self.drawing_row_start + content_num_rows).saturating_sub(self.terminal_height);
        let mut scrolled = false;
        if self.drawing_row_start > 0 && lines_to_scroll > 0 {
            self.scroll_by(lines_to_scroll);
            self.drawing_row_start = self.drawing_row_start.saturating_sub(lines_to_scroll);
            // When we scroll, it messes up ratatui's buffer diffing
            scrolled = true;
        }

        let content_row_idx_to_frame_row_idx = |content_row_idx: u16| -> Option<u16> {
            let num_rows_available_in_frame =
                self.terminal_height.saturating_sub(self.drawing_row_start);

            let content_row_start_idx: u16;
            let content_row_end_idx: u16;
            if num_rows_available_in_frame < content_num_rows {
                // We need to choose a subset of the content rows to show
                if let Some((_, edit_cursor_row)) = content.edit_cursor_pos {
                    // Try to center the edit cursor row in the available space
                    if edit_cursor_row >= num_rows_available_in_frame / 2 {
                        content_row_start_idx = edit_cursor_row
                            .saturating_sub(num_rows_available_in_frame / 2)
                            .min(content_num_rows.saturating_sub(num_rows_available_in_frame));
                    } else {
                        content_row_start_idx = 0;
                    }
                } else {
                    // No edit cursor position, just show from the end
                    content_row_start_idx =
                        content_num_rows.saturating_sub(num_rows_available_in_frame);
                }
                content_row_end_idx = content_row_start_idx + num_rows_available_in_frame;
            } else {
                // Show all content rows
                content_row_start_idx = 0;
                content_row_end_idx = content_num_rows;
            }

            if content_row_start_idx <= content_row_idx && content_row_idx < content_row_end_idx {
                let frame_row_idx = self.drawing_row_start
                    + (content_row_idx.saturating_sub(content_row_start_idx));
                if frame_row_idx < self.terminal_height {
                    return Some(frame_row_idx);
                } else {
                    return None;
                }
            } else {
                return None;
            }
        };

        // Copy the contents of the Content into the Frame at the correct offset
        frame.buffer_mut().reset();
        self.drawing_row_end = self.drawing_row_start;
        for (content_row_idx, content_row) in content.buf.iter().enumerate() {
            if let Some(frame_row_idx) = content_row_idx_to_frame_row_idx(content_row_idx as u16) {
                self.drawing_row_end = self.drawing_row_end.max(frame_row_idx as u16 + 1);
                for (x, cell) in content_row.iter().enumerate() {
                    if x < frame_area.width as usize {
                        let mut new_cell = cell.clone();
                        if scrolled {
                            // Try this rarely implemented modifier to force a redraw of the cell
                            let style = new_cell.style();
                            new_cell.set_style(style.add_modifier(Modifier::SLOW_BLINK));
                        }

                        frame.buffer_mut().content
                            [frame_row_idx as usize * frame_area.width as usize + x] = new_cell;
                    }
                }
            }
        }
    }

    pub fn post_draw(&mut self, is_running: bool) {
        if !is_running {
            // If the terminal is height 10, and self.drawing_row_end is 10, we need to scroll up 1 row
            // If the terminal is height 10, and self.drawing_row_end is 9, we don't need to scroll
            let rows_to_scroll = (self.drawing_row_end + 1).saturating_sub(self.terminal_height);
            self.scroll_by(rows_to_scroll);
            let target_row = self.drawing_row_end.saturating_sub(rows_to_scroll);

            // Put the cursor just after the drawn content
            crossterm::execute!(std::io::stdout(), crossterm::cursor::MoveTo(0, target_row),)
                .unwrap_or_else(|e| log::error!("{}", e));
        } else {
            // TODO: if we want to keep the terminal emulator's cursor in sync while running, do it here.
        }
    }
}
