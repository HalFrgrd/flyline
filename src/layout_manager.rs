use crossterm;
use ratatui::prelude::*;

#[derive(Debug)]
pub struct LayoutManager {
    terminal_height: u16,
    terminal_width: u16,
    range_start: u16,
    range_end: u16,
}

impl LayoutManager {
    pub fn new(terminal_area: Rect) -> Self {
        let starting_cursor_position = crossterm::cursor::position().unwrap();

        let layout_manager = LayoutManager {
            terminal_height: terminal_area.height,
            terminal_width: terminal_area.width,
            range_start: starting_cursor_position.1,
            range_end: terminal_area.height,
        };
        layout_manager
    }

    pub fn update_area(&mut self, terminal_area: Rect) {
        self.terminal_height = terminal_area.height;
        self.terminal_width = terminal_area.width;
    }

    pub fn get_area(&mut self, output_num_lines: u16) -> Rect {
        let desired_area = Rect::new(0, self.range_start, self.terminal_width, output_num_lines);

        if desired_area.bottom() > self.terminal_height {
            let lines_to_scroll = desired_area.bottom().saturating_sub(self.terminal_height);
            log::debug!(
                "Desired area {:?} exceeds terminal height {}, scrolling by {}",
                desired_area,
                self.terminal_height,
                lines_to_scroll,
            );
            self.scroll_by(lines_to_scroll);
        }

        // TODO: check we are in bounds
        let area = Rect::new(0, self.range_start, self.terminal_width, output_num_lines);
        self.range_end = area.bottom();

        area
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

        self.range_start = self.range_start.saturating_sub(lines_to_scroll);
        self.range_end = self.range_end.saturating_sub(lines_to_scroll);
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
