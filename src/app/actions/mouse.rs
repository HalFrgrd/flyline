use crate::app::actions::{ContextExpr, ContextVar, KeyEventAction};
use crate::app::{App, AppRunningState, ContentMode, ExitState, FlycompPromptSelection};
use crate::content_builder::Tag;
use crate::mouse_state::ClickCount;
use crate::settings::MouseMode;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::sync::LazyLock;

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
}

pub struct MouseContextValues {
    pub(crate) left_clicked_down: bool,
    pub(crate) left_clicked_up: bool,
    pub(crate) left_button_down: bool,
    pub(crate) left_button_up: bool,
    pub(crate) right_clicked_down: bool,
    pub(crate) right_clicked_up: bool,
    pub(crate) drag_left: bool,
    pub(crate) scroll_up: bool,
    pub(crate) scroll_down: bool,
    pub(crate) scroll_left: bool,
    pub(crate) scroll_right: bool,
    pub(crate) moved: bool,
    pub(crate) clicked_tag: Option<Tag>,
    pub(crate) direct_tag: Option<Tag>,
    pub(crate) right_click_popup_active: bool,
    pub(crate) right_release_dismiss: bool,
}

impl MouseContextValues {
    pub fn evaluate(
        app: &App,
        mouse: &MouseEvent,
        clicked_tag: Option<Tag>,
        direct_tag: Option<Tag>,
    ) -> Self {
        let left_clicked_down = matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left));
        let left_clicked_up = matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left));
        let left_button_down = app.mouse_state.is_left_button_down();
        let left_button_up = !left_button_down;
        let right_clicked_down = matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right));
        let right_clicked_up = matches!(mouse.kind, MouseEventKind::Up(MouseButton::Right));
        let drag_left = matches!(mouse.kind, MouseEventKind::Drag(MouseButton::Left));
        let scroll_up = matches!(mouse.kind, MouseEventKind::ScrollUp);
        let scroll_down = matches!(mouse.kind, MouseEventKind::ScrollDown);
        let scroll_left = matches!(mouse.kind, MouseEventKind::ScrollLeft);
        let scroll_right = matches!(mouse.kind, MouseEventKind::ScrollRight);
        let moved = matches!(mouse.kind, MouseEventKind::Moved);
        let right_click_popup_active = app.right_click_popup_pos.is_some();
        let right_release_dismiss = if let MouseEventKind::Up(MouseButton::Right) = mouse.kind {
            app.mouse_state
                .right_click_down_pos
                .is_some_and(|(start_row, start_col)| {
                    (mouse.row, mouse.column) != (start_row, start_col)
                })
        } else {
            false
        };

        Self {
            left_clicked_down,
            left_clicked_up,
            left_button_down,
            left_button_up,
            right_clicked_down,
            right_clicked_up,
            drag_left,
            scroll_up,
            scroll_down,
            scroll_left,
            scroll_right,
            moved,
            clicked_tag,
            direct_tag,
            right_click_popup_active,
            right_release_dismiss,
        }
    }
}

impl MouseContextVar {
    pub fn evaluate(&self, app: &App, ctx: &MouseContextValues) -> bool {
        match self {
            MouseContextVar::LeftButtonClickedDown => ctx.left_clicked_down,
            MouseContextVar::LeftButtonClickedUp => ctx.left_clicked_up,
            MouseContextVar::LeftButtonIsDown => ctx.left_button_down,
            MouseContextVar::LeftButtonIsUp => ctx.left_button_up,
            MouseContextVar::RightButtonClickedDown => ctx.right_clicked_down,
            MouseContextVar::RightButtonClickedUp => ctx.right_clicked_up,
            MouseContextVar::DragLeft => ctx.drag_left,
            MouseContextVar::ScrollUp => ctx.scroll_up,
            MouseContextVar::ScrollDown => ctx.scroll_down,
            MouseContextVar::ScrollLeft => ctx.scroll_left,
            MouseContextVar::ScrollRight => ctx.scroll_right,
            MouseContextVar::Moved => ctx.moved,
            MouseContextVar::OverCellSemantically(pattern) => pattern.matches(ctx.clicked_tag),
            MouseContextVar::NotOverCellSemantically(pattern) => !pattern.matches(ctx.clicked_tag),
            MouseContextVar::OverCellDirectly(pattern) => pattern.matches(ctx.direct_tag),
            MouseContextVar::SmartModeClickAboveViewport => {
                app.settings.mouse_mode == MouseMode::Smart
                    && ctx.left_clicked_down
                    && app.last_contents.as_ref().is_some_and(|c| {
                        if let Some(mouse_info) = app.last_mouse.as_ref().map(|(m, _)| m) {
                            mouse_info.row < c.viewport_start
                        } else {
                            false
                        }
                    })
            }
            MouseContextVar::SmartModeScroll => {
                app.settings.mouse_mode == MouseMode::Smart
                    && (ctx.scroll_up || ctx.scroll_down || ctx.scroll_left || ctx.scroll_right)
            }
            MouseContextVar::IsOverSuggestions => matches!(
                ctx.clicked_tag,
                Some(Tag::Suggestion(_))
                    | Some(Tag::TabSuggestion)
                    | Some(Tag::TabCompletionScrollBar { .. })
            ),
            MouseContextVar::IsOverFuzzyHistory => matches!(
                ctx.clicked_tag,
                Some(Tag::HistoryResult(_)) | Some(Tag::FuzzySearch)
            ),
            MouseContextVar::ScrollBarDrag => {
                matches!(
                    app.mouse_state.drag_start_tag,
                    Some(Tag::TabCompletionScrollBar { .. })
                ) && (ctx.left_button_down || ctx.drag_left)
            }
            MouseContextVar::RightClickPopupActive => ctx.right_click_popup_active,
            MouseContextVar::RightReleaseDismiss => ctx.right_release_dismiss,
        }
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
    DragCommand,
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
}

pub struct MouseBinding {
    pub(crate) context: ContextExpr,
    pub(crate) mouse_event: Vec<MouseContextVar>,
    pub(crate) action: MouseEventAction,
}

pub static DEFAULT_MOUSE_BINDINGS: LazyLock<Vec<MouseBinding>> = LazyLock::new(|| {
    vec![
        // Smart mode viewport click or scroll -> Disable mouse capture
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![MouseContextVar::SmartModeScroll],
            action: MouseEventAction::DisableMouseCapture,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![MouseContextVar::SmartModeClickAboveViewport],
            action: MouseEventAction::DisableMouseCapture,
        },
        // Right click menu popup opening
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::RightButtonClickedDown,
                MouseContextVar::NotOverCellSemantically(TagPattern::RightClickMenu),
            ],
            action: MouseEventAction::RightClickMenuOpen,
        },
        // Right click menu popup dismissal on release scroll/click outside
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::RightClickPopupActive,
                MouseContextVar::RightReleaseDismiss,
            ],
            action: MouseEventAction::RightClickMenuDismiss,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::RightClickPopupActive,
                MouseContextVar::LeftButtonClickedDown,
                MouseContextVar::NotOverCellSemantically(TagPattern::RightClickMenu),
            ],
            action: MouseEventAction::RightClickMenuDismiss,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::RightClickPopupActive,
                MouseContextVar::ScrollUp,
                MouseContextVar::NotOverCellSemantically(TagPattern::RightClickMenu),
            ],
            action: MouseEventAction::RightClickMenuDismiss,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::RightClickPopupActive,
                MouseContextVar::ScrollDown,
                MouseContextVar::NotOverCellSemantically(TagPattern::RightClickMenu),
            ],
            action: MouseEventAction::RightClickMenuDismiss,
        },
        // Right click menu options (activated by Left Click Release / Up)
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::RightClickCopy),
            ],
            action: MouseEventAction::CopySelection,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::RightClickCut),
            ],
            action: MouseEventAction::CutSelection,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::RightClickPaste),
            ],
            action: MouseEventAction::PasteSelection,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::RightClickUndo),
            ],
            action: MouseEventAction::Undo,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::RightClickRedo),
            ],
            action: MouseEventAction::Redo,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::RightClickRunTutorial),
            ],
            action: MouseEventAction::RunTutorial,
        },
        // Scrolling in suggestions
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletion),
            mouse_event: vec![
                MouseContextVar::ScrollUp,
                MouseContextVar::IsOverSuggestions,
            ],
            action: MouseEventAction::ScrollSuggestionsUp,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletion),
            mouse_event: vec![
                MouseContextVar::ScrollDown,
                MouseContextVar::IsOverSuggestions,
            ],
            action: MouseEventAction::ScrollSuggestionsDown,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletion),
            mouse_event: vec![
                MouseContextVar::ScrollLeft,
                MouseContextVar::IsOverSuggestions,
            ],
            action: MouseEventAction::ScrollSuggestionsLeft,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletion),
            mouse_event: vec![
                MouseContextVar::ScrollRight,
                MouseContextVar::IsOverSuggestions,
            ],
            action: MouseEventAction::ScrollSuggestionsRight,
        },
        // Scrollbar Dragging
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletion),
            mouse_event: vec![MouseContextVar::ScrollBarDrag],
            action: MouseEventAction::ScrollSuggestionsBar,
        },
        // Scrolling in history
        MouseBinding {
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            mouse_event: vec![
                MouseContextVar::ScrollUp,
                MouseContextVar::IsOverFuzzyHistory,
            ],
            action: MouseEventAction::ScrollHistoryUp,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            mouse_event: vec![
                MouseContextVar::ScrollDown,
                MouseContextVar::IsOverFuzzyHistory,
            ],
            action: MouseEventAction::ScrollHistoryDown,
        },
        // Directory selection hover protection (prevents dismissal when hovering select widgets)
        MouseBinding {
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            ],
            action: MouseEventAction::HoverClearTooltip,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            ],
            action: MouseEventAction::HoverClearTooltip,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::NotOverCellSemantically(TagPattern::Ps1PromptCwd),
                MouseContextVar::NotOverCellSemantically(TagPattern::PromptCopyBuffer),
            ],
            action: MouseEventAction::PromptDirSelectDismiss,
        },
        // Hovering selection updates
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletion),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            ],
            action: MouseEventAction::HoverSuggestion,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            ],
            action: MouseEventAction::HoverHistoryResult,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::AgentOutputSelection),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            ],
            action: MouseEventAction::HoverAiResult,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::OverCellSemantically(TagPattern::Command),
            ],
            action: MouseEventAction::HoverCommand,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::Moved,
                MouseContextVar::NotOverCellSemantically(TagPattern::Command),
            ],
            action: MouseEventAction::HoverClearTooltip,
        },
        // Selecting/Accepting options
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletion),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::Suggestion),
            ],
            action: MouseEventAction::AcceptSuggestion,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::HistoryResult),
            ],
            action: MouseEventAction::AcceptHistoryResult,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::AgentOutputSelection),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::AiResult),
            ],
            action: MouseEventAction::AcceptAiResult,
        },
        // Command clicking and selection
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedDown,
                MouseContextVar::OverCellSemantically(TagPattern::Command),
            ],
            action: MouseEventAction::ClickCommand,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::DragLeft,
                MouseContextVar::OverCellSemantically(TagPattern::Command),
            ],
            action: MouseEventAction::DragCommand,
        },
        // Tutorial
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::TutorialPrev),
            ],
            action: MouseEventAction::ClickTutorialPrev,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::TutorialNext),
            ],
            action: MouseEventAction::ClickTutorialNext,
        },
        // Ps1 Cwd Click / Accept
        MouseBinding {
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            ],
            action: MouseEventAction::PromptDirAccept,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedDown,
                MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            ],
            action: MouseEventAction::PromptDirSelect,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::DragLeft,
                MouseContextVar::OverCellSemantically(TagPattern::Ps1PromptCwd),
            ],
            action: MouseEventAction::PromptDirSelect,
        },
        // Clipboard
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::Clipboard),
            ],
            action: MouseEventAction::ClickClipboard,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::Always),
            mouse_event: vec![
                MouseContextVar::LeftButtonClickedUp,
                MouseContextVar::OverCellSemantically(TagPattern::PromptCopyBuffer),
            ],
            action: MouseEventAction::ClickPromptCopyBuffer,
        },
        // Flycomp ask prompt
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            mouse_event: vec![MouseContextVar::OverCellSemantically(
                TagPattern::FlycompYes,
            )],
            action: MouseEventAction::FlycompSelectYes,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            mouse_event: vec![MouseContextVar::OverCellSemantically(TagPattern::FlycompNo)],
            action: MouseEventAction::FlycompSelectNo,
        },
        MouseBinding {
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            mouse_event: vec![MouseContextVar::OverCellSemantically(
                TagPattern::FlycompDontAsk,
            )],
            action: MouseEventAction::FlycompSelectDontAsk,
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
    ) -> bool {
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
                true
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
                true
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
                true
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
                true
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
                true
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
                true
            }
            MouseEventAction::ScrollSuggestionsUp => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_up_arrow();
                }
                false
            }
            MouseEventAction::ScrollSuggestionsDown => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_down_arrow();
                }
                false
            }
            MouseEventAction::ScrollSuggestionsLeft => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_left_arrow();
                }
                false
            }
            MouseEventAction::ScrollSuggestionsRight => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_right_arrow();
                }
                false
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
                false
            }
            MouseEventAction::ScrollHistoryUp => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    let source = source.clone();
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Forward);
                }
                false
            }
            MouseEventAction::ScrollHistoryDown => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    let source = source.clone();
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Backward);
                }
                false
            }
            MouseEventAction::HoverSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        log::debug!("Setting selected by idx: {}", idx);
                        active_suggestions.set_selected_by_idx(idx);
                    }
                }
                false
            }
            MouseEventAction::HoverHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                        let source = source.clone();
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                    }
                }
                false
            }
            MouseEventAction::HoverAiResult => {
                if let Some(Tag::AiResult(idx)) = clicked_tag {
                    if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                        selection.set_selected_by_idx(idx);
                    }
                }
                false
            }
            MouseEventAction::HoverCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if let Some(part) = app.formatted_buffer_cache.get_part_from_byte_pos(byte_pos)
                        && let Some(tooltip) = part.tooltip.as_ref()
                    {
                        app.tooltip = Some(tooltip.clone());
                    }
                }
                false
            }
            MouseEventAction::HoverClearTooltip => {
                app.tooltip = None;
                false
            }
            MouseEventAction::AcceptSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        active_suggestions.set_selected_by_idx(idx);
                        active_suggestions.accept_selected_filtered_item(&mut app.buffer);
                        app.content_mode = ContentMode::Normal;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            MouseEventAction::AcceptHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                        let source = source.clone();
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                        app.accept_fuzzy_history_search();
                        true
                    } else {
                        false
                    }
                } else {
                    false
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
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            MouseEventAction::ClickCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let left_click_count = app.mouse_state.record_left_click_down(byte_pos);
                        match left_click_count {
                            ClickCount::Single => {
                                let extend_selection =
                                    mouse.modifiers.contains(KeyModifiers::SHIFT);
                                if extend_selection {
                                    app.buffer.start_selection_if_none();
                                } else {
                                    app.buffer.clear_selection();
                                }
                                app.buffer.try_move_cursor_to_byte_pos(
                                    byte_pos,
                                    !cursor_directly_on_cell,
                                );
                                if !extend_selection {
                                    app.buffer.start_selection_if_none();
                                }
                            }
                            ClickCount::Double => {
                                app.buffer.try_move_cursor_to_byte_pos(
                                    byte_pos,
                                    !cursor_directly_on_cell,
                                );
                                app.buffer.select_word();
                            }
                            ClickCount::Triple => {
                                app.buffer.select_entire_buffer();
                            }
                            _ => {}
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            MouseEventAction::DragCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if app.settings.select_with_mouse {
                        let active_drag_tag = app.mouse_state.drag_start_tag;
                        if matches!(active_drag_tag, Some(Tag::Command(_))) {
                            match (
                                app.mouse_state.get_click_count(),
                                app.mouse_state.get_last_click_buffer_pos(),
                            ) {
                                (ClickCount::Double, Some(drag_start_pos)) => {
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
                                (ClickCount::Triple, _) => {
                                    app.buffer.select_entire_buffer();
                                }
                                _ => {
                                    app.buffer.start_selection_if_none();
                                    app.buffer.try_move_cursor_to_byte_pos(
                                        byte_pos,
                                        !cursor_directly_on_cell,
                                    );
                                }
                            }
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            MouseEventAction::ClickTutorialPrev => {
                app.settings.tutorial_step.prev();
                log::info!(
                    "Tutorial navigated to prev: {:?}",
                    app.settings.tutorial_step
                );
                false
            }
            MouseEventAction::ClickTutorialNext => {
                app.settings.tutorial_step.next();
                log::info!(
                    "Tutorial navigated to next: {:?}",
                    app.settings.tutorial_step
                );
                false
            }
            MouseEventAction::PromptDirAccept => {
                KeyEventAction::PromptDirAcceptEntry.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                true
            }
            MouseEventAction::PromptDirSelect => {
                if let Some(Tag::Ps1PromptCwdWidget(idx)) = clicked_tag {
                    app.content_mode = ContentMode::PromptDirSelect(idx);
                }
                false
            }
            MouseEventAction::PromptDirSelectDismiss => {
                if matches!(app.content_mode, ContentMode::PromptDirSelect(_)) {
                    app.content_mode = ContentMode::Normal;
                }
                false
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
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            MouseEventAction::ClickPromptCopyBuffer => {
                let text = app.buffer.buffer().to_string();
                if app.copy_to_clipboard(text.as_bytes()) {
                    log::info!("Copied current buffer to clipboard via copy-buffer widget");
                    true
                } else {
                    false
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
                        true
                    } else {
                        false
                    }
                } else {
                    false
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
                        true
                    } else {
                        false
                    }
                } else {
                    false
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
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            MouseEventAction::DisableMouseCapture => {
                log::debug!("Disabling mouse capture due to viewport event in smart mode");
                app.mouse_state.disable();
                app.mouse_state.last_mouse_over_cell_semantic = None;
                app.mouse_state.last_mouse_over_cell_direct = None;
                false
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

                false
            }
            MouseEventAction::RightClickMenuDismiss => {
                app.right_click_popup_pos = None;
                app.right_click_copy_target = None;
                false
            }
        }
    }
}
