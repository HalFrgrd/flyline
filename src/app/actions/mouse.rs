use crate::app::actions::{ContextExpr, ContextLiteral, KeyEventAction};
use crate::app::{App, AppRunningState, ContentMode, ExitState, FlycompPromptSelection};
use crate::content::{Coord, Tag};
use crate::mouse_state::{ClickCount, PointerShape, mouse_state};
use crate::settings::MouseMode;
use std::sync::LazyLock;
use termina::event::{
    KeyCode, KeyEvent, Modifiers as KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

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
    pub fn update_now() -> Self {
        Self {
            possible_buffer_change: true,
            desired_pointer_shape: None,
            redraw_urgency: RedrawUrgency::Now,
        }
    }

    pub fn update_soon() -> Self {
        Self {
            possible_buffer_change: true,
            desired_pointer_shape: None,
            redraw_urgency: RedrawUrgency::Soon,
        }
    }

    pub fn dont_update() -> Self {
        Self {
            possible_buffer_change: false,
            desired_pointer_shape: None,
            redraw_urgency: RedrawUrgency::Soon,
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
            (TagPattern::Ps1PromptCwd, Some(Tag::PromptCwdWidget(_))) => true,
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
    QuadClick,
    DragStartCommand,
    DragStartPointerTarget,
    IsPointerTarget,
    IsMouseScrolling,
}

impl super::ContextVar for MouseContextVar {
    fn evaluate(&self, app: &App) -> bool {
        let last_mouse = app.last_mouse.as_ref().map(|lm| lm.mouse);
        let clicked_tag = mouse_state(|m| m.last_mouse_over_cell_semantic);
        let direct_tag = mouse_state(|m| m.last_mouse_over_cell_direct);

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
            MouseContextVar::LeftButtonIsDown => mouse_state(|m| m.is_left_button_down()),
            MouseContextVar::LeftButtonIsUp => !mouse_state(|m| m.is_left_button_down()),
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
                            && app.last_contents.as_ref().is_some_and(|c| {
                                c.viewport_start
                                    .is_some_and(|viewport_start| m.row < viewport_start)
                            })
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
                    | Some(Tag::AutoCompletionTimeInfo)
            ),
            MouseContextVar::IsOverFuzzyHistory => matches!(
                clicked_tag,
                Some(Tag::HistoryResult(_)) | Some(Tag::FuzzySearch)
            ),
            MouseContextVar::ScrollBarDrag => mouse_state(|m| {
                matches!(m.drag_start_tag, Some(Tag::TabCompletionScrollBar { .. }))
                    && (m.is_left_button_down()
                        || last_mouse.is_some_and(|m| {
                            matches!(m.kind, MouseEventKind::Drag(MouseButton::Left))
                        }))
            }),
            MouseContextVar::RightClickPopupActive => app.right_click_popup_pos.is_some(),
            MouseContextVar::RightReleaseDismiss => last_mouse.is_some_and(|m| {
                matches!(m.kind, MouseEventKind::Up(MouseButton::Right))
                    && mouse_state(|m_st| {
                        m_st.right_click_down_pos
                            .is_some_and(|(start_row, start_col)| {
                                (m.row, m.column) != (start_row, start_col)
                            })
                    })
            }),
            MouseContextVar::SingleClick => {
                mouse_state(|m| m.get_click_count()) == ClickCount::Single
            }
            MouseContextVar::DoubleClick => {
                mouse_state(|m| m.get_click_count()) == ClickCount::Double
            }
            MouseContextVar::TripleClick => {
                mouse_state(|m| m.get_click_count()) == ClickCount::Triple
            }
            MouseContextVar::QuadClick => mouse_state(|m| m.get_click_count()) == ClickCount::Quad,
            MouseContextVar::DragStartCommand => {
                mouse_state(|m| matches!(m.drag_start_tag, Some(Tag::Command(_))))
            }
            MouseContextVar::DragStartPointerTarget => is_pointer_target_tag(
                mouse_state(|m| m.drag_start_tag),
                app.right_click_popup_pos.is_some(),
            ),
            MouseContextVar::IsPointerTarget => is_pointer_target_tag(
                mouse_state(|m| m.last_mouse_over_cell_direct),
                app.right_click_popup_pos.is_some(),
            ),
            MouseContextVar::IsMouseScrolling => mouse_state(|m| m.is_mouse_scrolling()),
        }
    }

    fn display(&self) -> String {
        format!("{:?}", self)
    }
}

fn is_pointer_target_tag(tag: Option<Tag>, right_click_popup_active: bool) -> bool {
    if right_click_popup_active {
        tag.is_some_and(|t| {
            matches!(
                t,
                Tag::RightClickCopy
                    | Tag::RightClickCut
                    | Tag::RightClickPaste
                    | Tag::RightClickUndo
                    | Tag::RightClickRedo
                    | Tag::RightClickRunTutorial
            )
        })
    } else {
        tag.is_some_and(|t| {
            matches!(
                t,
                Tag::Suggestion(_)
                    | Tag::HistoryResult(_)
                    | Tag::AiResult(_)
                    | Tag::TutorialPrev
                    | Tag::TutorialNext
                    | Tag::PromptCopyBufferWidget
                    | Tag::Clipboard(_)
                    | Tag::PromptCwdWidget(_)
                    | Tag::TabCompletionScrollBar { .. }
                    | Tag::FlycompSandboxInfo
                    | Tag::FlycompInfo
                    | Tag::AutoCompletionTimeInfo
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
    Copy,
    Cut,
    Paste,
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
    ReleaseCommand,
    SelectWord,
    SelectLine,
    SelectAll,
    DragCommand,
    DragWord,
    DragLine,
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

#[derive(Debug, Clone)]
pub struct MouseBinding {
    pub(crate) context: super::ContextExpr<MouseContextVar>,
    pub(crate) actions: Vec<MouseEventAction>,
}

impl MouseBinding {
    pub fn new(context: super::ContextExpr<MouseContextVar>, actions: &[MouseEventAction]) -> Self {
        Self {
            context,
            actions: actions.to_vec(),
        }
    }
}

pub static DEFAULT_MOUSE_BINDINGS: LazyLock<Vec<MouseBinding>> = LazyLock::new(|| {
    vec![
        // Highest Priority: Right click popup dismissal on click / scroll outside
        MouseBinding::new(
            MouseContextVar::RightClickPopupActive
                + MouseContextVar::LeftButtonClickedUp
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            &[MouseEventAction::RightClickMenuDismiss],
        ),
        MouseBinding::new(
            MouseContextVar::RightClickPopupActive + MouseContextVar::RightReleaseDismiss,
            &[MouseEventAction::RightClickMenuDismiss],
        ),
        MouseBinding::new(
            MouseContextVar::RightClickPopupActive
                + MouseContextVar::ScrollUp
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            &[MouseEventAction::RightClickMenuDismiss],
        ),
        MouseBinding::new(
            MouseContextVar::RightClickPopupActive
                + MouseContextVar::ScrollDown
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            &[MouseEventAction::RightClickMenuDismiss],
        ),
        // Right click menu popup opening
        MouseBinding::new(
            MouseContextVar::RightButtonClickedDown
                + !MouseContextVar::OverCellSemantically(TagPattern::RightClickMenu),
            &[
                MouseEventAction::HoverSuggestion,
                MouseEventAction::HoverHistoryResult,
                MouseEventAction::HoverAiResult,
                MouseEventAction::HoverCommand,
                MouseEventAction::RightClickMenuOpen,
            ],
        ),
        // Right click menu options (activated by Left Click Release / Up)
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickCopy),
            &[MouseEventAction::Copy],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickCut),
            &[MouseEventAction::Cut],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickPaste),
            &[MouseEventAction::Paste],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickUndo),
            &[MouseEventAction::Undo],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickRedo),
            &[MouseEventAction::Redo],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::RightClickRunTutorial),
            &[MouseEventAction::RunTutorial],
        ),
        // Scrolling in suggestions
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::ScrollUp
                + MouseContextVar::IsOverSuggestions,
            &[MouseEventAction::ScrollSuggestionsUp],
        ),
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::ScrollDown
                + MouseContextVar::IsOverSuggestions,
            &[MouseEventAction::ScrollSuggestionsDown],
        ),
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::ScrollLeft
                + MouseContextVar::IsOverSuggestions,
            &[MouseEventAction::ScrollSuggestionsLeft],
        ),
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::ScrollRight
                + MouseContextVar::IsOverSuggestions,
            &[MouseEventAction::ScrollSuggestionsRight],
        ),
        // Scrollbar Dragging
        MouseBinding::new(
            MouseContextVar::TabCompletion + MouseContextVar::ScrollBarDrag,
            &[MouseEventAction::ScrollSuggestionsBar],
        ),
        // Scrolling in history
        MouseBinding::new(
            MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::ScrollUp
                + MouseContextVar::IsOverFuzzyHistory,
            &[MouseEventAction::ScrollHistoryUp],
        ),
        MouseBinding::new(
            MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::ScrollDown
                + MouseContextVar::IsOverFuzzyHistory,
            &[MouseEventAction::ScrollHistoryDown],
        ),
        // Directory selection hover protection (prevents dismissal when hovering select widgets)
        MouseBinding::new(
            MouseContextVar::PromptDirSelection
                + MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            &[MouseEventAction::HoverClearTooltip],
        ),
        MouseBinding::new(
            MouseContextVar::PromptDirSelection
                + MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            &[MouseEventAction::HoverClearTooltip],
        ),
        MouseBinding::new(
            MouseContextVar::PromptDirSelection
                + MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd)
                + !MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            &[MouseEventAction::PromptDirSelectDismiss],
        ),
        // Flycomp ask prompt
        MouseBinding::new(
            MouseContextVar::TabCompletionAskForFlycomp
                + MouseContextVar::OverCellSemantically(TagPattern::FlycompYes),
            &[MouseEventAction::FlycompSelectYes],
        ),
        MouseBinding::new(
            MouseContextVar::TabCompletionAskForFlycomp
                + MouseContextVar::OverCellSemantically(TagPattern::FlycompNo),
            &[MouseEventAction::FlycompSelectNo],
        ),
        MouseBinding::new(
            MouseContextVar::TabCompletionAskForFlycomp
                + MouseContextVar::OverCellSemantically(TagPattern::FlycompDontAsk),
            &[MouseEventAction::FlycompSelectDontAsk],
        ),
        // Hovering / clicking selection updates
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            &[MouseEventAction::HoverSuggestion],
        ),
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::DragLeft
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            &[MouseEventAction::HoverSuggestion],
        ),
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::LeftButtonClickedDown
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            &[MouseEventAction::HoverSuggestion],
        ),
        MouseBinding::new(
            MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            &[MouseEventAction::HoverHistoryResult],
        ),
        MouseBinding::new(
            MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::DragLeft
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            &[MouseEventAction::HoverHistoryResult],
        ),
        MouseBinding::new(
            MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::LeftButtonClickedDown
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            &[MouseEventAction::HoverHistoryResult],
        ),
        MouseBinding::new(
            MouseContextVar::AgentOutputSelection
                + MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            &[MouseEventAction::HoverAiResult],
        ),
        MouseBinding::new(
            MouseContextVar::AgentOutputSelection
                + MouseContextVar::DragLeft
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            &[MouseEventAction::HoverAiResult],
        ),
        MouseBinding::new(
            MouseContextVar::AgentOutputSelection
                + MouseContextVar::LeftButtonClickedDown
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::IsMouseScrolling
                + MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            &[MouseEventAction::HoverAiResult],
        ),
        MouseBinding::new(
            MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[MouseEventAction::HoverCommand],
        ),
        MouseBinding::new(
            MouseContextVar::Moved
                + !MouseContextVar::RightClickPopupActive
                + !MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[MouseEventAction::HoverClearTooltip],
        ),
        // Selecting/Accepting options
        MouseBinding::new(
            MouseContextVar::TabCompletion
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            &[MouseEventAction::AcceptSuggestion],
        ),
        MouseBinding::new(
            MouseContextVar::FuzzyHistorySearch
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            &[MouseEventAction::AcceptHistoryResult],
        ),
        MouseBinding::new(
            MouseContextVar::AgentOutputSelection
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            &[MouseEventAction::AcceptAiResult],
        ),
        // Command clicking (single, double, triple clicks)
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::SingleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[
                MouseEventAction::RightClickMenuDismiss,
                MouseEventAction::ClickCommand,
            ],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::DoubleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[
                MouseEventAction::RightClickMenuDismiss,
                MouseEventAction::SelectWord,
            ],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::TripleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[
                MouseEventAction::RightClickMenuDismiss,
                MouseEventAction::SelectLine,
            ],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::QuadClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[
                MouseEventAction::RightClickMenuDismiss,
                MouseEventAction::SelectAll,
            ],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[MouseEventAction::ReleaseCommand],
        ),
        // Command dragging
        MouseBinding::new(
            MouseContextVar::DragLeft
                + MouseContextVar::SingleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[
                MouseEventAction::RightClickMenuDismiss,
                MouseEventAction::DragCommand,
            ],
        ),
        MouseBinding::new(
            MouseContextVar::DragLeft
                + MouseContextVar::DoubleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[MouseEventAction::DragWord],
        ),
        MouseBinding::new(
            MouseContextVar::DragLeft
                + MouseContextVar::TripleClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[MouseEventAction::DragLine],
        ),
        MouseBinding::new(
            MouseContextVar::DragLeft
                + MouseContextVar::QuadClick
                + MouseContextVar::OverCellSemantically(TagPattern::Command),
            &[MouseEventAction::DragAll],
        ),
        // Tutorial
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::TutorialPrev),
            &[MouseEventAction::ClickTutorialPrev],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::TutorialNext),
            &[MouseEventAction::ClickTutorialNext],
        ),
        // Ps1 Cwd Click / Accept
        MouseBinding::new(
            MouseContextVar::PromptDirSelection
                + MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            &[MouseEventAction::PromptDirAccept],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedDown
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            &[MouseEventAction::PromptDirSelect],
        ),
        MouseBinding::new(
            MouseContextVar::DragLeft
                + MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            &[MouseEventAction::PromptDirSelect],
        ),
        // Clipboard
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::Clipboard),
            &[MouseEventAction::ClickClipboard],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonClickedUp
                + MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            &[MouseEventAction::ClickPromptCopyBuffer],
        ),
        // Smart mode viewport click or scroll -> Disable mouse capture
        MouseBinding::new(
            ContextExpr::from(MouseContextVar::SmartModeScroll),
            &[MouseEventAction::DisableMouseCapture],
        ),
        MouseBinding::new(
            ContextExpr::from(MouseContextVar::SmartModeClickAboveViewport),
            &[MouseEventAction::DisableMouseCapture],
        ),
    ]
});

pub static DEFAULT_POINTER_SHAPE_BINDINGS: LazyLock<Vec<MouseBinding>> = LazyLock::new(|| {
    vec![
        MouseBinding::new(
            ContextExpr::from(MouseContextVar::OverCellDirectly(TagPattern::Command)),
            &[MouseEventAction::SetPointer(PointerShape::Text)],
        ),
        MouseBinding::new(
            ContextExpr::from(MouseContextVar::DragStartCommand),
            &[MouseEventAction::SetPointer(PointerShape::Text)],
        ),
        MouseBinding::new(
            MouseContextVar::LeftButtonIsDown + MouseContextVar::DragStartPointerTarget,
            &[MouseEventAction::SetPointer(PointerShape::Grabbing)],
        ),
        MouseBinding::new(
            !MouseContextVar::LeftButtonIsDown + MouseContextVar::IsPointerTarget,
            &[MouseEventAction::SetPointer(PointerShape::Pointer)],
        ),
        MouseBinding::new(
            !MouseContextVar::LeftButtonIsDown
                + !MouseContextVar::OverCellDirectly(TagPattern::Command)
                + !MouseContextVar::IsPointerTarget,
            &[MouseEventAction::SetPointer(PointerShape::Default)],
        ),
    ]
});

impl MouseEventAction {
    pub(crate) fn run(&self, app: &mut App, mouse: MouseEvent) -> MouseActionOutput {
        let clicked_tag = mouse_state(|m| m.last_mouse_over_cell_semantic);
        let move_past_final = !matches!(
            mouse_state(|m| m.last_mouse_over_cell_direct),
            Some(Tag::Command(_))
        );

        match self {
            MouseEventAction::Copy => {
                KeyEventAction::CopyTarget
                    .run(app, KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
                app.right_click_popup_pos = None;
                MouseActionOutput::update_now()
            }
            MouseEventAction::Cut => {
                KeyEventAction::CutSelection
                    .run(app, KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
                app.right_click_popup_pos = None;
                MouseActionOutput::update_now()
            }
            MouseEventAction::Paste => {
                KeyEventAction::PasteSystemClipboard
                    .run(app, KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                MouseActionOutput::update_now()
            }
            MouseEventAction::Undo => {
                KeyEventAction::Undo.run(app, KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                MouseActionOutput::update_now()
            }
            MouseEventAction::Redo => {
                KeyEventAction::Redo.run(app, KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                MouseActionOutput::update_now()
            }
            MouseEventAction::RunTutorial => {
                app.settings.run_tutorial = true;
                app.settings.tutorial_step = crate::tutorial::TutorialStep::Welcome;
                use termina::escape::csi::{Csi, Cursor, Edit, EraseInDisplay};
                if let Err(e) = crate::flush_stdout!(
                    "{}{}",
                    Csi::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseDisplay)),
                    Csi::Cursor(Cursor::goto(0, 0))
                ) {
                    log::warn!("Failed to clear terminal: {}", e);
                }
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                app.mode = AppRunningState::Exiting(ExitState::WithoutCommand);
                MouseActionOutput::update_now()
            }
            MouseEventAction::ScrollSuggestionsUp => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_up_arrow();
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ScrollSuggestionsDown => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_down_arrow();
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ScrollSuggestionsLeft => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_left_arrow();
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ScrollSuggestionsRight => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_right_arrow();
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ScrollSuggestionsBar => {
                let active_drag_tag = mouse_state(|m| m.drag_start_tag);
                if let Some(Tag::TabCompletionScrollBar {
                    max_cell_height,
                    y_start,
                    ..
                }) = active_drag_tag
                {
                    if let Some(ref drawn) = app.last_contents
                        && let Some(min_row) = drawn.content_row_to_term_em_row(y_start)
                    {
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
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ScrollHistoryUp => {
                if let ContentMode::FuzzyHistorySearch(source) = app.content_mode {
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Forward);
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ScrollHistoryDown => {
                if let ContentMode::FuzzyHistorySearch(source) = app.content_mode {
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Backward);
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::HoverSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        log::debug!("Setting selected by idx: {}", idx);
                        active_suggestions.set_selected_by_idx(idx);
                    }
                }
                // Noticeably smooth if we update_now
                MouseActionOutput::update_now()
            }
            MouseEventAction::HoverHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(source) = app.content_mode {
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                    }
                }
                // Noticeably smooth if we update_now
                MouseActionOutput::update_now()
            }
            MouseEventAction::HoverAiResult => {
                if let Some(Tag::AiResult(idx)) = clicked_tag {
                    if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                        selection.set_selected_by_idx(idx);
                    }
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::HoverCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if let Some(part) = app.formatted_buffer_cache.get_part_from_byte_pos(byte_pos)
                        && let Some(tooltip) = part.tooltip.as_ref()
                    {
                        app.tooltip = Some(tooltip.clone());
                    }
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::HoverClearTooltip => {
                app.tooltip = None;
                MouseActionOutput::dont_update()
            }
            MouseEventAction::AcceptSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        active_suggestions.set_selected_by_idx(idx);
                        active_suggestions.accept_selected_filtered_item(&mut app.buffer);
                        app.content_mode = ContentMode::Normal;
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::AcceptHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                        let source = source.clone();
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                        app.accept_fuzzy_history_search();
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
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
                            MouseActionOutput::update_now()
                        } else {
                            MouseActionOutput::dont_update()
                        }
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
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

                        let target_pos = byte_pos;

                        app.buffer
                            .try_move_cursor_to_byte_pos(target_pos, move_past_final);
                        if !extend_selection {
                            app.buffer.start_selection_if_none();
                        }
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::ReleaseCommand => MouseActionOutput::update_now(),
            MouseEventAction::SelectAll => {
                if app.settings.select_with_mouse {
                    app.buffer.select_entire_buffer();
                    MouseActionOutput::update_now()
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::SelectWord => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        app.buffer
                            .try_move_cursor_to_byte_pos(byte_pos, move_past_final);
                        app.buffer.select_word_using_mouse();
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::SelectLine => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        app.buffer
                            .try_move_cursor_to_byte_pos(byte_pos, move_past_final);
                        app.buffer.select_line_using_mouse();
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::DragCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let active_drag_tag = mouse_state(|m| m.drag_start_tag);
                        if matches!(active_drag_tag, Some(Tag::Command(_))) {
                            app.buffer.start_selection_if_none();

                            app.buffer
                                .try_move_cursor_to_byte_pos(byte_pos, move_past_final);
                            MouseActionOutput::update_soon()
                        } else {
                            MouseActionOutput::dont_update()
                        }
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::DragWord => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let active_drag_tag = mouse_state(|m| m.drag_start_tag);
                        if matches!(active_drag_tag, Some(Tag::Command(_))) {
                            if let Some(drag_start_pos) =
                                mouse_state(|m| m.get_last_click_buffer_pos())
                            {
                                app.buffer
                                    .try_move_cursor_to_byte_pos(drag_start_pos, move_past_final);
                                let anchor_word_sel_range = app.buffer.select_word_using_mouse();
                                app.buffer
                                    .try_move_cursor_to_byte_pos(byte_pos, move_past_final);
                                let new_word_sel_range = app.buffer.select_word_using_mouse();
                                let new_sel_range =
                                    anchor_word_sel_range.start.min(new_word_sel_range.start)
                                        ..anchor_word_sel_range.end.max(new_word_sel_range.end);
                                let cursor_is_left = drag_start_pos > byte_pos;
                                app.buffer
                                    .set_selection_range(new_sel_range, cursor_is_left);
                            }
                            MouseActionOutput::update_soon()
                        } else {
                            MouseActionOutput::dont_update()
                        }
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::DragLine => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let active_drag_tag = mouse_state(|m| m.drag_start_tag);
                        if matches!(active_drag_tag, Some(Tag::Command(_))) {
                            if let Some(drag_start_pos) =
                                mouse_state(|m| m.get_last_click_buffer_pos())
                            {
                                app.buffer
                                    .try_move_cursor_to_byte_pos(drag_start_pos, move_past_final);
                                let anchor_line_sel_range = app.buffer.select_line_using_mouse();
                                app.buffer
                                    .try_move_cursor_to_byte_pos(byte_pos, move_past_final);
                                let new_line_sel_range = app.buffer.select_line_using_mouse();
                                let new_sel_range =
                                    anchor_line_sel_range.start.min(new_line_sel_range.start)
                                        ..anchor_line_sel_range.end.max(new_line_sel_range.end);
                                let cursor_is_left = drag_start_pos > byte_pos;
                                app.buffer
                                    .set_selection_range(new_sel_range, cursor_is_left);
                            }
                            MouseActionOutput::update_soon()
                        } else {
                            MouseActionOutput::dont_update()
                        }
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::DragAll => {
                if app.settings.select_with_mouse {
                    app.buffer.select_entire_buffer();
                    MouseActionOutput::update_soon()
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::ClickTutorialPrev => {
                app.settings.tutorial_step.prev();
                log::info!(
                    "Tutorial navigated to prev: {:?}",
                    app.settings.tutorial_step
                );
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ClickTutorialNext => {
                app.settings.tutorial_step.next();
                log::info!(
                    "Tutorial navigated to next: {:?}",
                    app.settings.tutorial_step
                );
                MouseActionOutput::dont_update()
            }
            MouseEventAction::PromptDirAccept => {
                KeyEventAction::PromptDirAcceptEntry
                    .run(app, KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
                MouseActionOutput::update_now()
            }
            MouseEventAction::PromptDirSelect => {
                if let Some(Tag::PromptCwdWidget(idx)) = clicked_tag {
                    app.content_mode = ContentMode::PromptDirSelect(idx);
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::PromptDirSelectDismiss => {
                if matches!(app.content_mode, ContentMode::PromptDirSelect(_)) {
                    app.content_mode = ContentMode::Normal;
                }
                MouseActionOutput::dont_update()
            }
            MouseEventAction::ClickClipboard => {
                if let Some(tag @ Tag::Clipboard(clipboard_type)) = clicked_tag {
                    if let Some(text) = app
                        .last_contents
                        .as_ref()
                        .and_then(|c| c.contents.clipboards.get(&tag))
                    {
                        let text = text.clone();
                        if app.copy_to_clipboard(text.as_bytes()) {
                            log::info!("Copied to clipboard via OSC 52 ({:?})", clipboard_type);
                        }
                        app.buffer.replace_buffer(&text);
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::ClickPromptCopyBuffer => {
                let text = app.buffer.buffer().to_string();
                if app.copy_to_clipboard(text.as_bytes()) {
                    log::info!("Copied current buffer to clipboard via copy-buffer widget");
                    MouseActionOutput::update_now()
                } else {
                    MouseActionOutput::dont_update()
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
                            ..
                        } = mode
                        {
                            app.run_flycomp(command_word, word_under_cursor);
                        }
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
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
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
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
                            app.settings.flycomp.add_to_blacklist(command_word);
                        }
                        MouseActionOutput::update_now()
                    } else {
                        MouseActionOutput::dont_update()
                    }
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::DisableMouseCapture => {
                log::debug!("Disabling mouse capture due to viewport event in smart mode");
                mouse_state(|m| m.disable());
                mouse_state(|m| {
                    m.last_mouse_over_cell_semantic = None;
                    m.last_mouse_over_cell_direct = None;
                });
                MouseActionOutput::dont_update()
            }
            MouseEventAction::RightClickMenuOpen => {
                let content_row = if let Some(ref drawn) = app.last_contents {
                    drawn
                        .term_em_row_to_content_row(mouse.row)
                        .unwrap_or(0)
                        .max(0) as u16
                } else {
                    mouse.row
                };
                app.right_click_popup_pos = Some(Coord::new(content_row, mouse.column));
                mouse_state(|m| m.set_right_click_down_pos(mouse.row, mouse.column));

                let target = match clicked_tag {
                    Some(Tag::Suggestion(idx)) => {
                        if let ContentMode::TabCompletion(ref active_suggestions) = app.content_mode
                        {
                            active_suggestions
                                .filtered_suggestions
                                .get(idx)
                                .and_then(|item| {
                                    active_suggestions
                                        .processed_suggestions
                                        .get(item.suggestion_idx)
                                })
                                .map(|s| crate::app::RightClickCopyTarget::Suggestion(s.s.clone()))
                        } else {
                            None
                        }
                    }
                    Some(Tag::HistoryResult(idx)) => {
                        let source = match app.content_mode {
                            ContentMode::FuzzyHistorySearch(s) => Some(s),
                            _ => None,
                        };
                        source.and_then(|s| {
                            let manager = app.select_fuzzy_history_manager(&s);
                            let cmd = manager.fuzzy_search_command_by_idx(idx)?;
                            let entry = manager.fuzzy_search_entry_by_idx(idx).cloned();
                            Some(crate::app::RightClickCopyTarget::HistoryEntry(cmd, entry))
                        })
                    }
                    Some(Tag::PromptCwdWidget(idx)) => app
                        .prompt_manager
                        .cwd_path_for_index(idx)
                        .map(crate::app::RightClickCopyTarget::Cwd),
                    Some(Tag::AiResult(idx)) => {
                        if let ContentMode::AgentOutputSelection(ref selection) = app.content_mode {
                            selection.suggestions.get(idx).map(|s| {
                                crate::app::RightClickCopyTarget::AiResult(s.command.clone())
                            })
                        } else {
                            None
                        }
                    }
                    Some(tag @ Tag::Clipboard(_)) => app
                        .last_contents
                        .as_ref()
                        .and_then(|c| c.contents.clipboards.get(&tag))
                        .map(|text| crate::app::RightClickCopyTarget::Clipboard(text.clone())),
                    Some(Tag::PromptCopyBufferWidget) => Some(
                        crate::app::RightClickCopyTarget::Buffer(app.buffer.buffer().to_string()),
                    ),
                    _ => None,
                };

                app.right_click_copy_target = Some(target.unwrap_or_else(|| {
                    if let Some(selection) = app.buffer.selected_text() {
                        crate::app::RightClickCopyTarget::Selection(selection)
                    } else {
                        crate::app::RightClickCopyTarget::Buffer(app.buffer.buffer().to_string())
                    }
                }));

                MouseActionOutput::dont_update()
            }
            MouseEventAction::RightClickMenuDismiss => {
                if app.right_click_popup_pos.is_some() {
                    app.right_click_popup_pos = None;
                    app.right_click_copy_target = None;
                    MouseActionOutput::update_now()
                } else {
                    MouseActionOutput::dont_update()
                }
            }
            MouseEventAction::SetPointer(shape) => {
                let mut output = MouseActionOutput::dont_update();
                output.desired_pointer_shape = Some(*shape);
                output
            }
        }
    }
}
