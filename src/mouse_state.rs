use crate::settings::MouseMode;
use std::time::{Duration, Instant};

pub struct MouseState {
    enabled: bool,
    /// When set (Smart mode only), re-enable mouse capture at this instant.
    reenable_after: Option<Instant>,
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
            reenable_after: None,
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
    pub fn toggle(&mut self, reason: &str) {
        if self.enabled {
            self.disable(reason);
        } else {
            self.enable(reason);
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Schedule a re-enable after 500 ms (used by Smart mode).
    pub fn schedule_reenable(&mut self) {
        self.reenable_after = Some(Instant::now() + Duration::from_millis(500));
    }

    /// Cancel any pending re-enable timer.
    pub fn cancel_reenable(&mut self) {
        self.reenable_after = None;
    }

    /// If a re-enable is scheduled and the deadline has passed, enable capture.
    /// Returns true if the timer was consumed (either fired or cleared).
    pub fn check_reenable_timer(&mut self) -> bool {
        if let Some(at) = self.reenable_after {
            if Instant::now() >= at {
                self.enable("smart mode: 500 ms periodic re-enable");
                self.reenable_after = None;
                return true;
            }
        }
        false
    }
}
