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

    fn scroll_by(&mut self, lines_to_scroll: u16) {
        if lines_to_scroll == 0 {
            return;
        }
        log::debug!("Scrolling by {}", lines_to_scroll,);
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::ScrollUp(lines_to_scroll),
        )
        .unwrap();

        self.drawing_row_start = self.drawing_row_start.saturating_sub(lines_to_scroll);
        // self.drawing_row_end = self.drawing_row_end.saturating_sub(lines_to_scroll);
    }

    pub fn fit_content_to_frame(
        &mut self,
        content: &mut crate::content_builder::Contents,
        frame: &mut Frame,
    ) {
        let frame_area = frame.area();
        assert!(content.width == frame_area.width);
        let content_num_rows = content.height();

        // log::debug!(
        //     "Fitting Content of height {} into Frame area {:?} with range_start {}",
        //     content_num_rows,
        //     frame_area,
        //     self.drawing_row_start,
        // );

        if content_num_rows + self.drawing_row_start > self.terminal_height {
            let lines_to_scroll =
                (content_num_rows + self.drawing_row_start).saturating_sub(self.terminal_height);
            log::debug!(
                "Content height {} plus drawing_row_start {} exceeds terminal height {}, scrolling by {}",
                content_num_rows,
                self.drawing_row_start,
                self.terminal_height,
                lines_to_scroll,
            );
            self.scroll_by(lines_to_scroll);
        }

        // Draw the bottom part of the Content if it exceeds the frame area height
        let frame_buffer_subrange_to_draw =
            (content.height().saturating_sub(frame_area.height))..content.height();
        assert!(
            frame_buffer_subrange_to_draw.len() as u16 <= frame_area.height,
            "FrameBuffer subrange to draw length {} exceeds frame area height {}",
            frame_buffer_subrange_to_draw.len(),
            frame_area.height,
        );

        // Copy the contents of the Content into the Frame at the correct offset
        frame.buffer_mut().reset();
        let offset_into_frame = self.drawing_row_start;
        for (content_row_idx, content_row) in content.buf.iter().enumerate() {
            if !frame_buffer_subrange_to_draw.contains(&(content_row_idx as u16)) {
                continue;
            }
            let frame_row_idx = offset_into_frame as usize + content_row_idx;
            if frame_row_idx < frame_area.height as usize {
                self.drawing_row_end = self.drawing_row_end.max(frame_row_idx as u16 + 1);
                for (x, cell) in content_row.iter().enumerate() {
                    if x < frame_area.width as usize {
                        frame.buffer_mut().content[frame_row_idx * frame_area.width as usize + x] =
                            cell.clone();
                    }
                }
            }
        }
    }

    pub fn post_draw(&mut self, is_running: bool) {
        if !is_running {
            // If we've drawn on the last line, scroll up to make room for the cursor
            self.scroll_by((self.drawing_row_end + 1).saturating_sub(self.terminal_height));

            // Put the cursor just after the drawn content
            crossterm::execute!(
                std::io::stdout(),
                crossterm::cursor::MoveTo(0, self.drawing_row_end),
            )
            .unwrap_or_else(|e| log::error!("{}", e));
        } else {
            // TODO: if we want to keep the terminal emulators cursor in sync while running, do it here.
        }
    }
}
