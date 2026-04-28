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
    /// True when the user has explicitly disabled mouse capture via a toggle action.
    /// Smart mode will not automatically re-enable while this flag is set.
    explicitly_disabled_by_user: bool,
    last_left_click_times: Vec<std::time::Instant>,
    last_left_click_buffer_pos: Option<usize>,
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
            explicitly_disabled_by_user: false,
            last_left_click_times: Vec::new(),
            last_left_click_buffer_pos: None,
        }
    }

    /// Enable mouse capture, logging `reason` to explain why.
    /// Does nothing (and logs nothing) if mouse capture is already enabled.
    pub fn enable(&mut self, reason: &str) {
        if self.enabled {
            return;
        }
        match crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture) {
            Ok(_) => {
                log::trace!("Mouse capture enabled: {}", reason);
                self.enabled = true;
            }
            Err(e) => {
                log::error!("Failed to enable mouse capture: {}", e);
            }
        }
    }

    /// Disable mouse capture, logging `reason` to explain why.
    /// Does nothing (and logs nothing) if mouse capture is already disabled.
    pub fn disable(&mut self, reason: &str) {
        if !self.enabled {
            return;
        }
        match crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture) {
            Ok(_) => {
                log::trace!("Mouse capture disabled: {}", reason);
                self.enabled = false;
            }
            Err(e) => {
                log::error!("Failed to disable mouse capture: {}", e);
            }
        }
    }

    /// Toggle mouse capture, logging `reason` to explain why.
    /// Tracks whether the user has explicitly disabled capture so that Smart mode
    /// automatic re-enable logic can respect user intent.
    pub fn toggle(&mut self, reason: &str) {
        if self.enabled {
            self.disable(reason);
            self.explicitly_disabled_by_user = true;
        } else {
            self.enable(reason);
            self.explicitly_disabled_by_user = false;
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Whether the user has explicitly disabled mouse capture via a toggle action.
    /// When true, Smart mode will not automatically re-enable mouse capture.
    pub fn is_explicitly_disabled_by_user(&self) -> bool {
        self.explicitly_disabled_by_user
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
}
