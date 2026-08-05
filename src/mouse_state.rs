use crate::content_builder::Tag;
use crate::settings::MouseMode;

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
    fn to_str(&self) -> &'static str {
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

use termina::event::{Modifiers as KeyModifiers, MouseEvent, MouseEventKind};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlylineMouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub column_as_f32: f32,
    pub row_as_f32: f32,
    pub x_pixel: Option<u32>,
    pub y_pixel: Option<u32>,
    pub modifiers: KeyModifiers,
}

impl FlylineMouseEvent {
    pub fn from_termina_mouse(
        mouse: MouseEvent,
        sgr1016_enabled: bool,
        cell_width_px: Option<f32>,
        cell_height_px: Option<f32>,
    ) -> Self {
        if sgr1016_enabled
            && let (Some(w), Some(h)) = (cell_width_px, cell_height_px)
            && w > 0.0
            && h > 0.0
        {
            let x_pixel = mouse.column as u32;
            let y_pixel = mouse.row as u32;
            let col_f32 = x_pixel as f32 / w;
            let row_f32 = y_pixel as f32 / h;
            log::info!(
                "Cell pixel dimensions: width = {:.2}px, height = {:.2}px | Mouse pixel: ({}, {}) => col = {:.2}, row = {:.2}",
                w,
                h,
                x_pixel,
                y_pixel,
                col_f32,
                row_f32
            );
            FlylineMouseEvent {
                kind: mouse.kind,
                column: col_f32 as u16,
                row: row_f32 as u16,
                column_as_f32: col_f32,
                row_as_f32: row_f32,
                x_pixel: Some(x_pixel),
                y_pixel: Some(y_pixel),
                modifiers: mouse.modifiers,
            }
        } else {
            FlylineMouseEvent {
                kind: mouse.kind,
                column: mouse.column,
                row: mouse.row,
                column_as_f32: mouse.column as f32,
                row_as_f32: mouse.row as f32,
                x_pixel: None,
                y_pixel: None,
                modifiers: mouse.modifiers,
            }
        }
    }
}

pub struct MouseState {
    enabled: bool,
    pub sgr1016_enabled: bool,
    pub cell_width_px: Option<f32>,
    pub cell_height_px: Option<f32>,
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
}

impl MouseState {
    /// Initialize mouse state for the given mode, immediately enabling mouse capture
    /// when appropriate.
    pub fn initialize(mode: &MouseMode) -> Self {
        use termina::escape::csi::{Csi, DecPrivateMode, DecPrivateModeCode, Mode, Window};
        let set_mode = |code| Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(code)));
        let enabled = match mode {
            MouseMode::Disabled => false,
            MouseMode::Simple | MouseMode::Smart => {
                match crate::flush_stdout!(
                    "{}{}{}{}{}{}{}{}",
                    set_mode(DecPrivateModeCode::MouseTracking),
                    set_mode(DecPrivateModeCode::ButtonEventMouse),
                    set_mode(DecPrivateModeCode::AnyEventMouse),
                    set_mode(DecPrivateModeCode::SGRMouse),
                    set_mode(DecPrivateModeCode::SGRPixelsMouse),
                    Csi::Window(Box::new(Window::ReportTextAreaSizePixels)),
                    Csi::Window(Box::new(Window::ReportCellSizePixels)),
                    XtShiftEscape::Enable
                ) {
                    Ok(_) => {
                        log::trace!(
                            "Mouse capture enabled (with SGR 1016 pixel mode): initial setup for {:?} mode",
                            mode
                        );
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
            sgr1016_enabled: enabled,
            cell_width_px: None,
            cell_height_px: None,
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
        }
    }

    /// Enable mouse capture, logging `reason` to explain why.
    /// Does nothing (and logs nothing) if mouse capture is already enabled.
    pub fn enable(&mut self) {
        if self.enabled {
            return;
        }
        use termina::escape::csi::{Csi, DecPrivateMode, DecPrivateModeCode, Mode, Window};
        let set_mode = |code| Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(code)));
        match crate::flush_stdout!(
            "{}{}{}{}{}{}{}{}",
            set_mode(DecPrivateModeCode::MouseTracking),
            set_mode(DecPrivateModeCode::ButtonEventMouse),
            set_mode(DecPrivateModeCode::AnyEventMouse),
            set_mode(DecPrivateModeCode::SGRMouse),
            set_mode(DecPrivateModeCode::SGRPixelsMouse),
            Csi::Window(Box::new(Window::ReportTextAreaSizePixels)),
            Csi::Window(Box::new(Window::ReportCellSizePixels)),
            XtShiftEscape::Enable
        ) {
            Ok(_) => {
                log::trace!("Mouse capture enabled");
                self.enabled = true;
                self.sgr1016_enabled = true;
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
        // Reset pointer shape before actually disabling, so the code is written
        self.set_pointer_shape(PointerShape::Default, false);
        use termina::escape::csi::{Csi, DecPrivateMode, DecPrivateModeCode, Mode};
        let reset_mode = |code| Csi::Mode(Mode::ResetDecPrivateMode(DecPrivateMode::Code(code)));
        match crate::flush_stdout!(
            "{}{}{}{}{}{}",
            reset_mode(DecPrivateModeCode::SGRPixelsMouse),
            reset_mode(DecPrivateModeCode::SGRMouse),
            reset_mode(DecPrivateModeCode::AnyEventMouse),
            reset_mode(DecPrivateModeCode::ButtonEventMouse),
            reset_mode(DecPrivateModeCode::MouseTracking),
            XtShiftEscape::Disable
        ) {
            Ok(_) => {
                log::trace!("Mouse capture disabled");
                self.enabled = false;
                self.sgr1016_enabled = false;
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

    pub(crate) fn set_pointer_shape(&mut self, shape: PointerShape, force: bool) {
        if !self.enabled {
            return;
        }
        if !force && self.current_pointer_shape == shape {
            return;
        }
        self.current_pointer_shape = shape;

        log::trace!("pointer shape set: {:?}", shape);

        use std::io::Write;
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{}", shape).and_then(|_| stdout.flush());
    }
}

impl Drop for MouseState {
    fn drop(&mut self) {
        if self.enabled {
            use std::io::Write;
            let mut stdout = std::io::stdout();
            let _ = write!(
                stdout,
                "{}{}",
                PointerShape::Default,
                XtShiftEscape::Disable
            )
            .and_then(|_| stdout.flush());
        }
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

    #[test]
    fn test_flyline_mouse_event_from_termina() {
        let mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 15,
            row: 4,
            modifiers: KeyModifiers::NONE,
        };
        let event = FlylineMouseEvent::from_termina_mouse(mouse, false, None, None);
        assert_eq!(event.column, 15);
        assert_eq!(event.row, 4);
        assert_eq!(event.column_as_f32, 15.0);
        assert_eq!(event.row_as_f32, 4.0);
        assert_eq!(event.x_pixel, None);
        assert_eq!(event.y_pixel, None);

        // With SGR 1016 enabled, column/row are pixels
        let pixel_mouse = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 150,
            row: 80,
            modifiers: KeyModifiers::NONE,
        };
        let event_px =
            FlylineMouseEvent::from_termina_mouse(pixel_mouse, true, Some(10.0), Some(20.0));
        assert_eq!(event_px.column, 15);
        assert_eq!(event_px.row, 4);
        assert_eq!(event_px.column_as_f32, 15.0);
        assert_eq!(event_px.row_as_f32, 4.0);
        assert_eq!(event_px.x_pixel, Some(150));
        assert_eq!(event_px.y_pixel, Some(80));
    }
}
