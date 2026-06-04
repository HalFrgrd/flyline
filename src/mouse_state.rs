use crate::content_builder::Tag;
use crate::settings::MouseMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickCount {
    None,
    Single,
    Double,
    Triple,
}

pub struct MouseState {
    enabled: bool,
    last_left_click_times: Vec<std::time::Instant>,
    last_left_click_buffer_pos: Option<usize>,
    /// True while the left mouse button is currently being held down.
    /// Set on `MouseEventKind::Down(Left)` and cleared on `MouseEventKind::Up(Left)`.
    left_button_down: bool,
    pub last_mouse_over_cell: Option<Tag>,
    pub drag_start_tag: Option<Tag>,
}

impl MouseState {
    /// Initialize mouse state for the given mode, immediately enabling mouse capture
    /// (via crossterm) when appropriate.
    pub fn initialize(mode: &MouseMode) -> Self {
        let enabled = match mode {
            MouseMode::Disabled => false,
            MouseMode::Simple | MouseMode::Smart => {
                match crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture) {
                    Ok(_) => {
                        log::trace!("Mouse capture enabled: initial setup for {:?} mode", mode);
                        true
                    }
                    Err(e) => {
                        log::error!("Failed to enable mouse capture on init: {}", e);
                        false
                    }
                }
            }
        };
        MouseState {
            enabled,
            last_left_click_times: Vec::new(),
            last_left_click_buffer_pos: None,
            left_button_down: false,
            last_mouse_over_cell: None,
            drag_start_tag: None,
        }
    }

    /// Enable mouse capture, logging `reason` to explain why.
    /// Does nothing (and logs nothing) if mouse capture is already enabled.
    pub fn enable(&mut self) {
        if self.enabled {
            return;
        }
        match crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture) {
            Ok(_) => {
                log::trace!("Mouse capture enabled");
                self.enabled = true;
            }
            Err(e) => {
                log::error!("Failed to enable mouse capture: {}", e);
            }
        }
    }

    /// Disable mouse capture, logging `reason` to explain why.
    /// Does nothing (and logs nothing) if mouse capture is already disabled.
    pub fn disable(&mut self) {
        if !self.enabled {
            return;
        }
        self.left_button_down = false;
        match crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture) {
            Ok(_) => {
                log::trace!("Mouse capture disabled");
                self.enabled = false;
            }
            Err(e) => {
                log::error!("Failed to disable mouse capture: {}", e);
            }
        }
    }

    pub fn toggle(&mut self) {
        if self.enabled {
            self.disable();
        } else {
            self.enable();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_disabled(&self) -> bool {
        !self.enabled
    }

    pub fn record_left_click_down(&mut self, byte_pos: usize) -> ClickCount {
        let now = std::time::Instant::now();
        if let Some(last_pos) = self.last_left_click_buffer_pos
            && last_pos != byte_pos
        {
            // If the click position has changed, reset the click count.
            self.last_left_click_times.clear();
        }
        self.last_left_click_buffer_pos = Some(byte_pos);

        self.last_left_click_times.push(now);
        const CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(500);
        self.last_left_click_times
            .retain(|&t| now.duration_since(t) <= CLICK_WINDOW);
        self.get_click_count()
    }

    pub fn get_click_count(&self) -> ClickCount {
        match self.last_left_click_times.len() {
            0 => ClickCount::None,
            1 => ClickCount::Single,
            2 => ClickCount::Double,
            _ => ClickCount::Triple,
        }
    }

    pub fn get_last_click_buffer_pos(&self) -> Option<usize> {
        self.last_left_click_buffer_pos
    }

    /// Mark the left mouse button as currently held down.
    pub fn set_left_button_down(&mut self) {
        self.left_button_down = true;
    }

    /// Mark the left mouse button as released.
    pub fn set_left_button_up(&mut self) {
        self.left_button_down = false;
    }

    /// Whether the left mouse button is currently being held down.
    pub fn is_left_button_down(&self) -> bool {
        self.left_button_down
    }
}
