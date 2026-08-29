use crate::content::Tag;
use crate::settings::MouseMode;

use std::sync::Mutex;

pub static GLOBAL_MOUSE_STATE: Mutex<Option<MouseState>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickCount {
    None,
    Single,
    Double,
    Triple,
    Quad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerShape {
    Default,
    Text,
    Pointer,
    Grabbing,
}

impl PointerShape {
    fn to_str(self) -> &'static str {
        match self {
            PointerShape::Default => "default",
            PointerShape::Text => "text",
            PointerShape::Pointer => "pointer",
            PointerShape::Grabbing => "grabbing",
        }
    }
}

impl std::fmt::Display for PointerShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\x1b]22;{}\x1b\\", self.to_str())
    }
}

#[derive(Clone, Debug)]
pub struct MouseState {
    enabled: bool,
    last_left_click_times: Vec<std::time::Instant>,
    last_left_click_buffer_pos: Option<usize>,
    /// True while the left mouse button is currently being held down.
    /// Set on `MouseEventKind::Down(Left)` and cleared on `MouseEventKind::Up(Left)`.
    left_button_down: bool,
    left_button_dragging: bool,
    /// `DrawnContent::get_tagged_cell` sometimes returns a different tag than the actual direct cell under mouse.
    /// This improves UX.
    pub last_mouse_over_cell_semantic: Option<Tag>,
    pub last_mouse_over_cell_direct: Option<Tag>,
    pub drag_start_tag: Option<Tag>,
    current_pointer_shape: PointerShape,
    /// The coordinates where the right mouse button was last pressed down.
    pub right_click_down_pos: Option<(u16, u16)>,
    pub last_mouse_pos: Option<(u16, u16)>,
    last_scroll_time: Option<std::time::Instant>,
    pub last_mouse_event_time: Option<std::time::Instant>,
}

/// Access or mutate the global `MouseState` instance.
pub fn mouse_state<R>(f: impl FnOnce(&mut MouseState) -> R) -> R {
    let mut lock = GLOBAL_MOUSE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = lock.get_or_insert_with(MouseState::default);
    f(state)
}

impl Default for MouseState {
    fn default() -> Self {
        MouseState {
            enabled: false,
            last_left_click_times: Vec::new(),
            last_left_click_buffer_pos: None,
            left_button_down: false,
            left_button_dragging: false,
            last_mouse_over_cell_semantic: None,
            last_mouse_over_cell_direct: None,
            drag_start_tag: None,
            current_pointer_shape: PointerShape::Default,
            right_click_down_pos: None,
            last_mouse_pos: None,
            last_scroll_time: None,
            last_mouse_event_time: None,
        }
    }
}

impl MouseState {
    /// Enable mouse capture for the given mode (or default Smart mode).
    pub fn enable_mode(mode: &MouseMode) {
        mouse_state(|m| m.enable_with_mode(mode));
    }

    pub fn enable(&mut self) {
        self.enable_with_mode(&MouseMode::Smart);
    }

    pub fn enable_with_mode(&mut self, mode: &MouseMode) {
        use termina::escape::csi::{Csi, DecPrivateMode, DecPrivateModeCode, Mode};
        let set_mode = |code| Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(code)));

        match mode {
            MouseMode::Disabled => {
                if self.enabled {
                    self.disable();
                }
            }
            MouseMode::Simple | MouseMode::Smart => {
                if self.enabled {
                    return;
                }
                match crate::flush_stdout!(
                    "{}{}{}{}{}",
                    set_mode(DecPrivateModeCode::MouseTracking),
                    set_mode(DecPrivateModeCode::ButtonEventMouse),
                    set_mode(DecPrivateModeCode::AnyEventMouse),
                    set_mode(DecPrivateModeCode::SGRMouse),
                    XtShiftEscape::Enable
                ) {
                    Ok(_) => {
                        log::trace!("Mouse capture enabled for {:?} mode", mode);
                        self.enabled = true;
                    }
                    Err(e) => {
                        log::error!("Failed to enable mouse capture: {}", e);
                        self.enabled = false;
                    }
                }
            }
        }
    }

    pub fn disable(&mut self) {
        if !self.enabled {
            return;
        }
        use termina::escape::csi::{Csi, DecPrivateMode, DecPrivateModeCode, Mode};
        let reset_mode = |code| Csi::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(code)));
        let _ = crate::flush_stdout!(
            "{}{}{}{}{}{}",
            PointerShape::Default,
            reset_mode(DecPrivateModeCode::AnyEventMouse),
            reset_mode(DecPrivateModeCode::ButtonEventMouse),
            reset_mode(DecPrivateModeCode::MouseTracking),
            reset_mode(DecPrivateModeCode::SGRMouse),
            XtShiftEscape::Disable
        );
        self.enabled = false;
        self.left_button_down = false;
        self.current_pointer_shape = PointerShape::Default;
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
            3 => ClickCount::Triple,
            _ => ClickCount::Quad,
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

    /// Mark the left mouse button as dragging or not.
    pub fn set_left_button_dragging(&mut self, dragging: bool) {
        self.left_button_dragging = dragging;
    }

    /// Whether the left mouse button is currently dragging.
    pub fn is_left_button_dragging(&self) -> bool {
        self.left_button_dragging
    }

    /// Set the coordinates where the right click was depressed.
    pub fn set_right_click_down_pos(&mut self, row: u16, col: u16) {
        self.right_click_down_pos = Some((row, col));
    }

    /// Retrieve and clear the coordinates where the right click was depressed.
    pub fn take_right_click_down_pos(&mut self) -> Option<(u16, u16)> {
        self.right_click_down_pos.take()
    }

    /// Record a mouse scroll event timestamp.
    pub fn record_scroll(&mut self) {
        self.last_scroll_time = Some(std::time::Instant::now());
    }

    /// Returns true if a mouse scroll event occurred within the last 50ms.
    pub fn is_mouse_scrolling(&self) -> bool {
        self.last_scroll_time
            .is_some_and(|t| t.elapsed() <= std::time::Duration::from_millis(50))
    }

    /// Record that a mouse event occurred at the current time.
    pub fn record_mouse_event_time(&mut self) {
        self.last_mouse_event_time = Some(std::time::Instant::now());
    }

    /// Returns true if a mouse event occurred within the specified time window.
    pub fn has_recent_mouse_activity(&self, window: std::time::Duration) -> bool {
        self.last_mouse_event_time
            .is_some_and(|t| t.elapsed() <= window)
    }

    pub(crate) fn set_pointer_shape(&mut self, shape: PointerShape) {
        if !self.enabled {
            return;
        }
        if self.current_pointer_shape == shape {
            return;
        }
        self.current_pointer_shape = shape;

        log::info!("pointer shape set: {:?}", shape);

        let _ = crate::flush_stdout!("{}", shape);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XtShiftEscape {
    Enable,
    Disable,
}

impl std::fmt::Display for XtShiftEscape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XtShiftEscape::Enable => write!(f, "\x1b[>1s"),
            XtShiftEscape::Disable => write!(f, "\x1b[>0s"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_has_recent_mouse_activity() {
        let mut state = MouseState::default();
        assert!(!state.has_recent_mouse_activity(Duration::from_millis(100)));

        state.record_mouse_event_time();
        assert!(state.has_recent_mouse_activity(Duration::from_millis(100)));

        // An event from 200ms ago should not be considered recent for a 100ms window
        state.last_mouse_event_time = Some(Instant::now() - Duration::from_millis(200));
        assert!(!state.has_recent_mouse_activity(Duration::from_millis(100)));
    }
}
