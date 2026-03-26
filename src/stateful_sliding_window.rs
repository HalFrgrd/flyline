/// When you have an index and want to show a "window" of items around that index,
/// such that the window moves as little as possible when the index changes,
/// And you want to try and keep the window BUFFER items away from the edges of the window when possible,
/// so that you have some context around the index of interest,
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatefulSlidingWindow {
    start_index: usize,
    index_of_interest: usize,
    window_size: usize,
}

impl StatefulSlidingWindow {
    pub fn new(index_of_interest: usize, window_size: usize) -> Self {
        let mut self_instance = Self {
            start_index: 0,
            index_of_interest,
            window_size,
        };
        self_instance.fix_window();
        self_instance
    }

    fn buffer(&self) -> usize {
        match self.window_size {
            0 | 1 => 0,
            2 => 0,
            3 | 4 => 1,
            5 | 6 => 2,
            _ => 3,
        }
    }

    fn fix_window(&mut self) {
        let buffer = self.buffer();
        if self.index_of_interest < self.start_index + buffer {
            // Move window up so that index_of_interest is buffer away from the top, or at the top if that's not possible
            self.start_index = self.index_of_interest.saturating_sub(buffer);
        } else if self.index_of_interest >= self.start_index + self.window_size - buffer {
            // Move window down so that index_of_interest is buffer away from the bottom, or at the bottom if that's not possible
            self.start_index = self
                .index_of_interest
                .saturating_sub(self.window_size - buffer - 1);
        }
    }

    pub fn move_index_to(&mut self, new_index_of_interest: usize) {
        self.index_of_interest = new_index_of_interest;
        self.fix_window();
    }

    pub fn update_window_size(&mut self, new_window_size: usize) {
        self.window_size = new_window_size;
        self.fix_window();
    }

    pub fn get_window_range(&self) -> std::ops::Range<usize> {
        self.start_index..(self.start_index + self.window_size)
    }

    pub fn visual_index_of_interest(&self) -> usize {
        self.index_of_interest - self.start_index
    }

    pub fn set_visual_index_of_interest(&mut self, visual_index: usize) {
        self.index_of_interest = self.start_index + visual_index;
        self.fix_window();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stateful_sliding_window() {
        let mut window = StatefulSlidingWindow::new(0, 1);
        assert_eq!(window.get_window_range(), 0..1);

        window.move_index_to(1);
        assert_eq!(window.get_window_range(), 1..2);

        let mut window = StatefulSlidingWindow::new(0, 5);
        assert_eq!(window.get_window_range(), 0..5);

        window.move_index_to(1);
        assert_eq!(window.get_window_range(), 0..5);
        window.move_index_to(2);
        assert_eq!(window.get_window_range(), 0..5);
        window.move_index_to(3);
        assert_eq!(window.get_window_range(), 1..6);
        window.move_index_to(4);
        assert_eq!(window.get_window_range(), 2..7);
        window.move_index_to(5);
        assert_eq!(window.get_window_range(), 3..8);
        window.move_index_to(6);
        assert_eq!(window.get_window_range(), 4..9);
        // Go back up
        window.move_index_to(5);
        assert_eq!(window.get_window_range(), 3..8);
        window.move_index_to(4);
        assert_eq!(window.get_window_range(), 2..7);
        window.move_index_to(3);
        assert_eq!(window.get_window_range(), 1..6);
        window.move_index_to(2);
        assert_eq!(window.get_window_range(), 0..5);
        window.move_index_to(1);
        assert_eq!(window.get_window_range(), 0..5);
        window.move_index_to(0);
        assert_eq!(window.get_window_range(), 0..5);
    }
}
