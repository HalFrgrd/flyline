use crate::app::actions::{ContextExpr, ContextLiteral, KeyEventAction};
use crate::app::{App, AppRunningState, ContentMode, ExitState, FlycompPromptSelection};
use crate::content_builder::Tag;
use crate::mouse_state::{ClickCount, PointerShape};
use crate::settings::MouseMode;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedrawUrgency {
    Now,
    Soon,
}

#[derive(Debug, Clone)]
pub struct MouseActionOutput {
    pub possible_buffer_change: bool,
    pub desired_pointer_shape: Option<crate::mouse_state::PointerShape>,
    pub redraw_urgency: RedrawUrgency,
}

impl Default for MouseActionOutput {
    fn default() -> Self {
        Self {
            possible_buffer_change: false,
            desired_pointer_shape: None,
            redraw_urgency: RedrawUrgency::Now,
        }
    }
}

impl MouseActionOutput {
    pub fn new(possible_buffer_change: bool, redraw_urgency: RedrawUrgency) -> Self {
        Self {
            possible_buffer_change,
            desired_pointer_shape: None,
            redraw_urgency,
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.possible_buffer_change |= other.possible_buffer_change;
        if other.desired_pointer_shape.is_some() {
            self.desired_pointer_shape = other.desired_pointer_shape;
        }
        if other.redraw_urgency == RedrawUrgency::Now {
            self.redraw_urgency = RedrawUrgency::Now;
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagPattern {
    Command,
    Suggestion,
    HistoryResult,
    AiResult,
    TutorialPrev,
    TutorialNext,
    Clipboard,
    PromptCopyBuffer,
    Ps1PromptCwd,
    FlycompYes,
    FlycompNo,
    FlycompDontAsk,
    RightClickCopy,
    RightClickCut,
    RightClickPaste,
    RightClickUndo,
    RightClickRedo,
    RightClickRunTutorial,
    RightClickMenu,
    Any,
    None,
}

impl TagPattern {
    pub fn matches(&self, tag: Option<Tag>) -> bool {
        match (self, tag) {
            (TagPattern::Any, _) => true,
            (TagPattern::None, None) => true,
            (TagPattern::Command, Some(Tag::Command(_))) => true,
            (TagPattern::Suggestion, Some(Tag::Suggestion(_)))
            | (TagPattern::Suggestion, Some(Tag::TabSuggestion)) => true,
            (TagPattern::HistoryResult, Some(Tag::HistoryResult(_))) => true,
            (TagPattern::AiResult, Some(Tag::AiResult(_))) => true,
            (TagPattern::TutorialPrev, Some(Tag::TutorialPrev)) => true,
            (TagPattern::TutorialNext, Some(Tag::TutorialNext)) => true,
            (TagPattern::Clipboard, Some(Tag::Clipboard(_))) => true,
            (TagPattern::PromptCopyBuffer, Some(Tag::PromptCopyBufferWidget)) => true,
            (TagPattern::Ps1PromptCwd, Some(Tag::Ps1PromptCwdWidget(_))) => true,
            (TagPattern::FlycompYes, Some(Tag::FlycompYes)) => true,
            (TagPattern::FlycompNo, Some(Tag::FlycompNo)) => true,
            (TagPattern::FlycompDontAsk, Some(Tag::FlycompDontAsk)) => true,
            (TagPattern::RightClickCopy, Some(Tag::RightClickCopy)) => true,
            (TagPattern::RightClickCut, Some(Tag::RightClickCut)) => true,
            (TagPattern::RightClickPaste, Some(Tag::RightClickPaste)) => true,
            (TagPattern::RightClickUndo, Some(Tag::RightClickUndo)) => true,
            (TagPattern::RightClickRedo, Some(Tag::RightClickRedo)) => true,
            (TagPattern::RightClickRunTutorial, Some(Tag::RightClickRunTutorial)) => true,
            (TagPattern::RightClickMenu, Some(Tag::RightClickCopy))
            | (TagPattern::RightClickMenu, Some(Tag::RightClickCut))
            | (TagPattern::RightClickMenu, Some(Tag::RightClickPaste))
            | (TagPattern::RightClickMenu, Some(Tag::RightClickUndo))
            | (TagPattern::RightClickMenu, Some(Tag::RightClickRedo))
            | (TagPattern::RightClickMenu, Some(Tag::RightClickRunTutorial))
            | (TagPattern::RightClickMenu, Some(Tag::RightClickMenu)) => true,
            _ => false,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseContextVar {
    Always,
    TabCompletion,
    FuzzyHistorySearch,
    AgentOutputSelection,
    PromptDirSelection,
    TabCompletionAskForFlycomp,

    LeftButtonClickedDown,
    LeftButtonClickedUp,
    LeftButtonIsDown,
    LeftButtonIsUp,
    RightButtonClickedDown,
    RightButtonClickedUp,
    DragLeft,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    Moved,
    OverCellSemantically(TagPattern),
    NotOverCellSemantically(TagPattern),
    OverCellDirectly(TagPattern),
    SmartModeClickAboveViewport,
    SmartModeScroll,
    IsOverSuggestions,
    IsOverFuzzyHistory,
    ScrollBarDrag,
    RightClickPopupActive,
    RightReleaseDismiss,
    SingleClick,
    DoubleClick,
    TripleClick,
    PointerShapeEnabled,
    DragStartCommand,
    IsPointerTarget,
}

impl super::ContextVar for MouseContextVar {
    fn evaluate(&self, app: &App) -> bool {
        let last_mouse = app.last_mouse.as_ref().map(|(m, _)| m);
        let clicked_tag = app.mouse_state.last_mouse_over_cell_semantic;
        let direct_tag = app.mouse_state.last_mouse_over_cell_direct;

        match self {
            MouseContextVar::Always => true,
            MouseContextVar::TabCompletion => {
                matches!(app.content_mode, ContentMode::TabCompletion { .. })
            }
            MouseContextVar::FuzzyHistorySearch => {
                matches!(app.content_mode, ContentMode::FuzzyHistorySearch(_))
            }
            MouseContextVar::AgentOutputSelection => {
                matches!(app.content_mode, ContentMode::AgentOutputSelection { .. })
            }
            MouseContextVar::PromptDirSelection => {
                matches!(app.content_mode, ContentMode::PromptDirSelect(_))
            }
            MouseContextVar::TabCompletionAskForFlycomp => {
                matches!(
                    app.content_mode,
                    ContentMode::TabCompletionAskForFlycomp { .. }
                )
            }

            MouseContextVar::LeftButtonClickedDown => last_mouse
                .is_some_and(|m| matches!(m.kind, MouseEventKind::Down(MouseButton::Left))),
            MouseContextVar::LeftButtonClickedUp => {
                last_mouse.is_some_and(|m| matches!(m.kind, MouseEventKind::Up(MouseButton::Left)))
            }
            MouseContextVar::LeftButtonIsDown => app.mouse_state.is_left_button_down(),
            MouseContextVar::LeftButtonIsUp => !app.mouse_state.is_left_button_down(),
            MouseContextVar::RightButtonClickedDown => last_mouse
                .is_some_and(|m| matches!(m.kind, MouseEventKind::Down(MouseButton::Right))),
            MouseContextVar::RightButtonClickedUp => {
                last_mouse.is_some_and(|m| matches!(m.kind, MouseEventKind::Up(MouseButton::Right)))
            }
            MouseContextVar::DragLeft => last_mouse
                .is_some_and(|m| matches!(m.kind, MouseEventKind::Drag(MouseButton::Left))),
            MouseContextVar::ScrollUp => {
                last_mouse.is_some_and(|m| matches!(m.kind, MouseEventKind::ScrollUp))
            }
            MouseContextVar::ScrollDown => {
                last_mouse.is_some_and(|m| matches!(m.kind, MouseEventKind::ScrollDown))
            }
            MouseContextVar::ScrollLeft => {
                last_mouse.is_some_and(|m| matches!(m.kind, MouseEventKind::ScrollLeft))
            }
            MouseContextVar::ScrollRight => {
                last_mouse.is_some_and(|m| matches!(m.kind, MouseEventKind::ScrollRight))
            }
            MouseContextVar::Moved => {
                last_mouse.is_some_and(|m| matches!(m.kind, MouseEventKind::Moved))
            }
            MouseContextVar::OverCellSemantically(pattern) => pattern.matches(clicked_tag),
            MouseContextVar::NotOverCellSemantically(pattern) => !pattern.matches(clicked_tag),
            MouseContextVar::OverCellDirectly(pattern) => pattern.matches(direct_tag),
            MouseContextVar::SmartModeClickAboveViewport => {
                app.settings.mouse_mode == MouseMode::Smart
                    && last_mouse.is_some_and(|m| {
                        matches!(m.kind, MouseEventKind::Down(_))
                            && app
                                .last_contents
                                .as_ref()
                                .is_some_and(|c| m.row < c.viewport_start)
                    })
            }
            MouseContextVar::SmartModeScroll => {
                app.settings.mouse_mode == MouseMode::Smart
                    && last_mouse.is_some_and(|m| {
                        matches!(
                            m.kind,
                            MouseEventKind::ScrollUp
                                | MouseEventKind::ScrollDown
                                | MouseEventKind::ScrollLeft
                                | MouseEventKind::ScrollRight
                        )
                    })
            }
            MouseContextVar::IsOverSuggestions => matches!(
                clicked_tag,
                Some(Tag::Suggestion(_))
                    | Some(Tag::TabSuggestion)
                    | Some(Tag::TabCompletionScrollBar { .. })
            ),
            MouseContextVar::IsOverFuzzyHistory => matches!(
                clicked_tag,
                Some(Tag::HistoryResult(_)) | Some(Tag::FuzzySearch)
            ),
            MouseContextVar::ScrollBarDrag => {
                matches!(
                    app.mouse_state.drag_start_tag,
                    Some(Tag::TabCompletionScrollBar { .. })
                ) && (app.mouse_state.is_left_button_down()
                    || last_mouse
                        .is_some_and(|m| matches!(m.kind, MouseEventKind::Drag(MouseButton::Left))))
            }
            MouseContextVar::RightClickPopupActive => app.right_click_popup_pos.is_some(),
            MouseContextVar::RightReleaseDismiss => {
                last_mouse.is_some_and(|m| {
                    matches!(m.kind, MouseEventKind::Up(MouseButton::Right))
                        && app.mouse_state.right_click_down_pos.is_some_and(
                            |(start_row, start_col)| (m.row, m.column) != (start_row, start_col),
                        )
                })
            }
            MouseContextVar::SingleClick => app.mouse_state.get_click_count() == ClickCount::Single,
            MouseContextVar::DoubleClick => app.mouse_state.get_click_count() == ClickCount::Double,
            MouseContextVar::TripleClick => app.mouse_state.get_click_count() == ClickCount::Triple,
            MouseContextVar::PointerShapeEnabled => app.settings.mouse_mode != MouseMode::Disabled,
            MouseContextVar::DragStartCommand => {
                matches!(app.mouse_state.drag_start_tag, Some(Tag::Command(_)))
            }
            MouseContextVar::IsPointerTarget => {
                let hovered_tag = app.mouse_state.last_mouse_over_cell_direct;
                hovered_tag.is_some_and(|tag| {
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
                            | Tag::FlycompYes
                            | Tag::FlycompNo
                            | Tag::FlycompDontAsk
                    )
                })
            }
        }
    }

    fn display(&self) -> String {
        format!("{:?}", self)
    }
}

impl std::ops::Not for MouseContextVar {
    type Output = ContextLiteral<MouseContextVar>;

    fn not(self) -> Self::Output {
        ContextLiteral::new(self, true)
    }
}

impl<Rhs> std::ops::Add<Rhs> for MouseContextVar
where
    Rhs: Into<super::ContextExpr<MouseContextVar>>,
{
    type Output = super::ContextExpr<MouseContextVar>;

    fn add(self, rhs: Rhs) -> Self::Output {
        super::ContextExpr::from(self) + rhs
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventAction {
    CopySelection,
    CutSelection,
    PasteSelection,
    Undo,
    Redo,
    RunTutorial,
    ScrollSuggestionsUp,
    ScrollSuggestionsDown,
    ScrollSuggestionsLeft,
    ScrollSuggestionsRight,
    ScrollHistoryUp,
    ScrollHistoryDown,
    AcceptSuggestion,
    AcceptHistoryResult,
    AcceptAiResult,
    ClickCommand,
    SelectWord,
    SelectAll,
    DragCommand,
    DragWord,
    DragAll,
    ClickTutorialPrev,
    ClickTutorialNext,
    PromptDirAccept,
    PromptDirSelect,
    ClickClipboard,
    ClickPromptCopyBuffer,
    FlycompSelectYes,
    FlycompSelectNo,
    FlycompSelectDontAsk,
    HoverSuggestion,
    HoverHistoryResult,
    HoverAiResult,
    HoverCommand,
    HoverClearTooltip,
    PromptDirSelectDismiss,
    DisableMouseCapture,
    ScrollSuggestionsBar,
    RightClickMenuOpen,
    RightClickMenuDismiss,
    SetPointer(PointerShape),
}

pub struct MouseBinding {
    pub(crate) context: super::ContextExpr<MouseContextVar>,
    pub(crate) action: MouseEventAction,
}

pub static DEFAULT_MOUSE_BINDINGS: LazyLock<Vec<MouseBinding>> = LazyLock::new(|| {
    vec![
        // Smart mode viewport click or scroll -> Disable mouse capture
        MouseBinding {
            context: ContextExpr::from(MouseContextVar::SmartModeScroll),
            action: MouseEventAction::DisableMouseCapture,
        },
        MouseBinding {
            context: ContextExpr::from(MouseContextVar::SmartModeClickAboveViewport),
            action: MouseEventAction::DisableMouseCapture,
        },
        // Right click menu popup opening
        MouseBinding {
            context: MouseContextVar::RightButtonClickedDown
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            action: MouseEventAction::RightClickMenuOpen,
        },
        // Right click menu popup dismissal on release scroll/click outside
        MouseBinding {
            context: MouseContextVar::RightClickPopupActive + MouseContextVar::RightReleaseDismiss,
            action: MouseEventAction::RightClickMenuDismiss,
        },
        MouseBinding {
            context: MouseContextVar::RightClickPopupActive
                + MouseContextVar::LeftButtonClickedDown
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            action: MouseEventAction::RightClickMenuDismiss,
        },
        MouseBinding {
            context: MouseContextVar::RightClickPopupActive
                + MouseContextVar::ScrollUp
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            action: MouseEventAction::RightClickMenuDismiss,
        },
        MouseBinding {
            context: MouseContextVar::RightClickPopupActive
                + MouseContextVar::ScrollDown
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            action: MouseEventAction::RightClickMenuDismiss,
        },
        // Right click menu options (activated by Left Click Release / Up)
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickCopy),
            action: MouseEventAction::CopySelection,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickCut),
            action: MouseEventAction::CutSelection,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickPaste),
            action: MouseEventAction::PasteSelection,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickUndo),
            action: MouseEventAction::Undo,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickRedo),
            action: MouseEventAction::Redo,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickRunTutorial),
            action: MouseEventAction::RunTutorial,
        },
        // Scrolling in suggestions
        MouseBinding {
            context: MouseContextVar::TabCompletion
                + MouseContextVar::ScrollUp
                + MouseContextVar::IsOverSuggestions,
            action: MouseEventAction::ScrollSuggestionsUp,
        },
        MouseBinding {
            context: MouseContextVar::TabCompletion
                + MouseContextVar::ScrollDown
                + MouseContextVar::IsOverSuggestions,
            action: MouseEventAction::ScrollSuggestionsDown,
        },
        MouseBinding {
            context: MouseContextVar::TabCompletion
                + MouseContextVar::ScrollLeft
                + MouseContextVar::IsOverSuggestions,
            action: MouseEventAction::ScrollSuggestionsLeft,
        },
        MouseBinding {
            context: MouseContextVar::TabCompletion
                + MouseContextVar::ScrollRight
                + MouseContextVar::IsOverSuggestions,
            action: MouseEventAction::ScrollSuggestionsRight,
        },
        // Scrollbar Dragging
        MouseBinding {
            context: MouseContextVar::TabCompletion + MouseContextVar::ScrollBarDrag,
            action: MouseEventAction::ScrollSuggestionsBar,
        },
        // Scrolling in history
        MouseBinding {
            context: MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::ScrollUp
                + MouseContextVar::IsOverFuzzyHistory,
            action: MouseEventAction::ScrollHistoryUp,
        },
        MouseBinding {
            context: MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::ScrollDown
                + MouseContextVar::IsOverFuzzyHistory,
            action: MouseEventAction::ScrollHistoryDown,
        },
        // Directory selection hover protection (prevents dismissal when hovering select widgets)
        MouseBinding {
            context: MouseContextVar::PromptDirSelection
                + MouseContextVar::Moved
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            action: MouseEventAction::HoverClearTooltip,
        },
        MouseBinding {
            context: MouseContextVar::PromptDirSelection
                + MouseContextVar::Moved
                + MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            action: MouseEventAction::HoverClearTooltip,
        },
        MouseBinding {
            context: MouseContextVar::PromptDirSelection
                + MouseContextVar::Moved
                + !MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd)
                + !MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            action: MouseEventAction::PromptDirSelectDismiss,
        },
        // Hovering selection updates
        MouseBinding {
            context: MouseContextVar::TabCompletion
                + MouseContextVar::Moved
                + MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            action: MouseEventAction::HoverSuggestion,
        },
        MouseBinding {
            context: MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::Moved
                + MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            action: MouseEventAction::HoverHistoryResult,
        },
        MouseBinding {
            context: MouseContextVar::AgentOutputSelection
                + MouseContextVar::Moved
                + MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            action: MouseEventAction::HoverAiResult,
        },
        MouseBinding {
            context: MouseContextVar::Moved
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::HoverCommand,
        },
        MouseBinding {
            context: MouseContextVar::Moved
                + !MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::HoverClearTooltip,
        },
        // Selecting/Accepting options
        MouseBinding {
            context: MouseContextVar::TabCompletion
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            action: MouseEventAction::AcceptSuggestion,
        },
        MouseBinding {
            context: MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            action: MouseEventAction::AcceptHistoryResult,
        },
        MouseBinding {
            context: MouseContextVar::AgentOutputSelection
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            action: MouseEventAction::AcceptAiResult,
        },
        // Command clicking (single, double, triple clicks)
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::SingleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::ClickCommand,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::DoubleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::SelectWord,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::TripleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::SelectAll,
        },
        // Command dragging
        MouseBinding {
            context: MouseContextVar::DragLeft
                + MouseContextVar::SingleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::DragCommand,
        },
        MouseBinding {
            context: MouseContextVar::DragLeft
                + MouseContextVar::DoubleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::DragWord,
        },
        MouseBinding {
            context: MouseContextVar::DragLeft
                + MouseContextVar::TripleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            action: MouseEventAction::DragAll,
        },
        // Tutorial
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::TutorialPrev),
            action: MouseEventAction::ClickTutorialPrev,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::TutorialNext),
            action: MouseEventAction::ClickTutorialNext,
        },
        // Ps1 Cwd Click / Accept
        MouseBinding {
            context: MouseContextVar::PromptDirSelection
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            action: MouseEventAction::PromptDirAccept,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            action: MouseEventAction::PromptDirSelect,
        },
        MouseBinding {
            context: MouseContextVar::DragLeft
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            action: MouseEventAction::PromptDirSelect,
        },
        // Clipboard
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::Clipboard),
            action: MouseEventAction::ClickClipboard,
        },
        MouseBinding {
            context: MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            action: MouseEventAction::ClickPromptCopyBuffer,
        },
        // Flycomp ask prompt
        MouseBinding {
            context: MouseContextVar::TabCompletionAskForFlycomp
                + MouseContextVar::OverCellSemantically(TagPattern::FlycompYes),
            action: MouseEventAction::FlycompSelectYes,
        },
        MouseBinding {
            context: MouseContextVar::TabCompletionAskForFlycomp
                + MouseContextVar::OverCellSemantically(TagPattern::FlycompNo),
            action: MouseEventAction::FlycompSelectNo,
        },
        MouseBinding {
            context: MouseContextVar::TabCompletionAskForFlycomp
                + MouseContextVar::OverCellSemantically(TagPattern::FlycompDontAsk),
            action: MouseEventAction::FlycompSelectDontAsk,
        },
        // Pointer shape updating at the end of the matching sequence
        MouseBinding {
            context: ContextExpr::from(!MouseContextVar::PointerShapeEnabled),
            action: MouseEventAction::SetPointer(PointerShape::Default),
        },
        MouseBinding {
            context: MouseContextVar::PointerShapeEnabled
                + MouseContextVar::LeftButtonIsDown
                + !MouseContextVar::DragStartCommand,
            action: MouseEventAction::SetPointer(PointerShape::Grabbing),
        },
        MouseBinding {
            context: MouseContextVar::PointerShapeEnabled
                + !MouseContextVar::LeftButtonIsDown
                + MouseContextVar::OverCellDirectly(TagPattern::Command),
            action: MouseEventAction::SetPointer(PointerShape::Text),
        },
        MouseBinding {
            context: MouseContextVar::PointerShapeEnabled
                + MouseContextVar::LeftButtonIsDown
                + MouseContextVar::DragStartCommand,
            action: MouseEventAction::SetPointer(PointerShape::Text),
        },
        MouseBinding {
            context: MouseContextVar::PointerShapeEnabled
                + !MouseContextVar::LeftButtonIsDown
                + MouseContextVar::IsPointerTarget,
            action: MouseEventAction::SetPointer(PointerShape::Pointer),
        },
        MouseBinding {
            context: MouseContextVar::PointerShapeEnabled
                + !MouseContextVar::LeftButtonIsDown
                + !MouseContextVar::OverCellDirectly(TagPattern::Command)
                + !MouseContextVar::IsPointerTarget,
            action: MouseEventAction::SetPointer(PointerShape::Default),
        },
    ]
});

impl MouseEventAction {
    pub(crate) fn run(
        &self,
        app: &mut App,
        mouse: MouseEvent,
        clicked_tag: Option<Tag>,
        cursor_directly_on_cell: bool,
    ) -> MouseActionOutput {
        match self {
            MouseEventAction::CopySelection => {
                app.right_click_popup_pos = None;
                KeyEventAction::CopySelectionOsc52.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                MouseActionOutput::new(true, RedrawUrgency::Now)
            }
            MouseEventAction::CutSelection => {
                app.right_click_popup_pos = None;
                KeyEventAction::CutSelection.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                MouseActionOutput::new(true, RedrawUrgency::Now)
            }
            MouseEventAction::PasteSelection => {
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                KeyEventAction::PasteSystemClipboard.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                MouseActionOutput::new(true, RedrawUrgency::Now)
            }
            MouseEventAction::Undo => {
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                KeyEventAction::Undo.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                MouseActionOutput::new(true, RedrawUrgency::Now)
            }
            MouseEventAction::Redo => {
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                KeyEventAction::Redo.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                MouseActionOutput::new(true, RedrawUrgency::Now)
            }
            MouseEventAction::RunTutorial => {
                app.settings.run_tutorial = true;
                app.settings.tutorial_step = crate::tutorial::TutorialStep::Welcome;
                if let Err(e) = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                    crossterm::cursor::MoveTo(0, 0)
                ) {
                    log::warn!("Failed to clear terminal: {}", e);
                }
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                app.mode = AppRunningState::Exiting(ExitState::WithoutCommand);
                MouseActionOutput::new(true, RedrawUrgency::Now)
            }
            MouseEventAction::ScrollSuggestionsUp => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_up_arrow();
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::ScrollSuggestionsDown => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_down_arrow();
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::ScrollSuggestionsLeft => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_left_arrow();
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::ScrollSuggestionsRight => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_right_arrow();
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::ScrollSuggestionsBar => {
                let active_drag_tag = app.mouse_state.drag_start_tag;
                if let Some(Tag::TabCompletionScrollBar {
                    max_cell_height,
                    y_start,
                    ..
                }) = active_drag_tag
                {
                    if let Some(ref drawn) = app.last_contents {
                        let min_row = drawn.content_row_to_term_em_row(y_start);
                        let max_row = min_row + max_cell_height as u16;

                        let cell_height = if mouse.row < min_row {
                            0
                        } else if mouse.row > max_row {
                            max_cell_height
                        } else {
                            (mouse.row - min_row) as usize
                        };

                        if let ContentMode::TabCompletion(active_suggestions) =
                            &mut app.content_mode
                        {
                            active_suggestions
                                .set_selected_by_scrollbar_pos(cell_height, max_cell_height);
                        }
                    }
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::ScrollHistoryUp => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    let source = source.clone();
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Forward);
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::ScrollHistoryDown => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    let source = source.clone();
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Backward);
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::HoverSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        log::debug!("Setting selected by idx: {}", idx);
                        active_suggestions.set_selected_by_idx(idx);
                    }
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::HoverHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                        let source = source.clone();
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                    }
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::HoverAiResult => {
                if let Some(Tag::AiResult(idx)) = clicked_tag {
                    if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                        selection.set_selected_by_idx(idx);
                    }
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::HoverCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if let Some(part) = app.formatted_buffer_cache.get_part_from_byte_pos(byte_pos)
                        && let Some(tooltip) = part.tooltip.as_ref()
                    {
                        app.tooltip = Some(tooltip.clone());
                    }
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::HoverClearTooltip => {
                app.tooltip = None;
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::AcceptSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        active_suggestions.set_selected_by_idx(idx);
                        active_suggestions.accept_selected_filtered_item(&mut app.buffer);
                        app.content_mode = ContentMode::Normal;
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::AcceptHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                        let source = source.clone();
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                        app.accept_fuzzy_history_search();
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::AcceptAiResult => {
                if let Some(Tag::AiResult(idx)) = clicked_tag {
                    if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                        selection.set_selected_by_idx(idx);
                        if let Some(cmd) = selection.selected_command() {
                            let cmd = cmd.to_string();
                            app.buffer.replace_buffer(&cmd);
                            app.content_mode = ContentMode::Normal;
                            MouseActionOutput::new(true, RedrawUrgency::Now)
                        } else {
                            MouseActionOutput::new(false, RedrawUrgency::Now)
                        }
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::ClickCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let extend_selection = mouse.modifiers.contains(KeyModifiers::SHIFT);
                        if extend_selection {
                            app.buffer.start_selection_if_none();
                        } else {
                            app.buffer.clear_selection();
                        }
                        app.buffer
                            .try_move_cursor_to_byte_pos(byte_pos, !cursor_directly_on_cell);
                        if !extend_selection {
                            app.buffer.start_selection_if_none();
                        }
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::SelectAll => {
                if app.settings.select_with_mouse {
                    app.buffer.select_entire_buffer();
                    MouseActionOutput::new(true, RedrawUrgency::Now)
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::SelectWord => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        app.buffer
                            .try_move_cursor_to_byte_pos(byte_pos, !cursor_directly_on_cell);
                        app.buffer.select_word();
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::DragCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let active_drag_tag = app.mouse_state.drag_start_tag;
                        if matches!(active_drag_tag, Some(Tag::Command(_))) {
                            app.buffer.start_selection_if_none();
                            app.buffer
                                .try_move_cursor_to_byte_pos(byte_pos, !cursor_directly_on_cell);
                            MouseActionOutput::new(true, RedrawUrgency::Soon)
                        } else {
                            MouseActionOutput::new(false, RedrawUrgency::Soon)
                        }
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Soon)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Soon)
                }
            }
            MouseEventAction::DragWord => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let active_drag_tag = app.mouse_state.drag_start_tag;
                        if matches!(active_drag_tag, Some(Tag::Command(_))) {
                            if let Some(drag_start_pos) =
                                app.mouse_state.get_last_click_buffer_pos()
                            {
                                app.buffer.try_move_cursor_to_byte_pos(
                                    drag_start_pos,
                                    !cursor_directly_on_cell,
                                );
                                let anchor_word_sel_range = app.buffer.select_word();
                                app.buffer.try_move_cursor_to_byte_pos(
                                    byte_pos,
                                    !cursor_directly_on_cell,
                                );
                                let new_word_sel_range = app.buffer.select_word();
                                let new_sel_range =
                                    anchor_word_sel_range.start.min(new_word_sel_range.start)
                                        ..anchor_word_sel_range.end.max(new_word_sel_range.end);
                                let cursor_is_left = drag_start_pos > byte_pos;
                                app.buffer
                                    .set_selection_range(new_sel_range, cursor_is_left);
                            }
                            MouseActionOutput::new(true, RedrawUrgency::Soon)
                        } else {
                            MouseActionOutput::new(false, RedrawUrgency::Soon)
                        }
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Soon)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Soon)
                }
            }
            MouseEventAction::DragAll => {
                if app.settings.select_with_mouse {
                    app.buffer.select_entire_buffer();
                    MouseActionOutput::new(true, RedrawUrgency::Soon)
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Soon)
                }
            }
            MouseEventAction::ClickTutorialPrev => {
                app.settings.tutorial_step.prev();
                log::info!(
                    "Tutorial navigated to prev: {:?}",
                    app.settings.tutorial_step
                );
                MouseActionOutput::new(false, RedrawUrgency::Now)
            }
            MouseEventAction::ClickTutorialNext => {
                app.settings.tutorial_step.next();
                log::info!(
                    "Tutorial navigated to next: {:?}",
                    app.settings.tutorial_step
                );
                MouseActionOutput::new(false, RedrawUrgency::Now)
            }
            MouseEventAction::PromptDirAccept => {
                KeyEventAction::PromptDirAcceptEntry.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                MouseActionOutput::new(true, RedrawUrgency::Now)
            }
            MouseEventAction::PromptDirSelect => {
                if let Some(Tag::Ps1PromptCwdWidget(idx)) = clicked_tag {
                    app.content_mode = ContentMode::PromptDirSelect(idx);
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::PromptDirSelectDismiss => {
                if matches!(app.content_mode, ContentMode::PromptDirSelect(_)) {
                    app.content_mode = ContentMode::Normal;
                }
                MouseActionOutput::new(false, RedrawUrgency::Soon)
            }
            MouseEventAction::ClickClipboard => {
                if let Some(Tag::Clipboard(clipboard_type)) = clicked_tag {
                    if let Some(text) = app
                        .last_contents
                        .as_ref()
                        .and_then(|c| c.contents.clipboards.get(&clipboard_type))
                    {
                        let text = text.clone();
                        if app.copy_to_clipboard(text.as_bytes()) {
                            log::info!("Copied to clipboard via OSC 52 ({:?})", clipboard_type);
                        }
                        app.buffer.replace_buffer(&text);
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::ClickPromptCopyBuffer => {
                let text = app.buffer.buffer().to_string();
                if app.copy_to_clipboard(text.as_bytes()) {
                    log::info!("Copied current buffer to clipboard via copy-buffer widget");
                    MouseActionOutput::new(true, RedrawUrgency::Now)
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::FlycompSelectYes => {
                if let ContentMode::TabCompletionAskForFlycomp {
                    ref mut selection, ..
                } = app.content_mode
                {
                    *selection = FlycompPromptSelection::Yes;
                    if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
                        let mode = std::mem::replace(&mut app.content_mode, ContentMode::Normal);
                        if let ContentMode::TabCompletionAskForFlycomp {
                            command_word,
                            word_under_cursor,
                            sandbox,
                            ..
                        } = mode
                        {
                            app.run_flycomp(command_word, word_under_cursor, sandbox.is_some());
                        }
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::FlycompSelectNo => {
                if let ContentMode::TabCompletionAskForFlycomp {
                    ref mut selection, ..
                } = app.content_mode
                {
                    *selection = FlycompPromptSelection::No;
                    if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
                        app.content_mode = ContentMode::Normal;
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::FlycompSelectDontAsk => {
                if let ContentMode::TabCompletionAskForFlycomp {
                    ref mut selection, ..
                } = app.content_mode
                {
                    *selection = FlycompPromptSelection::DontAsk;
                    if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left)) {
                        let mode = std::mem::replace(&mut app.content_mode, ContentMode::Normal);
                        if let ContentMode::TabCompletionAskForFlycomp { command_word, .. } = mode {
                            app.settings.flycomp_blacklist.insert(command_word);
                        }
                        MouseActionOutput::new(true, RedrawUrgency::Now)
                    } else {
                        MouseActionOutput::new(false, RedrawUrgency::Now)
                    }
                } else {
                    MouseActionOutput::new(false, RedrawUrgency::Now)
                }
            }
            MouseEventAction::DisableMouseCapture => {
                log::debug!("Disabling mouse capture due to viewport event in smart mode");
                app.mouse_state.disable();
                app.mouse_state.last_mouse_over_cell_semantic = None;
                app.mouse_state.last_mouse_over_cell_direct = None;
                MouseActionOutput::new(false, RedrawUrgency::Now)
            }
            MouseEventAction::RightClickMenuOpen => {
                let content_row = if let Some(ref drawn) = app.last_contents {
                    drawn.term_em_row_to_content_row(mouse.row).max(0) as u16
                } else {
                    mouse.row
                };
                app.right_click_popup_pos = Some(crate::content_builder::Coord::new(
                    content_row,
                    mouse.column,
                ));
                app.mouse_state
                    .set_right_click_down_pos(mouse.row, mouse.column);

                let target = match clicked_tag {
                    Some(Tag::HistoryResult(idx)) => {
                        let source = match &app.content_mode {
                            ContentMode::FuzzyHistorySearch(s) => Some(s.clone()),
                            _ => None,
                        };
                        let text_opt = source.and_then(|s| {
                            let manager = app.select_fuzzy_history_manager(&s);
                            manager.fuzzy_search_command_by_idx(idx)
                        });
                        text_opt.map(crate::app::RightClickCopyTarget::HistoryEntry)
                    }
                    Some(Tag::Ps1PromptCwdWidget(idx)) => app
                        .prompt_manager
                        .cwd_path_for_index(idx)
                        .map(crate::app::RightClickCopyTarget::Cwd),
                    _ => None,
                };

                app.right_click_copy_target = Some(target.unwrap_or_else(|| {
                    if let Some(selection) = app.buffer.selected_text() {
                        crate::app::RightClickCopyTarget::Selection(selection)
                    } else {
                        crate::app::RightClickCopyTarget::Buffer(app.buffer.buffer().to_string())
                    }
                }));

                MouseActionOutput::new(false, RedrawUrgency::Now)
            }
            MouseEventAction::RightClickMenuDismiss => {
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                MouseActionOutput::new(false, RedrawUrgency::Now)
            }
            MouseEventAction::SetPointer(shape) => {
                let mut output = MouseActionOutput::new(false, RedrawUrgency::Soon);
                output.desired_pointer_shape = Some(*shape);
                output
            }
        }
    }
}
