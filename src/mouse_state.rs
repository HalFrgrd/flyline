pub struct MouseState {
    enabled: bool,
}

impl MouseState {
    pub fn new() -> Self {
        MouseState { enabled: true }
    }

    fn enable(&mut self) {
        match crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture) {
            Ok(_) => {
                log::debug!("Enabled mouse capture");
                self.enabled = true;
            }
            Err(e) => {
                log::error!("Failed to enable mouse capture: {}", e);
            }
        }
    }
    fn disable(&mut self) {
        match crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture) {
            Ok(_) => {
                log::debug!("Disabled mouse capture");
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

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
