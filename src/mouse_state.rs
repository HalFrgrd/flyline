use crate::settings::MouseMode;

pub struct MouseState {
    enabled: bool,
    /// True when the user has explicitly disabled mouse capture via a toggle action.
    /// Smart mode will not automatically re-enable while this flag is set.
    explicitly_disabled_by_user: bool,
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
                        log::info!("Mouse capture enabled: initial setup for {:?} mode", mode);
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
                log::info!("Mouse capture enabled: {}", reason);
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
                log::info!("Mouse capture disabled: {}", reason);
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
}
