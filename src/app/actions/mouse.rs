use crate::app::actions::{ContextExpr, ContextVar, KeyEventAction};
use crate::app::{App, AppRunningState, ContentMode, ExitState, FlycompPromptSelection};
use crate::content_builder::Tag;
use crate::mouse_state::ClickCount;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseActionResult {
    Handled,
    HandledUpdateBuffer,
    NotHandled,
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
            _ => false,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventMatch {
    LeftClickUp(TagPattern),
    LeftClickDown(TagPattern),
    LeftDrag(TagPattern),
    RightClickDown(TagPattern),
    ScrollUp(TagPattern),
    ScrollDown(TagPattern),
    ScrollLeft(TagPattern),
    ScrollRight(TagPattern),
    Hover(TagPattern),
}

impl MouseEventMatch {
    pub fn matches(&self, mouse: &MouseEvent, clicked_tag: Option<Tag>) -> bool {
        match (self, &mouse.kind) {
            (MouseEventMatch::LeftClickUp(pattern), MouseEventKind::Up(MouseButton::Left)) => {
                pattern.matches(clicked_tag)
            }
            (MouseEventMatch::LeftClickDown(pattern), MouseEventKind::Down(MouseButton::Left)) => {
                pattern.matches(clicked_tag)
            }
            (MouseEventMatch::LeftDrag(pattern), MouseEventKind::Drag(MouseButton::Left)) => {
                pattern.matches(clicked_tag)
            }
            (
                MouseEventMatch::RightClickDown(pattern),
                MouseEventKind::Down(MouseButton::Right),
            ) => pattern.matches(clicked_tag),
            (MouseEventMatch::ScrollUp(pattern), MouseEventKind::ScrollUp) => {
                pattern.matches(clicked_tag)
            }
            (MouseEventMatch::ScrollDown(pattern), MouseEventKind::ScrollDown) => {
                pattern.matches(clicked_tag)
            }
            (MouseEventMatch::ScrollLeft(pattern), MouseEventKind::ScrollLeft) => {
                pattern.matches(clicked_tag)
            }
            (MouseEventMatch::ScrollRight(pattern), MouseEventKind::ScrollRight) => {
                pattern.matches(clicked_tag)
            }
            (MouseEventMatch::Hover(pattern), MouseEventKind::Moved)
            | (MouseEventMatch::Hover(pattern), MouseEventKind::Drag(_)) => {
                pattern.matches(clicked_tag)
            }
            _ => false,
        }
    }
}

pub struct MouseBinding {
    pub(crate) mouse_event: MouseEventMatch,
    pub(crate) context: ContextExpr,
    pub(crate) action: MouseEventAction,
}

pub static DEFAULT_MOUSE_BINDINGS: LazyLock<Vec<MouseBinding>> = LazyLock::new(|| {
    vec![
        // Right click menu options
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::RightClickCopy),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::CopySelection,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::RightClickCut),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::CutSelection,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::RightClickPaste),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::PasteSelection,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::RightClickUndo),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::Undo,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::RightClickRedo),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::Redo,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::RightClickRunTutorial),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::RunTutorial,
        },
        // Scrolling in suggestions
        MouseBinding {
            mouse_event: MouseEventMatch::ScrollUp(TagPattern::Suggestion),
            context: ContextExpr::from(ContextVar::TabCompletion),
            action: MouseEventAction::ScrollSuggestionsUp,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::ScrollDown(TagPattern::Suggestion),
            context: ContextExpr::from(ContextVar::TabCompletion),
            action: MouseEventAction::ScrollSuggestionsDown,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::ScrollLeft(TagPattern::Suggestion),
            context: ContextExpr::from(ContextVar::TabCompletion),
            action: MouseEventAction::ScrollSuggestionsLeft,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::ScrollRight(TagPattern::Suggestion),
            context: ContextExpr::from(ContextVar::TabCompletion),
            action: MouseEventAction::ScrollSuggestionsRight,
        },
        // Scrolling in history
        MouseBinding {
            mouse_event: MouseEventMatch::ScrollUp(TagPattern::HistoryResult),
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            action: MouseEventAction::ScrollHistoryUp,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::ScrollDown(TagPattern::HistoryResult),
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            action: MouseEventAction::ScrollHistoryDown,
        },
        // Directory selection hover protection (prevents dismissal when hovering select widgets)
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::Ps1PromptCwd),
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            action: MouseEventAction::HoverClearTooltip,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::PromptCopyBuffer),
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            action: MouseEventAction::HoverClearTooltip,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::Any),
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            action: MouseEventAction::PromptDirSelectDismiss,
        },
        // Hovering selection updates
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::Suggestion),
            context: ContextExpr::from(ContextVar::TabCompletion),
            action: MouseEventAction::HoverSuggestion,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::HistoryResult),
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            action: MouseEventAction::HoverHistoryResult,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::AiResult),
            context: ContextExpr::from(ContextVar::AgentOutputSelection),
            action: MouseEventAction::HoverAiResult,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::Command),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::HoverCommand,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::Hover(TagPattern::Any),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::HoverClearTooltip,
        },
        // Selecting/Accepting options
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::Suggestion),
            context: ContextExpr::from(ContextVar::TabCompletion),
            action: MouseEventAction::AcceptSuggestion,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::HistoryResult),
            context: ContextExpr::from(ContextVar::FuzzyHistorySearch),
            action: MouseEventAction::AcceptHistoryResult,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::AiResult),
            context: ContextExpr::from(ContextVar::AgentOutputSelection),
            action: MouseEventAction::AcceptAiResult,
        },
        // Command clicking and selection
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickDown(TagPattern::Command),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::ClickCommand,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftDrag(TagPattern::Command),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::DragCommand,
        },
        // Tutorial
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::TutorialPrev),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::ClickTutorialPrev,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::TutorialNext),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::ClickTutorialNext,
        },
        // Directory selection accept/select
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::Ps1PromptCwd),
            context: ContextExpr::from(ContextVar::PromptDirSelection),
            action: MouseEventAction::PromptDirAccept,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickDown(TagPattern::Ps1PromptCwd),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::PromptDirSelect,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftDrag(TagPattern::Ps1PromptCwd),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::PromptDirSelect,
        },
        // Clipboard
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::Clipboard),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::ClickClipboard,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::PromptCopyBuffer),
            context: ContextExpr::from(ContextVar::Always),
            action: MouseEventAction::ClickPromptCopyBuffer,
        },
        // Flycomp ask prompt
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickDown(TagPattern::FlycompYes),
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            action: MouseEventAction::FlycompSelectYes,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::FlycompYes),
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            action: MouseEventAction::FlycompSelectYes,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickDown(TagPattern::FlycompNo),
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            action: MouseEventAction::FlycompSelectNo,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::FlycompNo),
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            action: MouseEventAction::FlycompSelectNo,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickDown(TagPattern::FlycompDontAsk),
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
            action: MouseEventAction::FlycompSelectDontAsk,
        },
        MouseBinding {
            mouse_event: MouseEventMatch::LeftClickUp(TagPattern::FlycompDontAsk),
            context: ContextExpr::from(ContextVar::TabCompletionAskForFlycomp),
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
    ) -> MouseActionResult {
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
                MouseActionResult::HandledUpdateBuffer
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
                MouseActionResult::HandledUpdateBuffer
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
                MouseActionResult::HandledUpdateBuffer
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
                MouseActionResult::HandledUpdateBuffer
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
                MouseActionResult::HandledUpdateBuffer
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
                MouseActionResult::Handled
            }
            MouseEventAction::ScrollSuggestionsUp => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_up_arrow();
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::ScrollSuggestionsDown => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_down_arrow();
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::ScrollSuggestionsLeft => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_left_arrow();
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::ScrollSuggestionsRight => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_right_arrow();
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::ScrollHistoryUp => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    let source = source.clone();
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Forward);
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::ScrollHistoryDown => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    let source = source.clone();
                    app.select_fuzzy_history_manager_mut(&source)
                        .fuzzy_search_onkeypress(crate::history::HistorySearchDirection::Backward);
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::HoverSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        log::debug!("Setting selected by idx: {}", idx);
                        active_suggestions.set_selected_by_idx(idx);
                    }
                }
                MouseActionResult::NotHandled
            }
            MouseEventAction::HoverHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                        let source = source.clone();
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                    }
                }
                MouseActionResult::NotHandled
            }
            MouseEventAction::HoverAiResult => {
                if let Some(Tag::AiResult(idx)) = clicked_tag {
                    if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                        selection.set_selected_by_idx(idx);
                    }
                }
                MouseActionResult::NotHandled
            }
            MouseEventAction::HoverCommand => {
                if let Some(Tag::Command(byte_pos)) = clicked_tag {
                    if let Some(part) = app.formatted_buffer_cache.get_part_from_byte_pos(byte_pos)
                        && let Some(tooltip) = part.tooltip.as_ref()
                    {
                        app.tooltip = Some(tooltip.clone());
                    }
                }
                MouseActionResult::NotHandled
            }
            MouseEventAction::HoverClearTooltip => {
                app.tooltip = None;
                MouseActionResult::NotHandled
            }
            MouseEventAction::AcceptSuggestion => {
                if let Some(Tag::Suggestion(idx)) = clicked_tag {
                    if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                        active_suggestions.set_selected_by_idx(idx);
                        active_suggestions.accept_selected_filtered_item(&mut app.buffer);
                        app.content_mode = ContentMode::Normal;
                        MouseActionResult::HandledUpdateBuffer
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::AcceptHistoryResult => {
                if let Some(Tag::HistoryResult(idx)) = clicked_tag {
                    if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                        let source = source.clone();
                        app.select_fuzzy_history_manager_mut(&source)
                            .fuzzy_search_set_idx(Some(idx));
                        app.accept_fuzzy_history_search();
                        MouseActionResult::HandledUpdateBuffer
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
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
                            MouseActionResult::HandledUpdateBuffer
                        } else {
                            MouseActionResult::NotHandled
                        }
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
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
                        MouseActionResult::HandledUpdateBuffer
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
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
                            MouseActionResult::HandledUpdateBuffer
                        } else {
                            MouseActionResult::NotHandled
                        }
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::ClickTutorialPrev => {
                app.settings.tutorial_step.prev();
                log::info!(
                    "Tutorial navigated to prev: {:?}",
                    app.settings.tutorial_step
                );
                MouseActionResult::Handled
            }
            MouseEventAction::ClickTutorialNext => {
                app.settings.tutorial_step.next();
                log::info!(
                    "Tutorial navigated to next: {:?}",
                    app.settings.tutorial_step
                );
                MouseActionResult::Handled
            }
            MouseEventAction::PromptDirAccept => {
                KeyEventAction::PromptDirAcceptEntry.run(
                    app,
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Null,
                        crossterm::event::KeyModifiers::NONE,
                    ),
                );
                MouseActionResult::HandledUpdateBuffer
            }
            MouseEventAction::PromptDirSelect => {
                if let Some(Tag::Ps1PromptCwdWidget(idx)) = clicked_tag {
                    app.content_mode = ContentMode::PromptDirSelect(idx);
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::PromptDirSelectDismiss => {
                if matches!(app.content_mode, ContentMode::PromptDirSelect(_)) {
                    app.content_mode = ContentMode::Normal;
                    MouseActionResult::Handled
                } else {
                    MouseActionResult::NotHandled
                }
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
                        MouseActionResult::HandledUpdateBuffer
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
                }
            }
            MouseEventAction::ClickPromptCopyBuffer => {
                let text = app.buffer.buffer().to_string();
                if app.copy_to_clipboard(text.as_bytes()) {
                    log::info!("Copied current buffer to clipboard via copy-buffer widget");
                    MouseActionResult::HandledUpdateBuffer
                } else {
                    MouseActionResult::NotHandled
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
                        MouseActionResult::Handled
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
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
                        MouseActionResult::Handled
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
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
                        MouseActionResult::Handled
                    } else {
                        MouseActionResult::NotHandled
                    }
                } else {
                    MouseActionResult::NotHandled
                }
            }
        }
    }
}
