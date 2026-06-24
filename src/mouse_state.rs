use crate::content_builder::Tag;
use crate::settings::MouseMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickCount {
    None,
    Single,
    Double,
    Triple,
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

impl crossterm::Command for PointerShape {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            PointerShape::Default => write!(f, "\x1b]22;\x1b\\"),
            _ => write!(f, "\x1b]22;{}\x1b\\", self.to_str()),
        }
    }
}

pub struct MouseState {
    enabled: bool,
    last_left_click_times: Vec<std::time::Instant>,
    last_left_click_buffer_pos: Option<usize>,
    /// True while the left mouse button is currently being held down.
    /// Set on `MouseEventKind::Down(Left)` and cleared on `MouseEventKind::Up(Left)`.
    left_button_down: bool,
    /// `DrawnContent::get_tagged_cell` sometimes returns a different tag than the actual direct cell under mouse.
    /// This improves UX.
    pub last_mouse_over_cell_semantic: Option<Tag>,
    pub last_mouse_over_cell_direct: Option<Tag>,
    pub drag_start_tag: Option<Tag>,
    current_pointer_shape: PointerShape,
    /// The coordinates where the right mouse button was last pressed down.
    pub right_click_down_pos: Option<(u16, u16)>,
}

impl MouseState {
    /// Initialize mouse state for the given mode, immediately enabling mouse capture
    /// (via crossterm) when appropriate.
    pub fn initialize(mode: &MouseMode) -> Self {
        let enabled = match mode {
            MouseMode::Disabled => false,
            MouseMode::Simple | MouseMode::Smart => {
                match crossterm::execute!(
                    std::io::stdout(),
                    crossterm::event::EnableMouseCapture,
                    XtShiftEscape::Enable
                ) {
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
            last_mouse_over_cell_semantic: None,
            last_mouse_over_cell_direct: None,
            drag_start_tag: None,
            current_pointer_shape: PointerShape::Default,
            right_click_down_pos: None,
        }
    }

    /// Enable mouse capture, logging `reason` to explain why.
    /// Does nothing (and logs nothing) if mouse capture is already enabled.
    pub fn enable(&mut self) {
        if self.enabled {
            return;
        }
        match crossterm::execute!(
            std::io::stdout(),
            crossterm::event::EnableMouseCapture,
            XtShiftEscape::Enable
        ) {
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
        // Reset pointer shape before actually disabling, so the code is written
        self.set_pointer_shape(PointerShape::Default, false);
        match crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            XtShiftEscape::Disable
        ) {
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

    /// Set the coordinates where the right click was depressed.
    pub fn set_right_click_down_pos(&mut self, row: u16, col: u16) {
        self.right_click_down_pos = Some((row, col));
    }

    /// Retrieve and clear the coordinates where the right click was depressed.
    pub fn take_right_click_down_pos(&mut self) -> Option<(u16, u16)> {
        self.right_click_down_pos.take()
    }

    fn set_pointer_shape(&mut self, shape: PointerShape, force: bool) {
        if !self.enabled {
            return;
        }
        if !force && self.current_pointer_shape == shape {
            return;
        }
        self.current_pointer_shape = shape;

        log::trace!("pointer shape set: {:?}", shape);

        let _ = crossterm::execute!(std::io::stdout(), shape);
    }

    pub fn update_pointer_shape(
        &mut self,
        _is_text_selected: bool,
        change_shape: bool,
        force: bool,
    ) {
        let is_dragging = self.left_button_down;
        let hovered_tag = self.last_mouse_over_cell_direct;
        let drag_start = self.drag_start_tag;

        let shape = if !change_shape {
            PointerShape::Default
        } else if is_dragging {
            if matches!(drag_start, Some(Tag::Command(_))) {
                PointerShape::Text
            } else {
                PointerShape::Grabbing
            }
        } else if matches!(hovered_tag, Some(Tag::Command(_))) {
            PointerShape::Text
        } else if hovered_tag.is_some_and(|tag| {
            matches!(
                tag,
                Tag::Suggestion(_)
                    | Tag::HistoryResult(_)
                    | Tag::AiResult(_)
                    | Tag::TutorialPrev
                    | Tag::TutorialNext
                    | Tag::PromptCopyBufferWidget
                    | Tag::Clipboard(_)
                    | Tag::Ps1PromptCwdWidget(_)
                    | Tag::TabCompletionScrollBar { .. }
                    | Tag::FlycompSandboxInfo
                    | Tag::FlycompInfo
                    | Tag::RightClickCopy
                    | Tag::RightClickCut
                    | Tag::RightClickPaste
                    | Tag::RightClickUndo
                    | Tag::RightClickRedo
                    | Tag::RightClickRunTutorial
            )
        }) {
            PointerShape::Pointer
        } else {
            PointerShape::Default
        };

        self.set_pointer_shape(shape, force);
    }
}

impl Drop for MouseState {
    fn drop(&mut self) {
        if self.enabled {
            let _ = crossterm::execute!(
                std::io::stdout(),
                PointerShape::Default,
                XtShiftEscape::Disable
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XtShiftEscape {
    Enable,
    Disable,
}

impl crossterm::Command for XtShiftEscape {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            XtShiftEscape::Enable => write!(f, "\x1b[>1s"),
            XtShiftEscape::Disable => write!(f, "\x1b[>0s"),
        }
    }
}
