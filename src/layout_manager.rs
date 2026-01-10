use crossterm;
use ratatui::prelude::*;

#[derive(Debug)]
pub struct LayoutManager {
    terminal_height: u16,
    // terminal_width: u16,
    range_start: u16,
    range_end: u16,
}

impl LayoutManager {
    pub fn new(terminal_area: Rect) -> Self {
        let starting_cursor_position = crossterm::cursor::position().unwrap();

        let layout_manager = LayoutManager {
            terminal_height: terminal_area.height,
            // terminal_width: terminal_area.width,
            range_start: starting_cursor_position.1,
            range_end: terminal_area.height,
        };
        layout_manager
    }

    pub fn update_area(&mut self, terminal_area: Rect) {
        self.terminal_height = terminal_area.height;
        // self.terminal_width = terminal_area.width;
    }

    // pub fn get_area(&mut self, output_num_lines: u16) -> Rect {
    //     let desired_area = Rect::new(0, self.range_start, self.terminal_width, output_num_lines);

    //     if desired_area.bottom() > self.terminal_height {
    //         let lines_to_scroll = desired_area.bottom().saturating_sub(self.terminal_height);
    //         log::debug!(
    //             "Desired area {:?} exceeds terminal height {}, scrolling by {}",
    //             desired_area,
    //             self.terminal_height,
    //             lines_to_scroll,
    //         );
    //         self.scroll_by(lines_to_scroll);
    //     }

    //     // TODO: check we are in bounds
    //     let area = Rect::new(0, self.range_start, self.terminal_width, output_num_lines);
    //     self.range_end = area.bottom();

    //     area
    // }

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

        self.range_start = self.range_start.saturating_sub(lines_to_scroll);
        self.range_end = self.range_end.saturating_sub(lines_to_scroll);
    }

    pub fn fit_frame_builder_to_frame(
        &mut self,
        fb: &mut crate::frame_builder::FrameBuilder,
        frame: &mut Frame,
    ) {
        let frame_area = frame.area();
        assert!(fb.width == frame_area.width);
        let num_rows_to_draw = fb.height();
        log::debug!(
            "Fitting FrameBuilder of height {} into Frame area {:?} with range_start {}",
            num_rows_to_draw,
            frame_area,
            self.range_start,
        );

        if num_rows_to_draw + self.range_start > self.terminal_height {
            let lines_to_scroll =
                (num_rows_to_draw + self.range_start).saturating_sub(self.terminal_height);
            log::debug!(
                "FrameBuilder height {} plus range_start {} exceeds terminal height {}, scrolling by {}",
                num_rows_to_draw,
                self.range_start,
                self.terminal_height,
                lines_to_scroll,
            );
            self.scroll_by(lines_to_scroll);
        }

        // Copy the contents of the FrameBuilder into the Frame at the correct offset
        frame.buffer_mut().reset();
        let offset_into_frame = self.range_start;
        for (y, row) in fb.buf.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                let frame_y = offset_into_frame as usize + y;
                if frame_y < frame_area.height as usize && x < frame_area.width as usize {
                    frame.buffer_mut().content[frame_y * frame_area.width as usize + x] =
                        cell.clone();
                }
            }
        }
    }

    pub fn finalize(&mut self) {
        log::debug!("Finalizing layout pre  scroll  {:?}", self);
        self.scroll_by((self.range_end + 1).saturating_sub(self.terminal_height));
        log::debug!("Finalizing layout post scroll  {:?}", self);

        crossterm::execute!(
            std::io::stdout(),
            crossterm::cursor::MoveTo(0, self.range_end,),
        )
        .unwrap();
    }
}
