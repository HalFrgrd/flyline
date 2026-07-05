use crate::app::auto_close::surround_closing_char;
use crate::app::{App, ContentMode, FlycompPromptSelection, FuzzyHistorySource};
use crate::history::HistorySearchDirection;
use crate::settings::MouseMode;
use crate::text_buffer::WordDelim;
use anyhow::Result;
use clap_complete::CompletionCandidate;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::ops::Add;
use std::sync::LazyLock;
use strum::{
    AsRefStr, EnumIter, EnumMessage, EnumString, IntoEnumIterator, IntoStaticStr, VariantArray,
};

pub(crate) type ContextExpr = super::ContextExpr<ContextVar>;
pub(crate) type ContextValues = super::ContextValues<ContextVar>;

/// A single user-facing action.  Each variant maps one-to-one to a
/// camelCase action name as exposed in the CLI (derived via strum).
/// Actions are not scoped — the binding's `ContextExpr` controls when
/// the action runs.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, EnumIter, EnumString, AsRefStr, IntoStaticStr, EnumMessage,
)]
#[strum(serialize_all = "camelCase")]
pub enum KeyEventAction {
    #[strum(message = "Toggle Yes/No choice in the flycomp prompt")]
    FlycompAskToggleChoice,
    #[strum(message = "Accept the current Yes/No choice in the flycomp prompt")]
    FlycompAskAcceptChoice,
    #[strum(message = "Accept inline history suggestion")]
    InlineSuggestionAccept,
    #[strum(message = "Temporarily dismiss the inline history suggestion")]
    InlineSuggestionDismiss,
    #[strum(message = "Move down in agent output selection")]
    AgentOutputSelectNext,
    #[strum(message = "Move up in agent output selection")]
    AgentOutputSelectPrev,
    #[strum(message = "Accept the currently selected agent output")]
    AgentOutputAcceptEntry,
    #[strum(message = "Move to the next tab completion suggestion")]
    AgentOutputNextSuggestion,
    #[strum(message = "Select the first entry in agent output selection")]
    AgentOutputSelectFirstEntry,
    #[strum(message = "Start agent mode with the current buffer again")]
    AgentOutputRunAgentMode,
    #[strum(message = "Move up in tab completion suggestions")]
    TabCompletionMoveUp,
    #[strum(message = "Move down in tab completion suggestions")]
    TabCompletionMoveDown,
    #[strum(message = "Move left in tab completion suggestions")]
    TabCompletionMoveLeft,
    #[strum(message = "Move right in tab completion suggestions")]
    TabCompletionMoveRight,
    #[strum(message = "Move one page up / one column left in tab completion suggestions")]
    TabCompletionMovePageUp,
    #[strum(message = "Move one page down / one column right in tab completion suggestions")]
    TabCompletionMovePageDown,
    #[strum(message = "Accept the currently selected suggestion")]
    TabCompletionAcceptEntry,
    #[strum(message = "Accept all currently shown suggestions")]
    TabCompletionAcceptAll,
    #[strum(message = "Move to the previous tab completion suggestion")]
    TabCompletionPrevSuggestion,
    #[strum(message = "Move to the next tab completion suggestion")]
    TabCompletionNextSuggestion,
    #[strum(message = "Scroll up through fuzzy history search results")]
    FuzzyHistorySelectPrev,
    #[strum(message = "Select the top entry in the fuzzy history search results")]
    FuzzyHistorySelectTopEntry,
    #[strum(message = "Scroll down through fuzzy history search results")]
    FuzzyHistorySelectNext,
    #[strum(message = "Scroll up one page")]
    FuzzyHistoryScrollPageUp,
    #[strum(message = "Scroll down one page")]
    FuzzyHistoryScrollPageDown,
    #[strum(message = "Accept the currently selected entry")]
    FuzzyHistoryAcceptEntry,
    #[strum(message = "Accept the currently selected entry for agent commands")]
    FuzzyHistoryAcceptAgentCommandEntry,
    #[strum(message = "Accept the current fuzzy history search suggestion for editing")]
    FuzzyHistoryAcceptAndEdit,
    #[strum(message = "Accept the current fuzzy history search suggestion and immediately run it")]
    FuzzyHistoryAcceptAndRun,
    #[strum(message = "Run the agent mode command")]
    RunAgentMode,
    #[strum(message = "Run the agent mode help command")]
    AgentModeRunHelpCommand,
    #[strum(
        message = "Submit the current command or insert a newline if the buffer is an incomplete expression"
    )]
    SubmitOrNewline,
    #[strum(message = "Insert a newline")]
    InsertNewline,
    #[strum(message = "Start tab completion")]
    RunTabCompletion,
    #[strum(message = "Toggle mouse state (Simple and Smart modes)")]
    ToggleMouse,
    #[strum(message = "Send EOF to Bash if ignoreeof is non-zero")]
    Exit,
    #[strum(message = "Cancel the current command or exit if no command is running")]
    Cancel,
    #[strum(message = "Comment out the current line and submit")]
    CommentLineSubmit,
    #[strum(message = "Start fuzzy search through command history")]
    RunFuzzyHistorySearch,
    #[strum(message = "Start fuzzy search through cancelled command history")]
    RunFuzzyCancelledHistorySearch,
    #[strum(message = "Clear the screen")]
    ClearScreen,
    #[strum(message = "Clear the text buffer")]
    ClearBuffer,
    #[strum(message = "Delete until start of line")]
    DeleteLeftUntilStartOfLine,
    #[strum(
        message = "Delete one word part to the left stopping at punctuation or path segment boundaries"
    )]
    DeleteLeftOneWordPart,
    #[strum(message = "Delete one word to the left using whitespace as delimiter")]
    DeleteLeftOneWord,
    #[strum(message = "Delete character before cursor")]
    DeleteLeft,
    #[strum(message = "Delete until end of line")]
    DeleteRightUntilEndOfLine,
    #[strum(
        message = "Delete one word part to the right stopping at punctuation or path segment boundaries"
    )]
    DeleteRightOneWordPart,
    #[strum(message = "Delete one word to the right using whitespace as delimiter")]
    DeleteRightOneWord,
    #[strum(message = "Delete character after cursor")]
    DeleteRight,
    #[strum(message = "Move cursor to start of line")]
    MoveLeftStartOfLine,
    #[strum(message = "Move one word left using whitespace as delimiter")]
    MoveLeftOneWord,
    #[strum(
        message = "Move one word part to the left, stopping at punctuation or path segment boundaries"
    )]
    MoveLeftOneWordPart,
    #[strum(message = "Move cursor left")]
    MoveLeft,
    #[strum(message = "Move cursor to end of line")]
    MoveRightEndOfLine,
    #[strum(message = "Move one word right using whitespace as delimiter")]
    MoveRightOneWord,
    #[strum(
        message = "Move one word part to the right, stopping at punctuation or path segment boundaries"
    )]
    MoveRightOneWordPart,
    #[strum(message = "Move cursor right")]
    MoveRight,
    #[strum(message = "Move cursor up one line")]
    MoveLineUp,
    #[strum(message = "Move cursor down one line")]
    MoveLineDown,
    #[strum(message = "Navigate to previous history entry")]
    PrevHistoryEntry,
    #[strum(message = "Navigate to next history entry")]
    NextHistoryEntry,
    #[strum(message = "Undo last action")]
    Undo,
    #[strum(message = "Redo last action")]
    Redo,
    #[strum(message = "Insert character")]
    InsertChar,
    #[strum(message = "Move cursor left, extending the text selection")]
    MoveLeftExtendSelection,
    #[strum(message = "Move cursor right, extending the text selection")]
    MoveRightExtendSelection,
    #[strum(message = "Move cursor up one line, extending the text selection")]
    MoveLineUpExtendSelection,
    #[strum(message = "Move cursor down one line, extending the text selection")]
    MoveLineDownExtendSelection,
    #[strum(message = "Move cursor to start of line, extending the text selection")]
    MoveLeftStartOfLineExtendSelection,
    #[strum(message = "Move cursor to end of line, extending the text selection")]
    MoveRightEndOfLineExtendSelection,
    #[strum(message = "Move one word left (whitespace delimiter), extending the text selection")]
    MoveLeftOneWordExtendSelection,
    #[strum(message = "Move one word right (whitespace delimiter), extending the text selection")]
    MoveRightOneWordExtendSelection,
    #[strum(message = "Move one word part left, extending the text selection")]
    MoveLeftOneWordPartExtendSelection,
    #[strum(message = "Move one word part right, extending the text selection")]
    MoveRightOneWordPartExtendSelection,
    #[strum(message = "Copy the current text selection to the system clipboard via OSC 52")]
    CopySelectionOsc52,
    #[strum(
        message = "Cut the current text selection: copy it to the clipboard via OSC 52 and delete it from the buffer"
    )]
    CutSelection,
    #[strum(message = "Paste from the system clipboard")]
    PasteSystemClipboard,
    #[strum(message = "Insert the last word from the previous command in history")]
    InsertLastWordFromPrevCommand,
    #[strum(message = "Select the entire command buffer")]
    SelectAll,
    #[strum(message = "Do nothing (useful for unbinding a key)")]
    Nothing,
    #[strum(
        message = "Start prompt directory selection mode, allowing navigation via the prompt's directory segments"
    )]
    StartPromptDirSelect,
    #[strum(message = "Navigate to the parent directory segment in the prompt")]
    PromptDirMoveLeft,
    #[strum(message = "Navigate to the child directory segment or exit prompt CWD edit mode")]
    PromptDirMoveRight,
    #[strum(message = "Replace the buffer with `cd <selected path>` and exit prompt CWD edit mode")]
    PromptDirAcceptEntry,
    #[strum(message = "Move selection to the leftmost directory segment in the prompt")]
    PromptDirMoveToStart,
    #[strum(message = "Move selection to the rightmost (current) directory segment in the prompt")]
    PromptDirMoveToEnd,
    #[strum(message = "Return to the normal command editing mode")]
    EscapeToNormalMode,
    #[strum(message = "Activate the leader key state")]
    SetLeaderKey,
    #[strum(message = "Deactivate the leader key state")]
    UnsetLeaderKey,
    #[strum(message = "Insert a literal string of characters", disabled)]
    InsertString(String),
}

impl KeyEventAction {
    /// The camelCase action name as exposed in the CLI.
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyEventAction::InsertString(_) => "insertString",
            _ => <&'static str>::from(self),
        }
    }

    /// Human-readable description of what the action does.  Sourced from the
    /// strum `message` attribute on each variant.
    pub fn description(&self) -> &'static str {
        match self {
            KeyEventAction::InsertString(_) => "Insert a literal string of characters",
            _ => self.get_message().unwrap_or(""),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            KeyEventAction::InsertString(s) => format!("insertString('{}')", s.escape_debug()),
            _ => self.as_str().to_string(),
        }
    }

    /// Run the action's logic against the given `App` and key event.
    pub(crate) fn run(&self, app: &mut App, key: KeyEvent) {
        match self {
            KeyEventAction::InlineSuggestionAccept => {
                if let Some((_, suf)) = &app.inline_history_suggestion {
                    let new_buffer = format!("{}{}", app.buffer.buffer(), suf);
                    app.buffer.replace_buffer(&new_buffer);
                }
            }
            KeyEventAction::InlineSuggestionDismiss => {
                app.dismissed_inline_suggestion_buffer = Some(app.buffer.buffer().to_string());
                app.inline_history_suggestion = None;
            }
            KeyEventAction::AgentOutputSelectNext => {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    selection.move_down();
                }
            }
            KeyEventAction::AgentOutputSelectPrev => {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    selection.move_up();
                }
            }
            KeyEventAction::AgentOutputAcceptEntry => {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    if let Some(cmd) = selection.selected_command() {
                        let cmd = cmd.to_string();
                        app.buffer.replace_buffer(&cmd);
                    }
                    app.content_mode = ContentMode::Normal;
                }
            }
            KeyEventAction::AgentOutputNextSuggestion => {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    selection.move_down(); // TODO: cycle through
                }
            }
            KeyEventAction::AgentOutputSelectFirstEntry => {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    selection.set_selected_by_idx(0);
                }
            }
            KeyEventAction::AgentOutputRunAgentMode => {
                if let Some((agent_cmd, buffer)) = app.resolve_agent_command(false) {
                    app.start_agent_mode(agent_cmd, &buffer);
                } else {
                    app.content_mode = ContentMode::Normal;
                }
            }
            KeyEventAction::FlycompAskToggleChoice => {
                if let ContentMode::TabCompletionAskForFlycomp {
                    ref mut selection, ..
                } = app.content_mode
                {
                    *selection = match *selection {
                        FlycompPromptSelection::Yes => FlycompPromptSelection::No,
                        FlycompPromptSelection::No => FlycompPromptSelection::DontAsk,
                        FlycompPromptSelection::DontAsk => FlycompPromptSelection::Yes,
                    };
                }
            }
            KeyEventAction::FlycompAskAcceptChoice => {
                let mode = std::mem::replace(&mut app.content_mode, ContentMode::Normal);
                if let ContentMode::TabCompletionAskForFlycomp {
                    command_word,
                    word_under_cursor,
                    selection,
                    sandbox,
                    ..
                } = mode
                {
                    match selection {
                        FlycompPromptSelection::Yes => {
                            app.run_flycomp(command_word, word_under_cursor, sandbox.is_some());
                        }
                        FlycompPromptSelection::No => {}
                        FlycompPromptSelection::DontAsk => {
                            app.settings.flycomp_blacklist.insert(command_word);
                        }
                    }
                }
            }
            KeyEventAction::TabCompletionMoveUp => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_up_arrow();
                }
            }
            KeyEventAction::TabCompletionMoveDown => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_down_arrow();
                }
            }
            KeyEventAction::TabCompletionMoveLeft => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_left_arrow();
                }
            }
            KeyEventAction::TabCompletionMoveRight => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_right_arrow();
                }
            }
            KeyEventAction::TabCompletionMovePageUp => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_page_up();
                }
            }
            KeyEventAction::TabCompletionMovePageDown => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_page_down();
                }
            }
            KeyEventAction::TabCompletionAcceptEntry => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.accept_selected_filtered_item(&mut app.buffer);
                    app.content_mode = ContentMode::Normal;
                }
            }
            KeyEventAction::TabCompletionAcceptAll => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.accept_all_filtered_items(&mut app.buffer);
                    app.content_mode = ContentMode::Normal;
                }
            }
            KeyEventAction::TabCompletionPrevSuggestion => {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_tab(true);
                }
            }
            KeyEventAction::TabCompletionNextSuggestion => {
                let no_suggestions = matches!(
                    &app.content_mode,
                    ContentMode::TabCompletion(s) if s.filtered_suggestions_len() == 0
                );
                if no_suggestions {
                    let previous_suggestions =
                        match std::mem::replace(&mut app.content_mode, ContentMode::Normal) {
                            ContentMode::TabCompletion(suggestions) => Some(suggestions),
                            _ => None,
                        };
                    app.start_tab_complete(false, previous_suggestions);
                } else if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode
                {
                    active_suggestions.on_tab(false);
                }
            }
            KeyEventAction::FuzzyHistorySelectPrev => {
                let source = match &app.content_mode {
                    ContentMode::FuzzyHistorySearch(s) => s.clone(),
                    _ => return,
                };
                app.select_fuzzy_history_manager_mut(&source)
                    .fuzzy_search_onkeypress(HistorySearchDirection::Forward);
            }
            KeyEventAction::FuzzyHistorySelectTopEntry => {
                let source = match &app.content_mode {
                    ContentMode::FuzzyHistorySearch(s) => s.clone(),
                    _ => return,
                };
                app.select_fuzzy_history_manager_mut(&source)
                    .fuzzy_search_set_idx(Some(0));
            }
            KeyEventAction::FuzzyHistorySelectNext => {
                let source = match &app.content_mode {
                    ContentMode::FuzzyHistorySearch(s) => s.clone(),
                    _ => return,
                };
                app.select_fuzzy_history_manager_mut(&source)
                    .fuzzy_search_onkeypress(HistorySearchDirection::Backward);
            }
            KeyEventAction::FuzzyHistoryScrollPageUp => {
                let source = match &app.content_mode {
                    ContentMode::FuzzyHistorySearch(s) => s.clone(),
                    _ => return,
                };
                app.select_fuzzy_history_manager_mut(&source)
                    .fuzzy_search_onkeypress(HistorySearchDirection::PageForward);
            }
            KeyEventAction::FuzzyHistoryScrollPageDown => {
                let source = match &app.content_mode {
                    ContentMode::FuzzyHistorySearch(s) => s.clone(),
                    _ => return,
                };
                app.select_fuzzy_history_manager_mut(&source)
                    .fuzzy_search_onkeypress(HistorySearchDirection::PageBackward);
            }
            KeyEventAction::FuzzyHistoryAcceptEntry => {
                app.accept_fuzzy_history_search();
            }
            KeyEventAction::FuzzyHistoryAcceptAgentCommandEntry => {
                app.accept_fuzzy_history_search_agent_command();
            }
            KeyEventAction::FuzzyHistoryAcceptAndEdit => {
                app.accept_fuzzy_history_search();
            }
            KeyEventAction::FuzzyHistoryAcceptAndRun => {
                app.accept_fuzzy_history_search();
                app.try_submit_current_buffer();
            }
            KeyEventAction::RunAgentMode => {
                if let Some((agent_cmd, buffer)) = app.resolve_agent_command(false) {
                    app.start_agent_mode(agent_cmd, &buffer);
                } else {
                    app.show_agent_mode_not_configured_error();
                }
            }
            KeyEventAction::AgentModeRunHelpCommand => match &app.content_mode {
                ContentMode::AgentError {
                    suggested_setup_command: Some(setup_cmd),
                    ..
                } => {
                    let setup_cmd = setup_cmd.clone();
                    app.content_mode = ContentMode::Normal;
                    app.buffer.replace_buffer(&setup_cmd);
                    app.on_possible_buffer_change();
                    app.try_submit_current_buffer();
                }
                ContentMode::AgentError { .. } => {
                    app.content_mode = ContentMode::Normal;
                    app.buffer.replace_buffer("flyline set-agent-mode --help");
                    app.on_possible_buffer_change();
                    app.try_submit_current_buffer();
                }
                _ => {}
            },
            KeyEventAction::SubmitOrNewline => {
                if let Some((agent_cmd, buffer)) = app.resolve_agent_command(true) {
                    app.start_agent_mode(agent_cmd, &buffer);
                } else {
                    app.try_submit_current_buffer();
                }
            }
            KeyEventAction::InsertNewline => {
                app.buffer.insert_newline();
            }
            KeyEventAction::RunTabCompletion => app.start_tab_complete(false, None),
            KeyEventAction::ToggleMouse => {
                if matches!(
                    app.settings.mouse_mode,
                    MouseMode::Simple | MouseMode::Smart
                ) {
                    log::info!("Toggling mouse state due to toggle_mouse action");
                    app.toggle_mouse_state();
                }
            }
            KeyEventAction::Exit => {
                // We shouldn't check bash_symbols::ignoreeof here.
                // Bash handles this itself.
                log::info!("KeyEventAction::Exit: setting app mode to exiting with EOF");
                app.mode = crate::app::AppRunningState::Exiting(crate::app::ExitState::EOF);
            }
            KeyEventAction::Cancel => {
                let buf = app.buffer.buffer();
                if !buf.trim().is_empty() {
                    app.settings
                        .cancelled_command_history_manager
                        .push_entry(buf.to_string());
                }
                app.mode =
                    crate::app::AppRunningState::Exiting(crate::app::ExitState::WithoutCommand);
            }
            KeyEventAction::CommentLineSubmit => {
                app.buffer.move_to_start();
                app.buffer.insert_str("#");
                app.try_submit_current_buffer();
            }
            KeyEventAction::RunFuzzyHistorySearch => {
                app.history_manager
                    .warm_fuzzy_search_cache(app.buffer.buffer(), Some(0));
                app.content_mode =
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::PastCommands);
            }
            KeyEventAction::RunFuzzyCancelledHistorySearch => {
                app.settings
                    .cancelled_command_history_manager
                    .warm_fuzzy_search_cache(app.buffer.buffer(), Some(0));
                app.content_mode =
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::CancelledCommands);
            }
            KeyEventAction::ClearScreen => {
                app.needs_screen_cleared = true;
            }
            KeyEventAction::ClearBuffer => {
                app.buffer.replace_buffer("");
            }
            KeyEventAction::DeleteLeftUntilStartOfLine => {
                if app.buffer.delete_selection() {
                    return;
                }
                app.buffer.delete_until_start_of_line();
            }
            KeyEventAction::DeleteLeftOneWordPart => {
                if app.buffer.delete_selection() {
                    return;
                }
                app.buffer.delete_one_word_left(WordDelim::FineGrained);
            }
            KeyEventAction::DeleteLeftOneWord => {
                if app.buffer.delete_selection() {
                    return;
                }
                app.buffer.delete_one_word_left(WordDelim::WhiteSpace);
            }
            KeyEventAction::DeleteLeft => {
                if app.buffer.delete_selection() {
                    return;
                }
                if app.settings.auto_close_chars {
                    // Backspace: if the char to the right of the cursor is an auto-inserted closing token
                    // paired with the char about to be deleted, remove it as well.
                    app.delete_auto_inserted_closing_if_present();
                }
                app.buffer.delete_left();
            }
            KeyEventAction::DeleteRightUntilEndOfLine => {
                if app.buffer.delete_selection() {
                    return;
                }
                app.buffer.delete_until_end_of_line();
            }
            KeyEventAction::DeleteRightOneWordPart => {
                if app.buffer.delete_selection() {
                    return;
                }
                app.buffer.delete_right_one_word(WordDelim::FineGrained);
            }
            KeyEventAction::DeleteRightOneWord => {
                if app.buffer.delete_selection() {
                    return;
                }
                app.buffer.delete_right_one_word(WordDelim::WhiteSpace);
            }
            KeyEventAction::DeleteRight => {
                if app.buffer.delete_selection() {
                    return;
                }
                app.buffer.delete_right();
            }
            KeyEventAction::MoveLeftStartOfLine => {
                app.buffer.clear_selection();
                app.buffer.move_start_of_line();
            }
            KeyEventAction::MoveLeftOneWord => {
                app.buffer.clear_selection();
                app.buffer.move_one_word_left(WordDelim::WhiteSpace);
            }
            KeyEventAction::MoveLeftOneWordPart => {
                app.buffer.clear_selection();
                app.buffer.move_one_word_left_fine_grained();
            }
            KeyEventAction::MoveLeft => {
                app.buffer.move_left();
            }
            KeyEventAction::MoveRightEndOfLine => {
                app.buffer.clear_selection();
                app.buffer.move_end_of_line();
            }
            KeyEventAction::MoveRightOneWord => {
                app.buffer.clear_selection();
                app.buffer.move_one_word_right(WordDelim::WhiteSpace);
            }
            KeyEventAction::MoveRightOneWordPart => {
                app.buffer.clear_selection();
                app.buffer.move_one_word_right_fine_grained();
            }
            KeyEventAction::MoveRight => {
                app.buffer.move_right();
            }
            KeyEventAction::MoveLineUp => {
                app.buffer.clear_selection();
                app.buffer.move_line_up();
            }
            KeyEventAction::MoveLineDown => {
                app.buffer.clear_selection();
                app.buffer.move_line_down();
            }
            KeyEventAction::PrevHistoryEntry => {
                app.buffer.clear_selection();
                app.buffer_before_history_navigation
                    .get_or_insert_with(|| app.buffer.buffer().to_string());
                if let Some(entry) = app
                    .history_manager
                    .search_in_history(app.buffer.buffer(), HistorySearchDirection::Backward)
                {
                    app.buffer.replace_buffer(&entry.command);
                }
            }
            KeyEventAction::NextHistoryEntry => {
                app.buffer.clear_selection();
                match app
                    .history_manager
                    .search_in_history(app.buffer.buffer(), HistorySearchDirection::Forward)
                {
                    Some(entry) => {
                        app.buffer.replace_buffer(&entry.command);
                    }
                    None => {
                        if let Some(original_buffer) = app.buffer_before_history_navigation.take() {
                            app.buffer.replace_buffer(&original_buffer);
                        }
                    }
                }
            }
            KeyEventAction::Undo => {
                app.buffer.clear_selection();
                app.buffer.undo();
            }
            KeyEventAction::Redo => {
                app.buffer.clear_selection();
                app.buffer.redo();
            }
            KeyEventAction::InsertChar => {
                if let KeyCode::Char(c) = key.code {
                    // If a non-empty selection is active and the character is a
                    // recognised pairing character, surround the selection with
                    // the opening and closing chars instead of replacing it.
                    if let Some(close) = surround_closing_char(c) {
                        if app.buffer.surround_selection(c, close) {
                            return;
                        }
                    }
                }
                app.buffer.delete_selection();
                if let KeyCode::Char(c) = key.code {
                    if app.settings.auto_close_chars {
                        app.handle_char_insertion(c);
                    } else {
                        app.buffer.insert_char(c);
                    }
                }
            }
            // ── Selection-extending movement actions ──────────────────────────
            KeyEventAction::MoveLeftExtendSelection => {
                app.buffer.move_left_selection();
            }
            KeyEventAction::MoveRightExtendSelection => {
                app.buffer.move_right_selection();
            }
            KeyEventAction::MoveLineUpExtendSelection => {
                app.buffer.start_selection_if_none();
                app.buffer.move_line_up();
            }
            KeyEventAction::MoveLineDownExtendSelection => {
                app.buffer.start_selection_if_none();
                app.buffer.move_line_down();
            }
            KeyEventAction::MoveLeftStartOfLineExtendSelection => {
                app.buffer.start_selection_if_none();
                app.buffer.move_start_of_line();
            }
            KeyEventAction::MoveRightEndOfLineExtendSelection => {
                app.buffer.start_selection_if_none();
                app.buffer.move_end_of_line();
            }
            KeyEventAction::MoveLeftOneWordExtendSelection => {
                app.buffer.move_left_one_word_whitespace_extend_selection();
            }
            KeyEventAction::MoveRightOneWordExtendSelection => {
                app.buffer.move_right_one_word_whitespace_extend_selection();
            }
            KeyEventAction::MoveLeftOneWordPartExtendSelection => {
                app.buffer.start_selection_if_none();
                app.buffer.move_one_word_left_fine_grained();
            }
            KeyEventAction::MoveRightOneWordPartExtendSelection => {
                app.buffer.start_selection_if_none();
                app.buffer.move_one_word_right_fine_grained();
            }
            KeyEventAction::CopySelectionOsc52 => {
                let text_to_copy = if app.right_click_popup_pos.is_some() {
                    app.right_click_copy_target
                        .as_ref()
                        .map(|target| match target {
                            crate::app::RightClickCopyTarget::Selection(s) => s.clone(),
                            crate::app::RightClickCopyTarget::Buffer(s) => s.clone(),
                            crate::app::RightClickCopyTarget::HistoryEntry(s) => s.clone(),
                            crate::app::RightClickCopyTarget::Cwd(s) => s.clone(),
                        })
                } else {
                    app.buffer.selected_text()
                };

                if let Some(text) = text_to_copy {
                    match crossterm::execute!(
                        std::io::stdout(),
                        crossterm::clipboard::CopyToClipboard::to_clipboard_from(text)
                    ) {
                        Ok(()) => {
                            log::info!("Copied selection to clipboard via OSC 52");
                        }
                        Err(e) => {
                            log::error!("Failed to copy to clipboard via OSC 52: {}", e);
                        }
                    }
                    app.buffer.clear_selection();
                }
                app.right_click_copy_target = None;
            }
            KeyEventAction::CutSelection => {
                let target_to_cut = if app.right_click_popup_pos.is_some() {
                    app.right_click_copy_target.clone()
                } else {
                    app.buffer
                        .selected_text()
                        .map(crate::app::RightClickCopyTarget::Selection)
                };

                if let Some(target) = target_to_cut {
                    let text = match &target {
                        crate::app::RightClickCopyTarget::Selection(s) => s,
                        crate::app::RightClickCopyTarget::Buffer(s) => s,
                        crate::app::RightClickCopyTarget::HistoryEntry(s) => s,
                        crate::app::RightClickCopyTarget::Cwd(s) => s,
                    };
                    match crossterm::execute!(
                        std::io::stdout(),
                        crossterm::clipboard::CopyToClipboard::to_clipboard_from(text.clone())
                    ) {
                        Ok(()) => {
                            log::info!("Cut selection to clipboard via OSC 52");
                        }
                        Err(e) => {
                            log::error!("Failed to copy to clipboard via OSC 52: {}", e);
                        }
                    }
                    match target {
                        crate::app::RightClickCopyTarget::Selection(_) => {
                            app.buffer.delete_selection();
                        }
                        crate::app::RightClickCopyTarget::Buffer(_) => {
                            app.buffer.replace_buffer("");
                            app.on_possible_buffer_change();
                        }
                        crate::app::RightClickCopyTarget::HistoryEntry(_) => {
                            // History is read-only.
                        }
                        crate::app::RightClickCopyTarget::Cwd(_) => {
                            // CWD is read-only.
                        }
                    }
                }
                app.right_click_copy_target = None;
            }
            KeyEventAction::SelectAll => {
                let len = app.buffer.buffer().len();
                app.buffer.set_selection_range(0..len, false);
            }
            // https://github.com/theimpostor/osc
            // Normally the terminal emulator handles Ctrl+V
            // But if it doesn't it gives us an opportunity use OSC52 request system clibpoard!
            KeyEventAction::PasteSystemClipboard => {
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::clipboard::RequestClipboardContents::clipboard()
                );
            }
            KeyEventAction::InsertLastWordFromPrevCommand => {
                app.buffer.clear_selection();

                // Get the last word of the history command we are currently looking at
                let last_word_of_current_history_cmd = app
                    .history_manager
                    .get_last_word_insert_command()
                    .and_then(|cmd| crate::history::get_last_word(cmd));

                // Find if the last word of the current history command is touching the cursor
                let target_sub = last_word_of_current_history_cmd
                    .as_ref()
                    .and_then(|last_word| app.buffer.is_cursor_on_s(last_word));

                let is_continuation = target_sub.is_some();

                if !is_continuation {
                    app.history_manager.last_word_insert_reset();
                }

                // Move to the previous command with non-empty words
                if let Some(cmd) = app.history_manager.last_word_insert_move_prev() {
                    if let Some(w) = crate::history::get_last_word(cmd) {
                        if let Some(sub) = &target_sub {
                            let _ = app.buffer.replace_word_under_cursor(&w, sub);
                        } else {
                            app.buffer.insert_str(&w);
                        }
                    }
                }
            }
            KeyEventAction::Nothing => {}
            KeyEventAction::StartPromptDirSelect => {
                if app.prompt_manager.cwd_display_segment_count() > 0 {
                    app.content_mode = ContentMode::PromptDirSelect(0);
                }
            }
            KeyEventAction::PromptDirMoveLeft => {
                if let ContentMode::PromptDirSelect(ref mut index) = app.content_mode {
                    let max_index = app
                        .prompt_manager
                        .cwd_display_segment_count()
                        .saturating_sub(1);
                    if *index < max_index {
                        *index += 1;
                    }
                }
            }
            KeyEventAction::PromptDirMoveRight => match app.content_mode {
                ContentMode::PromptDirSelect(0) => {
                    app.content_mode = ContentMode::Normal;
                }
                ContentMode::PromptDirSelect(ref mut index) => {
                    *index -= 1;
                }
                _ => {}
            },
            KeyEventAction::PromptDirAcceptEntry => {
                if let ContentMode::PromptDirSelect(index) = app.content_mode {
                    if let Some(path) = app.prompt_manager.cwd_path_for_index(index) {
                        // Single-quote the path to handle spaces and most shell metacharacters.
                        // Embedded single quotes are escaped with the standard '\'' idiom.
                        // This is safe for CWD paths returned by the OS (no NUL bytes).
                        let quoted = format!("'{}'", path.replace('\'', r"'\''"));
                        app.buffer.replace_buffer(&format!("cd {}", quoted));
                    }
                    app.content_mode = ContentMode::Normal;
                    app.on_possible_buffer_change();
                    app.try_submit_current_buffer();
                }
            }
            KeyEventAction::PromptDirMoveToStart => {
                if let ContentMode::PromptDirSelect(ref mut index) = app.content_mode {
                    *index = app
                        .prompt_manager
                        .cwd_display_segment_count()
                        .saturating_sub(1);
                }
            }
            KeyEventAction::PromptDirMoveToEnd => {
                if let ContentMode::PromptDirSelect(ref mut index) = app.content_mode {
                    *index = 0;
                }
            }
            KeyEventAction::EscapeToNormalMode => {
                // Capture the word-under-cursor when dismissing tab completion, so we don't
                // auto-suggest on the same word the user just dismissed.
                match &app.content_mode {
                    ContentMode::TabCompletion(active_suggestions) => {
                        app.dismissed_tab_completion_wuc =
                            Some(active_suggestions.word_under_cursor.s.to_string());
                    }
                    ContentMode::TabCompletionWaiting { wuc_substring, .. } => {
                        app.dismissed_tab_completion_wuc = Some(wuc_substring.s.to_string());
                    }
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts) => {
                        app.dismissed_agent_prompts_buffer = Some(app.buffer.buffer().to_string());
                    }
                    _ => {
                        // Not tab completion; just clear the dismissed field.
                        app.dismissed_tab_completion_wuc = None;
                    }
                }

                app.buffer.clear_selection();
                app.content_mode = ContentMode::Normal;
            }
            KeyEventAction::SetLeaderKey => {
                app.leader_key_active_at = Some(std::time::Instant::now());
            }
            KeyEventAction::UnsetLeaderKey => {
                app.leader_key_active_at = None;
            }
            KeyEventAction::InsertString(s) => {
                app.buffer.delete_selection();
                app.buffer.insert_str(s);
            }
        }
    }
}

// `TryFrom<&str>` for `KeyEventAction` is automatically derived from `EnumString` via
// strum.  The error type is `strum::ParseError`; callers wishing for a richer
// error message should map it themselves (e.g. via anyhow context).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEventMatch {
    Exact(KeyEvent),
    AnyCharAndMods(KeyModifiers),
}

impl From<KeyCode> for KeyEventMatch {
    fn from(code: KeyCode) -> Self {
        KeyEventMatch::Exact(KeyEvent::new(code, KeyModifiers::empty()))
    }
}

impl From<KeyEvent> for KeyEventMatch {
    fn from(event: KeyEvent) -> Self {
        KeyEventMatch::Exact(event)
    }
}

/// Add a set of [`KeyModifiers`] to a [`KeyEventMatch`], OR-ing them into the
/// match's existing modifier set.  Combined with [`From<KeyCode>`] for
/// [`KeyEventMatch`], this lets binding definitions read like keyboard
/// chords:
///
/// ```ignore
/// let kem: KeyEventMatch = KeyModifiers::CONTROL + KeyCode::Char('s').into();
/// let kem: KeyEventMatch = KeyCode::Enter.into() + KeyModifiers::ALT;
/// ```
///
/// Direct `KeyModifiers + KeyCode` is not supported because both types are
/// foreign to this crate (orphan rule); convert one side to
/// [`KeyEventMatch`] first.  For "any char" matches with modifiers, write
/// `KeyEventMatch::AnyCharAndMods(KeyModifiers::SHIFT)` directly.
impl Add<KeyEventMatch> for KeyModifiers {
    type Output = KeyEventMatch;

    fn add(self, rhs: KeyEventMatch) -> KeyEventMatch {
        rhs + self
    }
}

impl Add<KeyModifiers> for KeyEventMatch {
    type Output = KeyEventMatch;

    fn add(self, rhs: KeyModifiers) -> KeyEventMatch {
        match self {
            KeyEventMatch::Exact(ev) => {
                KeyEventMatch::Exact(KeyEvent::new(ev.code, ev.modifiers | rhs))
            }
            KeyEventMatch::AnyCharAndMods(mods) => KeyEventMatch::AnyCharAndMods(mods | rhs),
        }
    }
}

impl Add<KeyEventMatch> for KeyCode {
    type Output = KeyEventMatch;

    fn add(self, rhs: KeyEventMatch) -> KeyEventMatch {
        KeyEventMatch::from(self) + rhs
    }
}

impl Add<KeyCode> for KeyEventMatch {
    type Output = KeyEventMatch;

    fn add(self, rhs: KeyCode) -> KeyEventMatch {
        self + KeyEventMatch::from(rhs)
    }
}

impl Add<KeyEventMatch> for KeyEventMatch {
    type Output = KeyEventMatch;

    fn add(self, rhs: KeyEventMatch) -> KeyEventMatch {
        match (self, rhs) {
            (KeyEventMatch::Exact(a), KeyEventMatch::Exact(b)) => {
                // Pick the non-Null code; otherwise prefer the right-hand code.
                let code = match (a.code, b.code) {
                    (KeyCode::Null, c) => c,
                    (c, KeyCode::Null) => c,
                    (_, c) => c,
                };
                KeyEventMatch::Exact(KeyEvent::new(code, a.modifiers | b.modifiers))
            }
            (KeyEventMatch::AnyCharAndMods(a), KeyEventMatch::AnyCharAndMods(b)) => {
                KeyEventMatch::AnyCharAndMods(a | b)
            }
            (KeyEventMatch::AnyCharAndMods(a), KeyEventMatch::Exact(b))
            | (KeyEventMatch::Exact(b), KeyEventMatch::AnyCharAndMods(a)) => {
                KeyEventMatch::Exact(KeyEvent::new(b.code, a | b.modifiers))
            }
        }
    }
}

impl TryFrom<&str> for KeyEventMatch {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let mut modifiers = KeyModifiers::empty();
        let mut parts = s.split('+').collect::<Vec<_>>();
        let key_part = parts
            .pop()
            .ok_or_else(|| anyhow::anyhow!("Invalid key event string: '{}'", s))?;
        for mod_part in parts {
            modifiers |= parse_single_modifier(mod_part)?;
        }
        if key_part.trim().eq_ignore_ascii_case("anychar") {
            return Ok(KeyEventMatch::AnyCharAndMods(modifiers));
        }
        let code = parse_single_keycode(key_part)?;
        Ok(KeyEventMatch::Exact(KeyEvent::new(code, modifiers)))
    }
}

/// A key code remapping or modifier remapping registered with `flyline key remap`.
///
/// Keys can only be remapped to keys, and modifiers can only be remapped to
/// modifiers.  When a key event arrives it is first transformed by
/// [`apply_remappings`] before being matched against bindings.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyRemap {
    /// Remap one non-modifier key to another (e.g. Tab → z).
    Key { from: KeyCode, to: KeyCode },
    /// Remap one modifier bit to another (e.g. Alt → Ctrl).
    Modifier {
        from: KeyModifiers,
        to: KeyModifiers,
    },
    /// Remap a key event with modifiers to another key event.
    Event { from: KeyEvent, to: KeyEvent },
}

/// Parse a single key-code name (no modifiers) into a [`KeyCode`].
fn parse_single_keycode(s: &str) -> Result<KeyCode> {
    use crossterm::event::{MediaKeyCode, ModifierKeyCode};
    let s = s.trim();
    if s.len() == 1 {
        // Convert upper case ASCII letters to lower case since terminals typically don't distinguish them in key codes.
        let c = s.chars().next().unwrap();
        let lower_case = c.to_ascii_lowercase();
        return Ok(KeyCode::Char(lower_case));
    }
    let lower = s.to_lowercase();
    // F-key: "f1" … "f255"
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            return Ok(KeyCode::F(n));
        }
    }
    // Media key: "media:play", "media:pause", …
    if let Some(rest) = lower.strip_prefix("media:") {
        let mk = match rest {
            "play" => MediaKeyCode::Play,
            "pause" => MediaKeyCode::Pause,
            "playpause" | "play_pause" => MediaKeyCode::PlayPause,
            "reverse" => MediaKeyCode::Reverse,
            "stop" => MediaKeyCode::Stop,
            "fastforward" | "fast_forward" => MediaKeyCode::FastForward,
            "rewind" => MediaKeyCode::Rewind,
            "tracknext" | "track_next" | "nexttrack" | "next_track" => MediaKeyCode::TrackNext,
            "trackprevious" | "track_previous" | "prevtrack" | "prev_track" => {
                MediaKeyCode::TrackPrevious
            }
            "record" => MediaKeyCode::Record,
            "lowervolume" | "lower_volume" | "volumedown" | "volume_down" => {
                MediaKeyCode::LowerVolume
            }
            "raisevolume" | "raise_volume" | "volumeup" | "volume_up" => MediaKeyCode::RaiseVolume,
            "mutevolume" | "mute_volume" | "mute" => MediaKeyCode::MuteVolume,
            other => return Err(anyhow::anyhow!("Unknown media key: '{}'", other)),
        };
        return Ok(KeyCode::Media(mk));
    }
    // Standalone modifier key: "modifier:leftshift", "modifier:rightctrl", …
    if let Some(rest) = lower.strip_prefix("modifier:") {
        let mk = match rest {
            "leftshift" | "left_shift" => ModifierKeyCode::LeftShift,
            "leftcontrol" | "left_control" | "leftctrl" | "left_ctrl" => {
                ModifierKeyCode::LeftControl
            }
            "leftalt" | "left_alt" => ModifierKeyCode::LeftAlt,
            "leftsuper" | "left_super" => ModifierKeyCode::LeftSuper,
            "lefthyper" | "left_hyper" => ModifierKeyCode::LeftHyper,
            "leftmeta" | "left_meta" => ModifierKeyCode::LeftMeta,
            "rightshift" | "right_shift" => ModifierKeyCode::RightShift,
            "rightcontrol" | "right_control" | "rightctrl" | "right_ctrl" => {
                ModifierKeyCode::RightControl
            }
            "rightalt" | "right_alt" => ModifierKeyCode::RightAlt,
            "rightsuper" | "right_super" => ModifierKeyCode::RightSuper,
            "righthyper" | "right_hyper" => ModifierKeyCode::RightHyper,
            "rightmeta" | "right_meta" => ModifierKeyCode::RightMeta,
            "isolevel3shift" | "iso_level3_shift" => ModifierKeyCode::IsoLevel3Shift,
            "isolevel5shift" | "iso_level5_shift" => ModifierKeyCode::IsoLevel5Shift,
            other => return Err(anyhow::anyhow!("Unknown modifier key: '{}'", other)),
        };
        return Ok(KeyCode::Modifier(mk));
    }
    match lower.as_str() {
        "enter" | "ret" | "return" => Ok(KeyCode::Enter),
        "backspace" | "bkspc" | "bs" => Ok(KeyCode::Backspace),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" | "pgup" => Ok(KeyCode::PageUp),
        "pagedown" | "pgdown" | "pgdn" => Ok(KeyCode::PageDown),
        "tab" => Ok(KeyCode::Tab),
        "backtab" => Ok(KeyCode::BackTab),
        "delete" | "del" => Ok(KeyCode::Delete),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "space" | "spc" => Ok(KeyCode::Char(' ')),
        "null" => Ok(KeyCode::Null),
        "capslock" | "caps_lock" | "caps" => Ok(KeyCode::CapsLock),
        "scrolllock" | "scroll_lock" => Ok(KeyCode::ScrollLock),
        "numlock" | "num_lock" => Ok(KeyCode::NumLock),
        "printscreen" | "print_screen" | "prtscn" => Ok(KeyCode::PrintScreen),
        "pause" => Ok(KeyCode::Pause),
        "menu" => Ok(KeyCode::Menu),
        "keypadbegin" | "keypad_begin" => Ok(KeyCode::KeypadBegin),
        other => Err(anyhow::anyhow!("Unknown key code: '{}'", other)),
    }
}

static MODS_TO_EQUIV_NAMES: LazyLock<HashMap<KeyModifiers, &'static [&'static str]>> =
    LazyLock::new(|| {
        HashMap::from([
            (KeyModifiers::CONTROL, &["ctrl", "control"] as &[&str]),
            (KeyModifiers::SHIFT, &["shift"] as &[&str]),
            (KeyModifiers::ALT, &["alt", "option"] as &[&str]),
            (KeyModifiers::META, &["meta"] as &[&str]),
            (
                KeyModifiers::SUPER,
                &["super", "cmd", "command", "gui", "win"] as &[&str],
            ),
            (KeyModifiers::HYPER, &["hyper"] as &[&str]),
        ])
    });

/// Parse a single modifier name into a single-bit [`KeyModifiers`] value.
fn parse_single_modifier(s: &str) -> Result<KeyModifiers> {
    let lower = s.trim().to_lowercase();
    MODS_TO_EQUIV_NAMES
        .iter()
        .find_map(|(modifier, names)| names.contains(&lower.as_str()).then_some(*modifier))
        .ok_or_else(|| anyhow::anyhow!("Unknown modifier: '{}'", s))
}

/// Parse and validate a remap pair (from, to).  Modifiers may only be remapped
/// to modifiers; keys may only be remapped to keys.
pub fn try_parse_remap(from: &str, to: &str) -> Result<KeyRemap> {
    if let Ok(KeyEventMatch::Exact(from_event)) = KeyEventMatch::try_from(from) {
        if let Ok(KeyEventMatch::Exact(to_event)) = KeyEventMatch::try_from(to) {
            if !from_event.modifiers.is_empty() || !to_event.modifiers.is_empty() {
                return Ok(KeyRemap::Event {
                    from: from_event,
                    to: to_event,
                });
            }
        }
    }

    let from_mod = parse_single_modifier(from);
    let to_mod = parse_single_modifier(to);
    match (&from_mod, &to_mod) {
        (Ok(f), Ok(t)) => return Ok(KeyRemap::Modifier { from: *f, to: *t }),
        (Ok(_), Err(_)) => {
            return Err(anyhow::anyhow!(
                "'{}' is a modifier but '{}' is not; modifiers can only be remapped to modifiers",
                from,
                to
            ));
        }
        (Err(_), Ok(_)) => {
            return Err(anyhow::anyhow!(
                "'{}' is not a modifier but '{}' is; keys can only be remapped to keys",
                from,
                to
            ));
        }
        (Err(_), Err(_)) => {}
    }
    let from_key = parse_single_keycode(from)
        .map_err(|_| anyhow::anyhow!("'{}' is not a recognised key or modifier name", from))?;
    let to_key = parse_single_keycode(to)
        .map_err(|_| anyhow::anyhow!("'{}' is not a recognised key or modifier name", to))?;
    Ok(KeyRemap::Key {
        from: from_key,
        to: to_key,
    })
}

/// Apply all remappings to a raw key event and return the logical key event
/// that should be matched against bindings.
///
/// All modifier remaps are applied simultaneously (based on the original
/// modifier bits) so that swapping two modifiers works correctly.
pub fn apply_remappings(key: KeyEvent, remappings: &[KeyRemap]) -> KeyEvent {
    if remappings.is_empty() {
        return key;
    }

    // 1. Key event remappings take precedence
    for remap in remappings {
        if let KeyRemap::Event { from, to } = remap {
            if key.code == from.code && key.modifiers == from.modifiers {
                return KeyEvent {
                    code: to.code,
                    modifiers: to.modifiers,
                    ..key
                };
            }
        }
    }

    // 2. Modifier remaps are applied simultaneously from the original modifier set.
    let original_modifiers = key.modifiers;
    let mut new_modifiers = KeyModifiers::empty();
    for &bit in &[
        KeyModifiers::CONTROL,
        KeyModifiers::SHIFT,
        KeyModifiers::ALT,
        KeyModifiers::META,
        KeyModifiers::SUPER,
    ] {
        if !original_modifiers.contains(bit) {
            continue;
        }
        let remapped = remappings.iter().find_map(|r| {
            if let KeyRemap::Modifier { from, to } = r {
                if *from == bit { Some(*to) } else { None }
            } else {
                None
            }
        });
        new_modifiers |= remapped.unwrap_or(bit);
    }

    // Key-code remap: at most one remap applies.
    let new_code = remappings
        .iter()
        .find_map(|r| {
            if let KeyRemap::Key { from, to } = r {
                if *from == key.code { Some(*to) } else { None }
            } else {
                None
            }
        })
        .unwrap_or(key.code);

    KeyEvent::new(new_code, new_modifiers)
}

#[derive(Debug, Clone)]
pub struct Binding {
    key_events: Vec<KeyEventMatch>,
    context: ContextExpr,
    actions: Vec<KeyEventAction>,
}

impl Binding {
    /// Create a binding from a list of [`KeyEventMatch`] values, a context
    /// expression, and a slice of actions.  This is infallible: parsing happens at
    /// compile time via the typed `KeyCode` / `KeyModifiers` constructors.
    pub fn new(
        key_events: &[KeyEventMatch],
        context: ContextExpr,
        actions: &[KeyEventAction],
    ) -> Self {
        let mut unique_events = Vec::new();
        for event in key_events {
            if !unique_events.contains(event) {
                unique_events.push(event.clone());
            }
        }
        Self {
            key_events: unique_events,
            context,
            actions: actions.to_vec(),
        }
    }

    /// Parse a user-provided binding from the CLI form
    /// `<KEY> <CONTEXT_EXPR>=<ACTION>`.
    pub fn try_new_from_strs(key_event: &str, context_and_action: &str) -> Result<Self> {
        let (context_str, action_str) = context_and_action.rsplit_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid context and action format: '{}'. Expected 'context=action'",
                context_and_action
            )
        })?;
        let action_str = action_str.trim();
        let actions: Vec<KeyEventAction> = action_str
            .split('+')
            .map(|s| {
                let s = s.trim();
                if let Some(stripped) = s.strip_prefix("insertString(") {
                    if !stripped.ends_with(')') {
                        return Err(anyhow::anyhow!(
                            "Invalid insertString syntax: '{}'. Expected 'insertString(value)'",
                            s
                        ));
                    }
                    let inner = &stripped[..stripped.len() - 1];
                    let inner_trimmed = if (inner.starts_with('"') && inner.ends_with('"'))
                        || (inner.starts_with('\'') && inner.ends_with('\''))
                    {
                        if inner.len() >= 2 {
                            &inner[1..inner.len() - 1]
                        } else {
                            inner
                        }
                    } else {
                        inner
                    };
                    let unescaped = unescape_string(inner_trimmed);
                    Ok(KeyEventAction::InsertString(unescaped))
                } else {
                    KeyEventAction::try_from(s)
                        .map_err(|_| anyhow::anyhow!("Unknown action: '{}'", s))
                }
            })
            .collect::<Result<Vec<_>>>()?;

        if actions.is_empty() {
            return Err(anyhow::anyhow!("No actions specified"));
        }

        Ok(Self::new(
            &[KeyEventMatch::try_from(key_event)?],
            ContextExpr::try_from(context_str.trim())?,
            &actions,
        ))
    }

    pub fn matches(&self, key: KeyEvent) -> bool {
        self.key_events.iter().any(|k| match k {
            KeyEventMatch::Exact(action_binding) => {
                keycodes_match(action_binding.code, key.code)
                    && key.modifiers == action_binding.modifiers
            }
            KeyEventMatch::AnyCharAndMods(mods) => {
                matches!(key.code, KeyCode::Char(_)) && key.modifiers == *mods
            }
        })
    }
}

fn unescape_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                match next_c {
                    'n' => {
                        result.push('\n');
                        chars.next();
                    }
                    't' => {
                        result.push('\t');
                        chars.next();
                    }
                    'r' => {
                        result.push('\r');
                        chars.next();
                    }
                    'e' => {
                        result.push('\x1b');
                        chars.next();
                    }
                    'x' => {
                        chars.next(); // consume 'x'
                        let mut hex = String::new();
                        if let Some(h1) = chars.next() {
                            hex.push(h1);
                        }
                        if let Some(h2) = chars.next() {
                            hex.push(h2);
                        }
                        if let Ok(val) = u8::from_str_radix(&hex, 16) {
                            result.push(val as char);
                        } else {
                            result.push('\\');
                            result.push('x');
                            result.extend(hex.chars());
                        }
                    }
                    _ => {
                        result.push(next_c);
                        chars.next();
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn keycodes_match(a: KeyCode, b: KeyCode) -> bool {
    match (a, b) {
        (KeyCode::Char(a), KeyCode::Char(b)) => a.eq_ignore_ascii_case(&b),
        _ => a == b,
    }
}

/// Return the list of terminal-equivalent [`KeyEventMatch`] values that
/// should all map to the same logical binding as `kem`.
///
/// The first entry is always `kem` itself; additional entries are sibling
/// chords commonly produced by different terminal emulators or input modes
/// for the same physical key.
///
/// # Expansion rules
///
/// | Input            | Expands to                                          |
/// |------------------|-----------------------------------------------------|
/// | `Enter`          | `Enter`, `Ctrl+j`                                   |
/// | `Shift+Tab`      | `Shift+Tab`, `BackTab`, `Shift+BackTab`             |
/// | `BackTab`        | `BackTab`, `Shift+Tab`, `Shift+BackTab`             |
/// | `Alt+Left`       | `Alt+Left`, `Alt+b`, `Meta+Left`, `Meta+b`          |
/// | `Alt+Right`      | `Alt+Right`, `Alt+f`, `Meta+Right`, `Meta+f`        |
/// | `Meta+Left`      | same four-way word-left group                       |
/// | `Alt+b` / `Meta+b` | same four-way word-left group                     |
/// | `Meta+Right`     | same four-way word-right group                      |
/// | `Alt+f` / `Meta+f` | same four-way word-right group                    |
/// | `Alt+Delete`     | `Alt+Delete`, `Meta+Delete`, `Alt+d`, `Meta+d`      |
/// | `Alt+X` (other)  | `Alt+X`, `Meta+X`                                   |
/// | `Home`           | `Home`, `Ctrl+a`                                    |
/// | `End`            | `End`, `Ctrl+e`                                     |
/// | anything else    | unchanged                                           |
pub fn expand_variations_one(kem: KeyEventMatch) -> Vec<KeyEventMatch> {
    use KeyCode::*;
    use KeyModifiers as M;

    // Helpers to build chord values concisely.
    let exact = |code: KeyCode, mods: KeyModifiers| -> KeyEventMatch {
        KeyEventMatch::Exact(KeyEvent::new(code, mods))
    };

    if let KeyEventMatch::Exact(ev) = kem {
        let mods = ev.modifiers;
        match (ev.code, mods) {
            // Enter ↔ Ctrl+J (ASCII LF)
            (Enter, m) if m.is_empty() => {
                return vec![exact(Enter, M::empty()), exact(Char('j'), M::CONTROL)];
            }
            // Word-left group: Alt+Left / Alt+b / Meta+Left / Meta+b
            (Left, m) if m == M::ALT => {
                return vec![
                    exact(Left, M::ALT),
                    exact(Char('b'), M::ALT),
                    exact(Left, M::META),
                    exact(Char('b'), M::META),
                ];
            }
            (Left, m) if m == M::META => {
                return vec![
                    exact(Left, M::META),
                    exact(Char('b'), M::META),
                    exact(Left, M::ALT),
                    exact(Char('b'), M::ALT),
                ];
            }
            (Char('b'), m) if m == M::ALT => {
                return vec![
                    exact(Char('b'), M::ALT),
                    exact(Left, M::ALT),
                    exact(Char('b'), M::META),
                    exact(Left, M::META),
                ];
            }
            (Char('b'), m) if m == M::META => {
                return vec![
                    exact(Char('b'), M::META),
                    exact(Left, M::META),
                    exact(Char('b'), M::ALT),
                    exact(Left, M::ALT),
                ];
            }
            // Word-right group: Alt+Right / Alt+f / Meta+Right / Meta+f
            (Right, m) if m == M::ALT => {
                return vec![
                    exact(Right, M::ALT),
                    exact(Char('f'), M::ALT),
                    exact(Right, M::META),
                    exact(Char('f'), M::META),
                ];
            }
            (Right, m) if m == M::META => {
                return vec![
                    exact(Right, M::META),
                    exact(Char('f'), M::META),
                    exact(Right, M::ALT),
                    exact(Char('f'), M::ALT),
                ];
            }
            (Char('f'), m) if m == M::ALT => {
                return vec![
                    exact(Char('f'), M::ALT),
                    exact(Right, M::ALT),
                    exact(Char('f'), M::META),
                    exact(Right, M::META),
                ];
            }
            (Char('f'), m) if m == M::META => {
                return vec![
                    exact(Char('f'), M::META),
                    exact(Right, M::META),
                    exact(Char('f'), M::ALT),
                    exact(Right, M::ALT),
                ];
            }
            // Alt+Delete / Meta+Delete / Alt+d / Meta+d are all word-delete-right.
            (Delete, m) if m == M::ALT => {
                return vec![
                    exact(Delete, M::ALT),
                    exact(Delete, M::META),
                    exact(Char('d'), M::ALT),
                    exact(Char('d'), M::META),
                ];
            }
            // Alt+Enter / Meta+Enter (Alt/Meta terminal equivalence).
            (Enter, m) if m == M::ALT => {
                return vec![exact(Enter, M::ALT), exact(Enter, M::META)];
            }
            // Alt+Backspace / Meta+Backspace.
            (Backspace, m) if m == M::ALT => {
                return vec![exact(Backspace, M::ALT), exact(Backspace, M::META)];
            }
            // Alt+d / Meta+d (word-delete-right shortcut).
            (Char('d'), m) if m == M::ALT => {
                return vec![exact(Char('d'), M::ALT), exact(Char('d'), M::META)];
            }
            // Alt+w / Meta+w (used as a Ctrl+w alias for word-delete-left).
            (Char('w'), m) if m == M::ALT => {
                return vec![exact(Char('w'), M::ALT), exact(Char('w'), M::META)];
            }
            // Home → also Ctrl+a (Emacs move-beginning-of-line).
            (Home, m) if m.is_empty() => {
                return vec![exact(Home, M::empty()), exact(Char('a'), M::CONTROL)];
            }
            // End → also Ctrl+e (Emacs move-end-of-line).
            (End, m) if m.is_empty() => {
                return vec![exact(End, M::empty()), exact(Char('e'), M::CONTROL)];
            }
            // BackTab ↔ Shift+Tab ↔ Shift+BackTab.
            (BackTab, m) if m.is_empty() => {
                return vec![
                    exact(BackTab, M::empty()),
                    exact(Tab, M::SHIFT),
                    exact(BackTab, M::SHIFT),
                ];
            }
            (Tab, m) if m == M::SHIFT => {
                return vec![
                    exact(BackTab, M::empty()),
                    exact(Tab, M::SHIFT),
                    exact(BackTab, M::SHIFT),
                ];
            }
            // Fallback for any other key with ALT or META
            (code, m) if m == M::ALT => {
                return vec![exact(code, M::ALT), exact(code, M::META)];
            }
            (code, m) if m == M::META => {
                return vec![exact(code, M::META), exact(code, M::ALT)];
            }
            _ => {}
        }
    }

    vec![kem]
}

/// Expand a list of [`KeyEventMatch`] values to include their common terminal
/// equivalents.
///
/// Returns a [`Vec<KeyEventMatch>`] that derefs to `&[KeyEventMatch]`, so it
/// can be passed directly as `&expand_variations![...]` to [`Binding::new`].
///
/// # Example
///
/// ```ignore
/// // expand_variations![KeyCode::Enter.into()]   →  [Enter, Ctrl+j]
/// // expand_variations![KeyModifiers::ALT + KeyCode::Enter.into()]
/// //                                              →  [Alt+Enter, Meta+Enter]
/// ```
macro_rules! expand_variations {
    [$($kem:expr),+ $(,)?] => {{
        let mut v: Vec<$crate::app::actions::KeyEventMatch> = Vec::new();
        $(v.extend($crate::app::actions::expand_variations_one($kem));)+
        v
    }};
}

#[cfg(test)]
mod expand_variations_tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_expand_variations_enter() {
        assert_eq!(
            expand_variations![KeyCode::Enter.into()],
            vec![
                KeyEventMatch::Exact(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())),
                KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
            ]
        );
    }

    #[test]
    fn test_expand_variations_alt_to_meta() {
        let v = expand_variations![KeyModifiers::ALT + KeyCode::Backspace.into()];
        assert_eq!(
            v,
            vec![
                KeyEventMatch::Exact(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
                KeyEventMatch::Exact(KeyEvent::new(KeyCode::Backspace, KeyModifiers::META)),
            ]
        );
    }

    #[test]
    fn test_expand_variations_alt_to_meta_generic() {
        let v = expand_variations![KeyModifiers::ALT + KeyCode::Char('.').into()];
        assert_eq!(
            v,
            vec![
                KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::ALT)),
                KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::META)),
            ]
        );
    }
}

/// Tab-completion for the `<context_expr>=<action>` argument of
/// `flyline key bind`.
///
/// If the input contains `=`, completes the action name to the right of the
/// last `=`; otherwise, completes the (possibly partial) `+`-separated
/// context variable to the right of the last `+`.
pub fn possible_context_action_completions(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy().to_string();
    if let Some(eq_idx) = current.rfind('=') {
        let prefix = &current[..=eq_idx];
        let action_part = &current[eq_idx + 1..];
        let last_plus_idx = action_part.rfind('+');
        let (action_prefix, last_action_str) = if let Some(idx) = last_plus_idx {
            (&action_part[..=idx], &action_part[idx + 1..])
        } else {
            ("", action_part)
        };

        // If the last action part is a fully matching action
        if let Ok(action) = KeyEventAction::try_from(last_action_str.trim()) {
            return vec![
                // Option 1: done adding actions (appends a space suffix)
                CompletionCandidate::new(format!(
                    "{}PREFIX_DELIM{}{}",
                    prefix,
                    action_prefix,
                    action.as_str()
                ))
                .help(action.get_message().map(clap::builder::StyledStr::from)),
                // Option 2: want to chain another action (appends '+' and suppresses space)
                CompletionCandidate::new(format!(
                    "{}PREFIX_DELIM{}{}+NO_SUFFIX",
                    prefix,
                    action_prefix,
                    action.as_str()
                ))
                .help(action.get_message().map(clap::builder::StyledStr::from)),
            ];
        }

        // Otherwise, complete the partial action name
        let action_lower = last_action_str.trim().to_lowercase();
        let mut candidates: Vec<CompletionCandidate> = KeyEventAction::iter()
            .filter_map(|a| {
                let s = a.as_str();
                if s.to_lowercase().contains(&action_lower) {
                    Some(
                        CompletionCandidate::new(format!(
                            "{}PREFIX_DELIM{}{}NO_SUFFIX",
                            prefix, action_prefix, s
                        ))
                        .help(a.get_message().map(clap::builder::StyledStr::from)),
                    )
                } else {
                    None
                }
            })
            .collect();

        if "insertstring".contains(&action_lower) {
            candidates.push(
                CompletionCandidate::new(format!(
                    "{}PREFIX_DELIM{}{}NO_SUFFIX",
                    prefix, action_prefix, "insertString(value)"
                ))
                .help(Some(clap::builder::StyledStr::from(
                    "Insert a literal string of characters",
                ))),
            );
        }
        return candidates;
    }
    // Completing context variables.  Determine the prefix already typed
    // (everything up to and including the last `+`) and the partial
    // variable name being typed.
    let (prefix, partial) = if let Some(idx) = current.rfind('+') {
        (&current[..idx + 1], &current[idx + 1..])
    } else {
        ("", current.as_str())
    };
    let partial_clean = partial.trim_start_matches('!');
    let partial_lower = partial_clean.to_lowercase();
    let neg_prefix = if partial.starts_with('!') { "!" } else { "" };
    ContextVar::VARIANTS
        .iter()
        .flat_map(|v| {
            let name = v.as_str();
            let description: Option<&str> = v.get_message();
            if !name.to_lowercase().contains(&partial_lower) {
                return Vec::new();
            }

            let extras: &[&str] = if name.eq_ignore_ascii_case(partial_clean) {
                &["=", "+"]
            } else {
                &[""]
            };

            extras
                .iter()
                .map(|extra| {
                    CompletionCandidate::new(format!(
                        "{}PREFIX_DELIM{}{}{}NO_SUFFIX",
                        prefix, neg_prefix, name, extra
                    ))
                    .help(description.map(clap::builder::StyledStr::from))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn key_sequence_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();

    let keys: Vec<String> = vec![
        KeyCode::Enter,
        KeyCode::Backspace,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Delete,
        KeyCode::Insert,
        KeyCode::Esc,
        KeyCode::CapsLock,
        KeyCode::ScrollLock,
        KeyCode::NumLock,
        KeyCode::PrintScreen,
        KeyCode::Pause,
        KeyCode::Menu,
        KeyCode::KeypadBegin,
        KeyCode::Null,
        // KeyCode::Char(c),
        // KeyCode::F(n),
        // KeyCode::Media(mk),
    ]
    .into_iter()
    .map(display_keycode)
    .chain(std::iter::once("AnyChar".to_string()))
    .chain(std::iter::once("Space".to_string()))
    .chain((97u8..123).map(|c| display_keycode(KeyCode::Char(c as char))))
    .collect();

    let parts: Vec<&str> = current.split('+').collect();

    let (used, current) = if parts.len() > 1 {
        (&parts[..parts.len() - 1], parts.last().unwrap())
    } else {
        (&[][..], &parts[0])
    };
    let current_lower = current.to_lowercase();
    let mut out = vec![];

    for (_m, mod_equivs) in MODS_TO_EQUIV_NAMES.iter() {
        log::info!(
            "Checking mod_equivs {:?} against used mods {:?}",
            mod_equivs,
            used
        );
        let used_mod = mod_equivs
            .iter()
            .any(|equiv| used.iter().any(|u| u.eq_ignore_ascii_case(equiv)));
        if !used_mod {
            for equiv in *mod_equivs {
                if equiv.to_lowercase().starts_with(&current_lower) {
                    let prefix = parts[..parts.len() - 1].join("+");
                    let prefix = if prefix.is_empty() {
                        "".into()
                    } else {
                        prefix + "+"
                    };
                    out.push(CompletionCandidate::new(format!(
                        "{}{}+NO_SUFFIX",
                        prefix,
                        capitalize_first(equiv)
                    )));
                }
            }
        }
    }

    for k in keys {
        if k.to_lowercase().starts_with(&current_lower) {
            let prefix = parts[..parts.len() - 1].join("+");
            let prefix = if prefix.is_empty() {
                "".into()
            } else {
                prefix + "+"
            };
            out.push(CompletionCandidate::new(format!("{}{}", prefix, k)));
        }
    }

    out
}

/// Completer for key remapping args.
/// Suggests standard key sequences as well as standalone capitalized modifier names.
pub fn remap_key_completer(current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let current_str = current.to_string_lossy();
    let current_lower = current_str.to_lowercase();

    // 1. Get the normal key sequence completions (e.g. including "Ctrl+", "Alt+", etc.)
    let mut out = key_sequence_completer(current);

    // 2. Also support remapping standalone modifiers (e.g. "Ctrl", "Alt", "Shift", etc.).
    for (_, mod_equivs) in MODS_TO_EQUIV_NAMES.iter() {
        for equiv in *mod_equivs {
            if equiv.to_lowercase().starts_with(&current_lower) {
                out.push(CompletionCandidate::new(capitalize_first(equiv)));
            }
        }
    }

    out
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// MacOs: https://stackoverflow.com/questions/12827888/what-is-the-representation-of-the-mac-command-key-in-the-terminal
/// MacOs command keyboard shortcuts are not sent to terminal apps by default.
/// They are often captured by the terminal emulator itself for various commands
/// Try `ghostty +list-keybinds --default` on ghostty. Most
///
/// META: this is similar to Alt. How are they different?
/// SUPER: Windows key or Mac Command key
/// HYPER: Often as as result of pressing Ctrl + Shift + Alt + Windows/Command key. rarely used.
///
/// https://en.wikipedia.org/wiki/Table_of_keyboard_shortcuts#Command_line_shortcuts
///
/// Meta vs Alt:
/// On iterm2, there is a setting in Profiles->Keys->Left option key.
/// Choices are Normal or Set high bit (not recommended) or Esc+.
/// Set high bit gives you a warning: "You have chosen to have an option key as Meta. This is
/// useful for backward compatibility with old applications. The "Esc+" option is recommended for most users"
/// In text_buffer.rs, I check if either of them are set for maximal compatibility.
/// From highest priority to lowest
pub static DEFAULT_BINDINGS: LazyLock<Vec<Binding>> = LazyLock::new(|| {
    use KeyCode as KC;
    use KeyModifiers as M;
    vec![
        // --- TabCompletionAskForFlycomp bindings ---
        Binding::new(
            &expand_variations![
                KC::Left.into(),
                KC::Right.into(),
                KC::Up.into(),
                KC::Down.into()
            ],
            ContextVar::TabCompletionAskForFlycomp.into(),
            &[KeyEventAction::FlycompAskToggleChoice],
        ),
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::TabCompletionAskForFlycomp.into(),
            &[KeyEventAction::FlycompAskToggleChoice],
        ),
        Binding::new(
            &[KC::Enter.into()],
            ContextVar::TabCompletionAskForFlycomp.into(),
            &[KeyEventAction::FlycompAskAcceptChoice],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::TabCompletionAskForFlycomp.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[
                M::CONTROL + KC::Char('c').into(),
                M::META + KC::Char('c').into(),
                M::SUPER + KC::Char('c').into(),
            ],
            ContextVar::TabCompletionAskForFlycomp.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        // --- TabCompletionRunningFlycomp bindings ---
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::TabCompletionRunningFlycomp.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[
                M::CONTROL + KC::Char('c').into(),
                M::META + KC::Char('c').into(),
                M::SUPER + KC::Char('c').into(),
            ],
            ContextVar::TabCompletionRunningFlycomp.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        // --- TabCompletionFlycompResult bindings ---
        Binding::new(
            &[KC::Esc.into(), KC::Enter.into(), KC::Backspace.into()],
            ContextVar::TabCompletionFlycompResult.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &expand_variations![
                KC::Left.into(),
                KC::Right.into(),
                KC::Up.into(),
                KC::Down.into()
            ],
            ContextVar::TabCompletionFlycompResult.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[
                M::CONTROL + KC::Char('c').into(),
                M::META + KC::Char('c').into(),
                M::SUPER + KC::Char('c').into(),
            ],
            ContextVar::TabCompletionFlycompResult.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KeyEventMatch::AnyCharAndMods(M::empty())],
            ContextVar::TabCompletionFlycompResult.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Down.into()],
            ContextVar::AgentOutputSelection.into(),
            &[KeyEventAction::AgentOutputSelectNext],
        ),
        Binding::new(
            &[KC::Up.into()],
            ContextVar::AgentOutputSelection.into(),
            &[KeyEventAction::AgentOutputSelectPrev],
        ),
        Binding::new(
            &[KC::Up.into()],
            !ContextVar::UserTriggeredSuggestions + ContextVar::TabCompletionEntrySelected,
            &[KeyEventAction::TabCompletionMoveUp],
        ),
        Binding::new(
            &[KC::Up.into()],
            ContextVar::UserTriggeredSuggestions.into(),
            &[KeyEventAction::TabCompletionMoveUp],
        ),
        Binding::new(
            &[KC::Down.into()],
            ContextVar::TabCompletionAvailable.into(),
            &[KeyEventAction::TabCompletionMoveDown],
        ),
        Binding::new(
            &[KC::Left.into()],
            ContextVar::TabCompletionMultiColAvailable.into(),
            &[KeyEventAction::TabCompletionMoveLeft],
        ),
        Binding::new(
            &[KC::Right.into()],
            ContextVar::TabCompletionMultiColAvailable.into(),
            &[KeyEventAction::TabCompletionMoveRight],
        ),
        Binding::new(
            &[KC::PageUp.into()],
            ContextVar::TabCompletionAvailable.into(),
            &[KeyEventAction::TabCompletionMovePageUp],
        ),
        Binding::new(
            &[KC::PageDown.into()],
            ContextVar::TabCompletionAvailable.into(),
            &[KeyEventAction::TabCompletionMovePageDown],
        ),
        Binding::new(
            &[KC::Up.into()],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::FuzzyHistorySelectPrev],
        ),
        Binding::new(
            &[KC::Down.into(), M::CONTROL + KC::Char('s').into()],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::FuzzyHistorySelectNext],
        ),
        Binding::new(
            &[KC::PageUp.into()],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::FuzzyHistoryScrollPageUp],
        ),
        Binding::new(
            &[KC::PageDown.into()],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::FuzzyHistoryScrollPageDown],
        ),
        Binding::new(
            &expand_variations![
                M::CONTROL + KC::Char('r').into(),
                M::ALT + KC::Char('r').into(),
            ],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::EscapeToNormalMode], // Stop fuzzy history search if active, otherwise escape to normal mode
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::BufferHasAgentModePrefix + ContextVar::EditingBufferMode,
            &[KeyEventAction::RunAgentMode],
        ),
        Binding::new(
            &expand_variations![M::ALT + KC::Enter.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::RunAgentMode],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::FuzzyHistorySearchAgentCommands.into(),
            &[KeyEventAction::FuzzyHistoryAcceptAgentCommandEntry],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::FuzzyHistorySearchNormalCommands.into(),
            &[KeyEventAction::FuzzyHistoryAcceptEntry],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::FuzzyHistorySearchCancelledCommands.into(),
            &[KeyEventAction::FuzzyHistoryAcceptEntry],
        ),
        Binding::new(
            &expand_variations![M::CONTROL + KC::Enter.into(), M::SUPER + KC::Enter.into()],
            ContextVar::TabCompletionAvailable.into(),
            &[KeyEventAction::TabCompletionAcceptAll],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::TabCompletionEntrySelected.into(),
            &[KeyEventAction::TabCompletionAcceptEntry],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::AgentModeError.into(),
            &[KeyEventAction::AgentModeRunHelpCommand],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::AgentOutputNoneSelected.into(),
            &[KeyEventAction::AgentOutputRunAgentMode],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::AgentOutputSelection.into(),
            &[KeyEventAction::AgentOutputAcceptEntry],
        ),
        // PromptCwdEdit Enter must appear before the Normal Enter binding.
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::PromptDirAcceptEntry],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::MultilineBuffer + ContextVar::CursorAtEndTrimmed,
            &[KeyEventAction::SubmitOrNewline],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::MultilineBuffer.into(),
            &[KeyEventAction::InsertNewline],
        ),
        Binding::new(
            &expand_variations![KC::Enter.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::SubmitOrNewline],
        ),
        Binding::new(
            &expand_variations![KC::BackTab.into()],
            ContextVar::TabCompletionAvailable.into(),
            &[KeyEventAction::TabCompletionPrevSuggestion],
        ),
        // Scoped Esc bindings must appear before the Normal Esc binding.
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::FuzzyHistorySearchNoneSelected.into(),
            &[KeyEventAction::FuzzyHistorySelectTopEntry],
        ),
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::FuzzyHistorySelectNext],
        ),
        Binding::new(
            &expand_variations![KC::BackTab.into()],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::FuzzyHistorySelectPrev],
        ),
        Binding::new(
            &expand_variations![KC::BackTab.into()],
            ContextVar::AgentOutputSelection.into(),
            &[KeyEventAction::AgentOutputSelectPrev],
        ),
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::AgentOutputNoneSelected.into(),
            &[KeyEventAction::AgentOutputSelectFirstEntry],
        ),
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::AgentOutputSelection.into(),
            &[KeyEventAction::AgentOutputNextSuggestion],
        ),
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::TabCompletionOneResult.into(),
            &[KeyEventAction::TabCompletionAcceptEntry],
        ),
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::TabCompletionAvailable.into(),
            &[KeyEventAction::TabCompletionNextSuggestion],
        ),
        Binding::new(
            &[KC::Tab.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::RunTabCompletion],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::AgentModeError.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::AgentModeWaiting.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::AgentOutputSelection.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::FuzzyHistorySearch.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::TabCompletionAvailable.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::TabCompletion.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::TabCompletionWaiting.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        // TextSelected Esc must appear before the Default Esc binding so that
        // pressing Esc with a selection active clears the selection rather
        // than toggling the mouse.
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::TextSelected.into(),
            &[KeyEventAction::EscapeToNormalMode],
        ),
        Binding::new(
            &[KC::Esc.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::ToggleMouse],
        ),
        // Ctrl+D / Super+D (Cmd+D on macOS): delete character under cursor when
        // the buffer is non-empty.  The BufferIsEmpty+Ctrl+D binding below takes
        // precedence on an empty buffer and sends EOF to Bash.
        Binding::new(
            &[
                M::CONTROL + KC::Char('d').into(),
                M::SUPER + KC::Char('d').into(),
            ],
            (!ContextVar::BufferIsEmpty).into(),
            &[KeyEventAction::DeleteRight],
        ),
        Binding::new(
            &[M::CONTROL + KC::Char('d').into()],
            ContextVar::BufferIsEmpty.into(),
            &[KeyEventAction::Exit],
        ),
        // TextSelected Ctrl+x cuts the selection to the clipboard.
        Binding::new(
            &[
                M::CONTROL + KC::Char('x').into(),
                M::META + KC::Char('x').into(),
                M::SUPER + KC::Char('x').into(),
            ],
            ContextVar::TextSelected.into(),
            &[KeyEventAction::CutSelection],
        ),
        // TextSelected Ctrl+c must appear before the Default Ctrl+c binding
        // so that copying the selection takes precedence over cancelling.
        Binding::new(
            &[
                M::CONTROL + KC::Char('c').into(),
                M::META + KC::Char('c').into(),
                M::SUPER + KC::Char('c').into(),
            ],
            ContextVar::TextSelected.into(),
            &[KeyEventAction::CopySelectionOsc52],
        ),
        Binding::new(
            &[
                M::CONTROL + KC::Char('c').into(),
                M::META + KC::Char('c').into(),
                M::SUPER + KC::Char('c').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::Cancel],
        ),
        // Paste from system clipboard on Ctrl+v / Cmd+v
        Binding::new(
            &[
                M::CONTROL + KC::Char('v').into(),
                M::META + KC::Char('v').into(),
                M::SUPER + KC::Char('v').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::PasteSystemClipboard],
        ),
        // Insert last word from previous history command on Alt+.
        Binding::new(
            &expand_variations![M::ALT + KC::Char('.').into(),],
            ContextVar::Always.into(),
            &[KeyEventAction::InsertLastWordFromPrevCommand],
        ),
        Binding::new(
            // Ctrl+/ (shows as Ctrl+7) - comment out and execute
            &[
                M::CONTROL + KC::Char('/').into(),
                M::META + KC::Char('/').into(),
                M::SUPER + KC::Char('/').into(),
                M::CONTROL + KC::Char('7').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::CommentLineSubmit],
        ),
        Binding::new(
            &[M::CONTROL + KC::Char('r').into()],
            ContextVar::Always.into(),
            &[KeyEventAction::RunFuzzyHistorySearch],
        ),
        Binding::new(
            &expand_variations![M::ALT + KC::Char('r').into(),],
            ContextVar::Always.into(),
            &[KeyEventAction::RunFuzzyCancelledHistorySearch],
        ),
        Binding::new(
            &[M::CONTROL + KC::Char('l').into()],
            ContextVar::Always.into(),
            &[KeyEventAction::ClearScreen],
        ),
        Binding::new(
            &[
                M::SUPER + KC::Backspace.into(),
                M::CONTROL + KC::Char('u').into(),
                (M::CONTROL | M::SHIFT) + KC::Backspace.into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteLeftUntilStartOfLine],
        ),
        Binding::new(
            &expand_variations![M::ALT + KC::Backspace.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteLeftOneWordPart],
        ),
        Binding::new(
            &expand_variations![
                M::CONTROL + KC::Backspace.into(),
                M::CONTROL + KC::Char('h').into(),
                M::ALT + KC::Char('w').into(),
                M::CONTROL + KC::Char('w').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteLeftOneWord],
        ),
        Binding::new(
            &[KC::Backspace.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteLeft],
        ),
        Binding::new(
            &[
                M::SUPER + KC::Delete.into(),
                (M::CONTROL | M::SHIFT) + KC::Delete.into(),
                M::CONTROL + KC::Char('k').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteRightUntilEndOfLine],
        ),
        Binding::new(
            &expand_variations![M::ALT + KC::Delete.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteRightOneWordPart],
        ),
        Binding::new(
            &expand_variations![M::CONTROL + KC::Delete.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteRightOneWord],
        ),
        Binding::new(
            &[KC::Delete.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::DeleteRight],
        ),
        Binding::new(
            &expand_variations![KC::Home.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::PromptDirMoveToStart],
        ),
        Binding::new(
            &expand_variations![KC::End.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::PromptDirMoveToEnd],
        ),
        Binding::new(
            &expand_variations![M::CONTROL + KC::Left.into(), M::ALT + KC::Left.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::PromptDirMoveLeft],
        ),
        Binding::new(
            &expand_variations![M::CONTROL + KC::Right.into(), M::ALT + KC::Right.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::PromptDirMoveRight],
        ),
        Binding::new(
            &[
                (M::CONTROL | M::SHIFT) + KC::Char('a').into(),
                (M::SUPER | M::SHIFT) + KC::Char('a').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::SelectAll],
        ),
        Binding::new(
            &[
                M::SHIFT + KC::Home.into(),
                (M::SUPER | M::SHIFT) + KC::Left.into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeftStartOfLineExtendSelection],
        ),
        Binding::new(
            &expand_variations![
                KC::Home.into(),
                M::SUPER + KC::Left.into(),
                M::CONTROL + KC::Char('a').into(),
                M::SUPER + KC::Char('a').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeftStartOfLine],
        ),
        Binding::new(
            &[(M::CONTROL | M::SHIFT) + KC::Left.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeftOneWordExtendSelection],
        ),
        Binding::new(
            &[M::CONTROL + KC::Left.into()], // Emacs-style whitespace word-left
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeftOneWord],
        ),
        Binding::new(
            &[
                (M::ALT | M::SHIFT) + KC::Left.into(),
                (M::META | M::SHIFT) + KC::Left.into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeftOneWordPartExtendSelection],
        ),
        Binding::new(
            // Fine-grained word-left (stops at punctuation / path boundaries)
            &expand_variations![M::ALT + KC::Left.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeftOneWordPart],
        ),
        Binding::new(
            &[KC::Left.into()],
            (ContextVar::CursorAtStart + !ContextVar::PromptDirSelection).into(),
            &[KeyEventAction::StartPromptDirSelect],
        ),
        // PromptCwdEdit Left must appear before the Normal Left binding.
        Binding::new(
            &[KC::Left.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::PromptDirMoveLeft],
        ),
        Binding::new(
            &[M::SHIFT + KC::Left.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeftExtendSelection],
        ),
        Binding::new(
            &[KC::Left.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLeft],
        ),
        Binding::new(
            &expand_variations![KC::Right.into(), KC::End.into()],
            (ContextVar::InlineSuggestionAvailable
                + ContextVar::CursorAtEnd
                + !ContextVar::TabCompletionMultiColAvailable)
                .into(),
            &[KeyEventAction::InlineSuggestionAccept],
        ),
        Binding::new(
            &[
                M::SHIFT + KC::End.into(),
                (M::SUPER | M::SHIFT) + KC::Right.into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRightEndOfLineExtendSelection],
        ),
        Binding::new(
            &expand_variations![
                KC::End.into(),
                M::SUPER + KC::Right.into(),
                M::CONTROL + KC::Char('e').into(),
                M::SUPER + KC::Char('e').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRightEndOfLine],
        ),
        Binding::new(
            &[(M::CONTROL | M::SHIFT) + KC::Right.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRightOneWordExtendSelection],
        ),
        Binding::new(
            &[M::CONTROL + KC::Right.into()], // Emacs-style whitespace word-right
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRightOneWord],
        ),
        Binding::new(
            &[
                (M::ALT | M::SHIFT) + KC::Right.into(),
                (M::META | M::SHIFT) + KC::Right.into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRightOneWordPartExtendSelection],
        ),
        Binding::new(
            // Fine-grained word-right (stops at punctuation / path boundaries)
            &expand_variations![M::ALT + KC::Right.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRightOneWordPart],
        ),
        // PromptCwdEdit Right must appear before the Normal Right binding.
        Binding::new(
            &[KC::Right.into()],
            ContextVar::PromptDirSelection.into(),
            &[KeyEventAction::PromptDirMoveRight],
        ),
        Binding::new(
            &[M::SHIFT + KC::Right.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRightExtendSelection],
        ),
        Binding::new(
            &[KC::Right.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveRight],
        ),
        Binding::new(
            &[M::SHIFT + KC::Up.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLineUpExtendSelection],
        ),
        Binding::new(
            &[KC::Up.into()],
            (!ContextVar::CursorOnFirstLine).into(),
            &[KeyEventAction::MoveLineUp],
        ),
        Binding::new(
            &[KC::Up.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::PrevHistoryEntry],
        ),
        Binding::new(
            &[M::SHIFT + KC::Down.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::MoveLineDownExtendSelection],
        ),
        Binding::new(
            &[KC::Down.into()],
            (!ContextVar::CursorOnFinalLine).into(),
            &[KeyEventAction::MoveLineDown],
        ),
        Binding::new(
            &[KC::Down.into()],
            ContextVar::Always.into(),
            &[KeyEventAction::NextHistoryEntry],
        ),
        Binding::new(
            &[
                M::CONTROL + KC::Char('y').into(),
                M::SUPER + KC::Char('y').into(),
                (M::CONTROL | M::SHIFT) + KC::Char('z').into(),
                (M::SUPER | M::SHIFT) + KC::Char('z').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::Redo],
        ),
        Binding::new(
            &[
                M::CONTROL + KC::Char('z').into(),
                M::SUPER + KC::Char('z').into(),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::Undo],
        ),
        Binding::new(
            &[
                KeyEventMatch::AnyCharAndMods(M::empty()),
                KeyEventMatch::AnyCharAndMods(M::SHIFT),
            ],
            ContextVar::Always.into(),
            &[KeyEventAction::InsertChar],
        ),
    ]
});

/// Return the display name for a [`KeyCode`].
fn display_keycode(code: KeyCode) -> String {
    match code {
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::CapsLock => "CapsLock".to_string(),
        KeyCode::ScrollLock => "ScrollLock".to_string(),
        KeyCode::NumLock => "NumLock".to_string(),
        KeyCode::PrintScreen => "PrintScreen".to_string(),
        KeyCode::Pause => "Pause".to_string(),
        KeyCode::Menu => "Menu".to_string(),
        KeyCode::KeypadBegin => "KeypadBegin".to_string(),
        KeyCode::Null => "Null".to_string(),
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("F{}", n),
        KeyCode::Media(mk) => format!("Media:{:?}", mk),
        KeyCode::Modifier(mk) => format!("Modifier:{:?}", mk),
    }
}

fn display_key_event(key: KeyEvent) -> String {
    KeyEventMatch::Exact(key).display()
}

/// Return the display name for a single modifier bit.
const fn display_modifier_bit(bit: KeyModifiers) -> &'static str {
    if bit.contains(KeyModifiers::CONTROL) {
        "Ctrl"
    } else if bit.contains(KeyModifiers::ALT) {
        "Alt"
    } else if bit.contains(KeyModifiers::META) {
        "Meta"
    } else if bit.contains(KeyModifiers::SHIFT) {
        "Shift"
    } else if bit.contains(KeyModifiers::SUPER) {
        "Super"
    } else if bit.contains(KeyModifiers::HYPER) {
        "Hyper"
    } else {
        "Unknown"
    }
}

/// Given a logical modifier bit and the current remappings, return what the
/// user must physically press to produce that logical modifier.
///
/// Returns `Ok(display_name)` when accessible, `Err(logical_name)` when
/// inaccessible (the bit is consumed by a remap and nothing maps back to it).
fn inverse_modifier_display(bit: KeyModifiers, remappings: &[KeyRemap]) -> Result<String, String> {
    // Something maps TO this bit → that something is what the user presses.
    for remap in remappings {
        if let KeyRemap::Modifier { from, to } = remap {
            if *to == bit {
                return Ok(display_modifier_bit(*from).to_string());
            }
        }
    }
    // This bit is the source of a remap → pressing it produces something else.
    for remap in remappings {
        if let KeyRemap::Modifier { from, to: _ } = remap {
            if *from == bit {
                return Err(display_modifier_bit(bit).to_string());
            }
        }
    }
    Ok(display_modifier_bit(bit).to_string())
}

/// Given a logical key code and the current remappings, return what the user
/// must physically press to produce that logical key code.
///
/// Returns `Ok(display_name)` when accessible, `Err(logical_name)` when
/// inaccessible.
fn inverse_keycode_display(code: KeyCode, remappings: &[KeyRemap]) -> Result<String, String> {
    // Something maps TO this code → that something is what the user presses.
    for remap in remappings {
        if let KeyRemap::Key { from, to } = remap {
            if *to == code {
                return Ok(display_keycode(*from));
            }
        }
    }
    // This code is the source of a remap → pressing it produces something else.
    for remap in remappings {
        if let KeyRemap::Key { from, to: _ } = remap {
            if *from == code {
                return Err(display_keycode(code));
            }
        }
    }
    Ok(display_keycode(code))
}

impl KeyEventMatch {
    fn display(&self) -> String {
        let display_modifiers = |mods: KeyModifiers| -> Vec<String> {
            [
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::META,
                KeyModifiers::SHIFT,
                KeyModifiers::SUPER,
            ]
            .iter()
            .filter(|&&bit| mods.contains(bit))
            .map(|&bit| display_modifier_bit(bit).to_string())
            .collect()
        };

        match self {
            KeyEventMatch::Exact(ke) => {
                let mut parts = display_modifiers(ke.modifiers);
                parts.push(display_keycode(ke.code));
                parts.join("+")
            }
            KeyEventMatch::AnyCharAndMods(mods) => {
                let mut parts = display_modifiers(*mods);
                parts.push("AnyChar".to_string());
                parts.join("+")
            }
        }
    }

    /// Display this key event match, applying the inverse of the given
    /// remappings so the output shows what the user physically needs to press.
    ///
    /// If a key or modifier required by the binding is not reachable via any
    /// physical key (because it has been remapped away), it is shown as
    /// `[INACCESSIBLE: X]`.
    fn display_with_remapping(&self, remappings: &[KeyRemap]) -> String {
        if remappings.is_empty() {
            return self.display();
        }

        // Build the display strings for all active modifier bits in `mods`,
        // pushing each result (or its [INACCESSIBLE:…] marker) into `parts`.
        let push_modifiers = |mods: KeyModifiers, parts: &mut Vec<String>| {
            for &bit in &[
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::META,
                KeyModifiers::SHIFT,
                KeyModifiers::SUPER,
            ] {
                if !mods.contains(bit) {
                    continue;
                }
                match inverse_modifier_display(bit, remappings) {
                    Ok(name) => parts.push(name),
                    Err(name) => parts.push(format!("[INACCESSIBLE: {}]", name)),
                }
            }
        };

        match self {
            KeyEventMatch::Exact(ke) => {
                let has_event_remap_to_ke = remappings.iter().any(|remap| {
                    if let KeyRemap::Event { to, .. } = remap {
                        to.code == ke.code && to.modifiers == ke.modifiers
                    } else {
                        false
                    }
                });

                if has_event_remap_to_ke {
                    let mut physical_events = Vec::new();

                    // 1. Check if the logical key itself is mapped away.
                    let mut is_mapped_away = false;
                    for remap in remappings {
                        match remap {
                            KeyRemap::Event { from, .. } => {
                                if from.code == ke.code && from.modifiers == ke.modifiers {
                                    is_mapped_away = true;
                                    break;
                                }
                            }
                            KeyRemap::Key { from, .. } => {
                                if *from == ke.code {
                                    is_mapped_away = true;
                                    break;
                                }
                            }
                            KeyRemap::Modifier { from, .. } => {
                                if ke.modifiers.contains(*from) {
                                    is_mapped_away = true;
                                    break;
                                }
                            }
                        }
                    }

                    if !is_mapped_away {
                        physical_events.push(display_key_event(*ke));
                    }

                    // 2. Find any Event remappings that target this logical event.
                    for remap in remappings {
                        if let KeyRemap::Event { from, to } = remap {
                            if to.code == ke.code && to.modifiers == ke.modifiers {
                                physical_events.push(display_key_event(*from));
                            }
                        }
                    }

                    if !physical_events.is_empty() {
                        return physical_events.join(" or ");
                    }
                }

                let mut parts: Vec<String> = Vec::new();
                push_modifiers(ke.modifiers, &mut parts);
                match inverse_keycode_display(ke.code, remappings) {
                    Ok(name) => parts.push(name),
                    Err(name) => parts.push(format!("[INACCESSIBLE: {}]", name)),
                }
                parts.join("+")
            }
            // AnyChar bindings: apply inverse modifier display per modifier set.
            KeyEventMatch::AnyCharAndMods(mods) => {
                let mut parts: Vec<String> = Vec::new();
                push_modifiers(*mods, &mut parts);
                parts.push("AnyChar".to_string());
                parts.join("+")
            }
        }
    }
}

/// ANSI escape sequence: blinking white text on red background.
const ANSI_BLINK_WHITE_ON_RED: &str = "\x1b[5;37;41m";
/// ANSI escape sequence: reset all attributes.
const ANSI_RESET: &str = "\x1b[0m";

fn key_event_a_shadows_b(a: &KeyEventMatch, b: &KeyEventMatch) -> bool {
    match (a, b) {
        // Under strict matching, key events only shadow each other if their modifiers match exactly.
        (KeyEventMatch::Exact(ea), KeyEventMatch::Exact(eb)) => {
            ea.code == eb.code && ea.modifiers == eb.modifiers
        }
        (KeyEventMatch::AnyCharAndMods(mods_a), KeyEventMatch::AnyCharAndMods(mods_b)) => {
            mods_a == mods_b
        }
        // AnyCharAndMods overlaps with an Exact char pattern, but not with a
        // non-char key (e.g. Enter, Tab) since AnyCharAndMods only fires on chars.
        (KeyEventMatch::AnyCharAndMods(mods), KeyEventMatch::Exact(e)) => {
            e.modifiers == *mods && matches!(e.code, KeyCode::Char(_))
        }
        (KeyEventMatch::Exact(e), KeyEventMatch::AnyCharAndMods(mods)) => {
            matches!(e.code, KeyCode::Char(_)) && e.modifiers == *mods
        }
    }
}

/// A key-binding conflict: a lower-priority binding that can never be reached
/// because a higher-priority binding always fires first.
struct Conflict {
    /// Human-readable display of the key event that is inaccessible.
    key_display: String,
    /// `<context>=<action>` of the inaccessible (shadowed) binding.
    inaccessible_action: String,
    /// `<context>=<action>` of the higher-priority binding that shadows it.
    shadowing_action: String,
}

/// Returns `true` if a higher-priority binding `higher` can shadow a
/// lower-priority binding `lower` for the same key, making `lower` unreachable.
///
/// With AND-only context expressions, `higher` shadows `lower` iff every
/// literal in `higher.context` also appears in `lower.context`.  Equivalently,
/// `higher`'s context is implied by `lower`'s context: any state in which
/// `lower` would fire also satisfies `higher`, so `higher` always wins.
fn context_shadows(higher: &Binding, lower: &Binding) -> bool {
    higher
        .context
        .literals
        .iter()
        .all(|h_lit| lower.context.literals.iter().any(|l_lit| l_lit == h_lit))
}

/// Scan the combined set of bindings (user overrides + defaults) and return
/// every case where a lower-priority binding is permanently shadowed.
fn detect_binding_conflicts(user_bindings: &[Binding], remappings: &[KeyRemap]) -> Vec<Conflict> {
    // Collect all bindings highest-priority-first, mirroring `handle_key_event`.
    let all_bindings: Vec<&Binding> = user_bindings
        .iter()
        .rev()
        .chain(DEFAULT_BINDINGS.iter())
        .collect();

    let mut conflicts = Vec::new();

    for (idx_b, binding_b) in all_bindings.iter().enumerate() {
        for kem_b in &binding_b.key_events {
            // Check whether any higher-priority binding shadows this key event.
            'find_shadow: for binding_a in &all_bindings[..idx_b] {
                if !context_shadows(binding_a, binding_b) {
                    continue;
                }
                for kem_a in &binding_a.key_events {
                    if key_event_a_shadows_b(kem_a, kem_b) {
                        let actions_b_str = binding_b
                            .actions
                            .iter()
                            .map(|a| a.display_name())
                            .collect::<Vec<_>>()
                            .join("+");
                        let actions_a_str = binding_a
                            .actions
                            .iter()
                            .map(|a| a.display_name())
                            .collect::<Vec<_>>()
                            .join("+");
                        conflicts.push(Conflict {
                            key_display: kem_b.display_with_remapping(remappings),
                            inaccessible_action: format!(
                                "{}={}",
                                binding_b.context.display(),
                                actions_b_str
                            ),
                            shadowing_action: format!(
                                "{}={}",
                                binding_a.context.display(),
                                actions_a_str
                            ),
                        });
                        break 'find_shadow;
                    }
                }
            }
        }
    }

    conflicts
}

/// Print all keybindings as a formatted table to stdout, ordered from lowest
/// to highest priority.  User-defined bindings appear above the defaults and
/// are marked with `*` in the rightmost column.
pub fn print_bindings_table(
    user_bindings: &[Binding],
    filter_key: Option<&str>,
    remappings: &[KeyRemap],
) {
    use crate::table::{TableAccum, TableOptions, render_table_constrained};
    use ratatui::layout::Constraint;

    let filter_event: Option<KeyEvent> =
        filter_key.and_then(|k| match KeyEventMatch::try_from(k) {
            Ok(KeyEventMatch::Exact(ev)) => Some(ev),
            _ => {
                eprintln!("Warning: could not parse key sequence '{}'", k);
                None
            }
        });

    struct Row {
        keys: String,
        context: String,
        action_name: String,
        description: String,
    }

    let binding_to_row = |binding: &Binding| -> Row {
        let keys = binding
            .key_events
            .iter()
            .map(|k| k.display_with_remapping(remappings))
            .collect::<Vec<_>>()
            .join(", ");
        let action_name = binding
            .actions
            .iter()
            .map(|a| a.display_name())
            .collect::<Vec<_>>()
            .join("+");
        let description = binding
            .actions
            .iter()
            .map(|a| a.description())
            .collect::<Vec<_>>()
            .join(" + ");
        Row {
            keys,
            context: binding.context.display(),
            action_name,
            description,
        }
    };

    let mut default_rows: Vec<Row> = Vec::new();
    for binding in DEFAULT_BINDINGS.iter().rev() {
        if filter_event.is_none_or(|ev| binding.matches(ev)) {
            default_rows.push(binding_to_row(binding));
        }
    }

    let mut user_rows: Vec<Row> = Vec::new();
    for binding in user_bindings.iter() {
        if filter_event.is_none_or(|ev| binding.matches(ev)) {
            user_rows.push(binding_to_row(binding));
        }
    }

    // Retrieve the terminal width; fall back to 120 columns if unavailable.
    let term_width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(120);

    let constraints = [
        Constraint::Fill(1), // Key(s)
        Constraint::Fill(2), // Context
        Constraint::Fill(2), // KeyEventAction
        Constraint::Fill(3), // Description
    ];
    let options = TableOptions { row_dividers: true };

    let render_rows = |title: &str, rows: &[Row]| {
        if rows.is_empty() {
            return;
        }
        println!("{}:", title);
        let mut accum = TableAccum::default();
        accum.header_cells = vec![
            "Key(s)".to_string(),
            "Context".to_string(),
            "KeyEventAction".to_string(),
            "Description".to_string(),
        ];
        for row in rows {
            accum.body_rows.push(vec![
                row.keys.clone(),
                row.context.clone(),
                row.action_name.clone(),
                row.description.clone(),
            ]);
        }

        for line in render_table_constrained(&accum, &constraints, term_width, &options) {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("{}", text);
        }
        println!();
    };

    render_rows("Default Keybindings", &default_rows);
    render_rows("User Keybindings", &user_rows);

    // Print remappings table after keybindings.
    if !remappings.is_empty() {
        println!("\nKey Remappings:");
        for remap in remappings {
            match remap {
                KeyRemap::Key { from, to } => {
                    println!("  {} -> {}", display_keycode(*from), display_keycode(*to));
                }
                KeyRemap::Modifier { from, to } => {
                    println!(
                        "  {} -> {}",
                        display_modifier_bit(*from),
                        display_modifier_bit(*to)
                    );
                }
                KeyRemap::Event { from, to } => {
                    println!(
                        "  {} -> {}",
                        display_key_event(*from),
                        display_key_event(*to)
                    );
                }
            }
        }
    }

    // Detect and print key-binding conflicts.
    let conflicts = detect_binding_conflicts(user_bindings, remappings);
    if !conflicts.is_empty() {
        println!("\nKey Binding Conflicts:");
        let use_color = std::io::stdout().is_terminal();
        for conflict in &conflicts {
            // "INACCESSIBLE: key" formatted as blinking white on red.
            let label = format!("INACCESSIBLE: {}", conflict.inaccessible_action);
            let styled_label = if use_color {
                format!("{}{}{}", ANSI_BLINK_WHITE_ON_RED, label, ANSI_RESET)
            } else {
                label
            };
            println!(
                "  {}  (shadowed by {} with key {})",
                styled_label, conflict.shadowing_action, conflict.key_display
            );
        }
    }
}

impl<'a> App<'a> {
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        let _timer = crate::perf::PerfTimer::start("handle_key_event");
        let initial_leader_time = self.leader_key_active_at;
        log::trace!("Key event: {:?}", key);
        self.right_click_popup_pos = None;
        self.right_click_copy_target = None;

        let key = apply_remappings(key, &self.settings.key_remappings);
        log::trace!("Key event after remapping: {:?}", key);

        // Evaluate every context variable once up front, so each variable's
        // condition runs at most once per key press regardless of how many
        // bindings reference it.
        let context_values = ContextValues::evaluate(self);

        // Find the highest-priority binding whose context is satisfied and
        // whose key matches.  We extract the action (Copy) before running it
        // so that running the action does not overlap with the immutable
        // borrow of `self.settings.keybindings`.
        let mut matched: Option<(Vec<KeyEventAction>, String)> = None;
        for binding in self
            .settings
            .keybindings
            .iter()
            .rev()
            .chain(DEFAULT_BINDINGS.iter())
        {
            if binding.context.evaluate(&context_values) && binding.matches(key) {
                matched = Some((binding.actions.clone(), binding.context.display()));
                break;
            }
        }

        let (context_debug, action_enums) = match &matched {
            Some((actions, context)) => (context.clone(), actions.clone()),
            None => ("none".to_string(), vec![KeyEventAction::Nothing]),
        };
        let sequence_number = self
            .last_key
            .as_ref()
            .map_or(1, |lk| lk.sequence_number + 1);
        self.last_key = Some(crate::app::LastKeyPress {
            key,
            display: display_key_event(key),
            context: context_debug,
            actions: action_enums,
            sequence_number,
        });

        if let Some((actions, _)) = &matched {
            for action in actions {
                if !self.mode.is_running() {
                    log::warn!("Ignoring other actions because mode is no longer running");
                    break;
                }
                log::trace!("Matched binding: {}", action.as_str());
                action.run(self, key);
            }
        }

        if matched
            .as_ref()
            .is_some_and(|(actions, _)| !actions.contains(&KeyEventAction::ToggleMouse))
            && self.settings.mouse_mode == MouseMode::Smart
            && self.mouse_state.is_disabled()
        {
            log::debug!("Reenabling mouse due to key event");
            self.mouse_state.enable();
        }

        // If the leader key was active before this key event, and was not refreshed or updated
        // by a SetLeaderKey action during this key event, deactivate it now.
        if initial_leader_time.is_some() && self.leader_key_active_at == initial_leader_time {
            self.leader_key_active_at = None;
        }

        self.on_possible_buffer_change();
    }
}

#[cfg(test)]
mod tests {
    use super::super::ContextLiteral;
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn key_with_mods(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    // --- try_parse_remap ---

    #[test]
    fn test_parse_remap_key_to_key() {
        let r = try_parse_remap("tab", "z").unwrap();
        assert_eq!(
            r,
            KeyRemap::Key {
                from: KeyCode::Tab,
                to: KeyCode::Char('z')
            }
        );
    }

    #[test]
    fn test_parse_remap_modifier_to_modifier() {
        let r = try_parse_remap("alt", "ctrl").unwrap();
        assert_eq!(
            r,
            KeyRemap::Modifier {
                from: KeyModifiers::ALT,
                to: KeyModifiers::CONTROL
            }
        );
    }

    #[test]
    fn test_parse_remap_key_to_modifier_fails() {
        assert!(try_parse_remap("tab", "ctrl").is_err());
    }

    #[test]
    fn test_parse_remap_modifier_to_key_fails() {
        assert!(try_parse_remap("ctrl", "tab").is_err());
    }

    #[test]
    fn test_parse_remap_unknown_fails() {
        assert!(try_parse_remap("unknownkey", "z").is_err());
    }

    #[test]
    fn test_parse_remap_event() {
        let r = try_parse_remap("ctrl+p", "up").unwrap();
        assert_eq!(
            r,
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                to: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
            }
        );

        let r2 = try_parse_remap("ctrl+n", "alt+4").unwrap();
        assert_eq!(
            r2,
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                to: KeyEvent::new(KeyCode::Char('4'), KeyModifiers::ALT),
            }
        );
    }

    // --- apply_remappings ---

    #[test]
    fn test_apply_remappings_empty() {
        let k = key(KeyCode::Tab);
        assert_eq!(apply_remappings(k, &[]), k);
    }

    #[test]
    fn test_apply_remappings_key_remap() {
        let remappings = vec![KeyRemap::Key {
            from: KeyCode::Tab,
            to: KeyCode::Char('z'),
        }];
        let result = apply_remappings(key(KeyCode::Tab), &remappings);
        assert_eq!(result.code, KeyCode::Char('z'));
        assert_eq!(result.modifiers, KeyModifiers::empty());
    }

    #[test]
    fn test_apply_remappings_key_remap_no_match() {
        let remappings = vec![KeyRemap::Key {
            from: KeyCode::Tab,
            to: KeyCode::Char('z'),
        }];
        let result = apply_remappings(key(KeyCode::Enter), &remappings);
        assert_eq!(result.code, KeyCode::Enter);
    }

    #[test]
    fn test_apply_remappings_modifier_remap() {
        let remappings = vec![KeyRemap::Modifier {
            from: KeyModifiers::ALT,
            to: KeyModifiers::CONTROL,
        }];
        let k = key_with_mods(KeyCode::Char('a'), KeyModifiers::ALT);
        let result = apply_remappings(k, &remappings);
        assert_eq!(result.code, KeyCode::Char('a'));
        assert!(result.modifiers.contains(KeyModifiers::CONTROL));
        assert!(!result.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn test_apply_remappings_swap_modifiers() {
        // Remap alt→ctrl and ctrl→alt simultaneously (swap).
        let remappings = vec![
            KeyRemap::Modifier {
                from: KeyModifiers::ALT,
                to: KeyModifiers::CONTROL,
            },
            KeyRemap::Modifier {
                from: KeyModifiers::CONTROL,
                to: KeyModifiers::ALT,
            },
        ];

        // Alt-only → should become Ctrl-only.
        let k = key_with_mods(KeyCode::Char('a'), KeyModifiers::ALT);
        let result = apply_remappings(k, &remappings);
        assert!(result.modifiers.contains(KeyModifiers::CONTROL));
        assert!(!result.modifiers.contains(KeyModifiers::ALT));

        // Ctrl-only → should become Alt-only.
        let k = key_with_mods(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let result = apply_remappings(k, &remappings);
        assert!(result.modifiers.contains(KeyModifiers::ALT));
        assert!(!result.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn test_apply_remappings_event() {
        let remappings = vec![
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                to: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
            },
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
                to: KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
            },
            KeyRemap::Modifier {
                from: KeyModifiers::CONTROL,
                to: KeyModifiers::ALT,
            },
        ];

        // 1. Ctrl+P should map to Up
        let k1 = key_with_mods(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let r1 = apply_remappings(k1, &remappings);
        assert_eq!(r1.code, KeyCode::Up);
        assert_eq!(r1.modifiers, KeyModifiers::empty());

        // 2. Ctrl+A should map to Esc (Event remap takes precedence over Modifier remap)
        let k2 = key_with_mods(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let r2 = apply_remappings(k2, &remappings);
        assert_eq!(r2.code, KeyCode::Esc);
        assert_eq!(r2.modifiers, KeyModifiers::empty());

        // 3. Ctrl+B should map to Alt+B (Modifier remap still applies to other keys)
        let k3 = key_with_mods(KeyCode::Char('b'), KeyModifiers::CONTROL);
        let r3 = apply_remappings(k3, &remappings);
        assert_eq!(r3.code, KeyCode::Char('b'));
        assert_eq!(r3.modifiers, KeyModifiers::ALT);
    }

    #[test]
    fn test_apply_remappings_compound() {
        let remappings = vec![
            KeyRemap::Modifier {
                from: KeyModifiers::CONTROL,
                to: KeyModifiers::ALT,
            },
            KeyRemap::Key {
                from: KeyCode::Char('p'),
                to: KeyCode::Char('k'),
            },
        ];

        // Ctrl+P should map to Alt+K (Modifier and Key remap applied simultaneously/independently)
        let k = key_with_mods(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let r = apply_remappings(k, &remappings);
        assert_eq!(r.code, KeyCode::Char('k'));
        assert_eq!(r.modifiers, KeyModifiers::ALT);
    }

    // --- inverse display ---

    #[test]
    fn test_display_no_remapping() {
        let kem = KeyEventMatch::Exact(key(KeyCode::Tab));
        assert_eq!(kem.display_with_remapping(&[]), "Tab");
    }

    #[test]
    fn test_display_remapped_key_shows_physical_key() {
        // Tab → z: a binding expecting 'z' should display as "Tab" (what user presses).
        let remappings = vec![KeyRemap::Key {
            from: KeyCode::Tab,
            to: KeyCode::Char('z'),
        }];
        let kem = KeyEventMatch::Exact(key(KeyCode::Char('z')));
        assert_eq!(kem.display_with_remapping(&remappings), "Tab");
    }

    #[test]
    fn test_display_inaccessible_key() {
        // Tab → z: a binding expecting Tab is now inaccessible.
        let remappings = vec![KeyRemap::Key {
            from: KeyCode::Tab,
            to: KeyCode::Char('z'),
        }];
        let kem = KeyEventMatch::Exact(key(KeyCode::Tab));
        assert_eq!(
            kem.display_with_remapping(&remappings),
            "[INACCESSIBLE: Tab]"
        );
    }

    #[test]
    fn test_display_escape_remapped_to_tab() {
        // Escape → Tab: a binding expecting Tab should display as "Esc".
        let remappings = vec![KeyRemap::Key {
            from: KeyCode::Esc,
            to: KeyCode::Tab,
        }];
        let kem = KeyEventMatch::Exact(key(KeyCode::Tab));
        assert_eq!(kem.display_with_remapping(&remappings), "Esc");
    }

    #[test]
    fn test_display_unaffected_key() {
        // Tab → z: Enter is unaffected.
        let remappings = vec![KeyRemap::Key {
            from: KeyCode::Tab,
            to: KeyCode::Char('z'),
        }];
        let kem = KeyEventMatch::Exact(key(KeyCode::Enter));
        assert_eq!(kem.display_with_remapping(&remappings), "Enter");
    }

    #[test]
    fn test_display_remapped_event_shows_alternatives() {
        let remappings = vec![KeyRemap::Event {
            from: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
            to: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        }];

        // Up is bound, Ctrl+P is mapped to Up. Physical keys should show "Up or Ctrl+P"
        let kem = KeyEventMatch::Exact(key(KeyCode::Up));
        assert_eq!(kem.display_with_remapping(&remappings), "Up or Ctrl+p");

        // If the logical key itself is mapped away:
        // E.g. Ctrl+P -> Up, and Up -> Down.
        // Then Up is mapped away (so it's not a physical key option anymore).
        // A binding on Down should show "Ctrl+P or Up" (or just the ones that target it).
        let remappings_chain = vec![
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                to: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
            },
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
                to: KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
            },
        ];
        let kem_down = KeyEventMatch::Exact(key(KeyCode::Down));
        assert_eq!(
            kem_down.display_with_remapping(&remappings_chain),
            "Down or Up"
        );
    }

    #[test]
    fn test_display_remapped_event_multiple_alternatives() {
        let remappings = vec![
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                to: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
            },
            KeyRemap::Event {
                from: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
                to: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
            },
        ];

        let kem = KeyEventMatch::Exact(key(KeyCode::Up));
        // Up is bound, Ctrl+P and Alt+P are mapped to Up. Physical keys should show all options.
        assert_eq!(
            kem.display_with_remapping(&remappings),
            "Up or Ctrl+p or Alt+p"
        );
    }

    #[test]
    fn test_print_bindings_table_with_event_remap() {
        let remappings = vec![KeyRemap::Event {
            from: KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
            to: KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        }];
        // Ensure that print_bindings_table runs with KeyRemap::Event without panic
        print_bindings_table(&[], None, &remappings);
    }

    #[test]
    fn test_display_inaccessible_modifier() {
        // Alt → Ctrl: a binding expecting Ctrl+a is accessible; expecting Alt+a is inaccessible.
        let remappings = vec![KeyRemap::Modifier {
            from: KeyModifiers::ALT,
            to: KeyModifiers::CONTROL,
        }];

        let kem_ctrl =
            KeyEventMatch::Exact(key_with_mods(KeyCode::Char('a'), KeyModifiers::CONTROL));
        // Ctrl+a is not targeted by any remap, but Alt is remapped away TO Ctrl.
        // So the inverse: Ctrl was produced by Alt → show Alt.
        assert_eq!(kem_ctrl.display_with_remapping(&remappings), "Alt+a");

        let kem_alt = KeyEventMatch::Exact(key_with_mods(KeyCode::Char('a'), KeyModifiers::ALT));
        // Alt+a: Alt is remapped away → inaccessible.
        assert_eq!(
            kem_alt.display_with_remapping(&remappings),
            "[INACCESSIBLE: Alt]+a"
        );
    }

    // --- parse_single_keycode aliases ---

    #[test]
    fn test_parse_keycode_aliases() {
        assert_eq!(parse_single_keycode("bkspc").unwrap(), KeyCode::Backspace);
        assert_eq!(parse_single_keycode("bs").unwrap(), KeyCode::Backspace);
        assert_eq!(parse_single_keycode("ret").unwrap(), KeyCode::Enter);
        assert_eq!(parse_single_keycode("return").unwrap(), KeyCode::Enter);
        assert_eq!(parse_single_keycode("del").unwrap(), KeyCode::Delete);
        assert_eq!(parse_single_keycode("ins").unwrap(), KeyCode::Insert);
        assert_eq!(parse_single_keycode("pgup").unwrap(), KeyCode::PageUp);
        assert_eq!(parse_single_keycode("pgdown").unwrap(), KeyCode::PageDown);
        assert_eq!(parse_single_keycode("pgdn").unwrap(), KeyCode::PageDown);
        assert_eq!(parse_single_keycode("space").unwrap(), KeyCode::Char(' '));
        assert_eq!(parse_single_keycode("spc").unwrap(), KeyCode::Char(' '));
        assert_eq!(parse_single_keycode("null").unwrap(), KeyCode::Null);
        assert_eq!(parse_single_keycode("caps").unwrap(), KeyCode::CapsLock);
        assert_eq!(
            parse_single_keycode("prtscn").unwrap(),
            KeyCode::PrintScreen
        );
        assert_eq!(
            parse_single_keycode("keypad_begin").unwrap(),
            KeyCode::KeypadBegin
        );
    }

    #[test]
    fn test_parse_keycode_f_keys() {
        assert_eq!(parse_single_keycode("f1").unwrap(), KeyCode::F(1));
        assert_eq!(parse_single_keycode("F1").unwrap(), KeyCode::F(1));
        assert_eq!(parse_single_keycode("f12").unwrap(), KeyCode::F(12));
        assert_eq!(parse_single_keycode("f255").unwrap(), KeyCode::F(255));
    }

    #[test]
    fn test_parse_keycode_media() {
        use crossterm::event::MediaKeyCode;
        assert_eq!(
            parse_single_keycode("media:play").unwrap(),
            KeyCode::Media(MediaKeyCode::Play)
        );
        assert_eq!(
            parse_single_keycode("media:pause").unwrap(),
            KeyCode::Media(MediaKeyCode::Pause)
        );
        assert_eq!(
            parse_single_keycode("media:playpause").unwrap(),
            KeyCode::Media(MediaKeyCode::PlayPause)
        );
        assert_eq!(
            parse_single_keycode("media:mute").unwrap(),
            KeyCode::Media(MediaKeyCode::MuteVolume)
        );
        assert_eq!(
            parse_single_keycode("media:volumeup").unwrap(),
            KeyCode::Media(MediaKeyCode::RaiseVolume)
        );
        assert_eq!(
            parse_single_keycode("media:volumedown").unwrap(),
            KeyCode::Media(MediaKeyCode::LowerVolume)
        );
        assert_eq!(
            parse_single_keycode("media:tracknext").unwrap(),
            KeyCode::Media(MediaKeyCode::TrackNext)
        );
    }

    #[test]
    fn test_parse_keycode_modifier_key() {
        use crossterm::event::ModifierKeyCode;
        assert_eq!(
            parse_single_keycode("modifier:leftshift").unwrap(),
            KeyCode::Modifier(ModifierKeyCode::LeftShift)
        );
        assert_eq!(
            parse_single_keycode("modifier:rightctrl").unwrap(),
            KeyCode::Modifier(ModifierKeyCode::RightControl)
        );
        assert_eq!(
            parse_single_keycode("modifier:leftsuper").unwrap(),
            KeyCode::Modifier(ModifierKeyCode::LeftSuper)
        );
    }

    #[test]
    fn test_parse_key_code_cases() {
        assert_eq!(parse_single_keycode("c").unwrap(), KeyCode::Char('c'));

        assert_eq!(parse_single_keycode("C").unwrap(), KeyCode::Char('c'));

        assert_eq!(parse_single_keycode("@").unwrap(), KeyCode::Char('@'));

        assert_eq!(parse_single_keycode("2").unwrap(), KeyCode::Char('2'));

        assert_eq!(parse_single_keycode("\"").unwrap(), KeyCode::Char('"'));

        assert_eq!(
            KeyEventMatch::try_from("Super+Z").unwrap(),
            KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::SUPER))
        );

        assert_eq!(
            KeyEventMatch::try_from("Super+z").unwrap(),
            KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::SUPER))
        );
    }

    #[test]
    fn test_shifted_char_binding_matches_uppercase_event_char() {
        let binding =
            Binding::try_new_from_strs("Shift+J", "always=fuzzyHistorySelectNext").unwrap();

        assert!(binding.matches(key_with_mods(KeyCode::Char('J'), KeyModifiers::SHIFT)));

        assert!(binding.matches(key_with_mods(KeyCode::Char('j'), KeyModifiers::SHIFT)));
    }

    // --- parse_single_modifier aliases ---

    #[test]
    fn test_parse_modifier_aliases() {
        assert_eq!(
            parse_single_modifier("command").unwrap(),
            KeyModifiers::SUPER
        );
        assert_eq!(parse_single_modifier("gui").unwrap(), KeyModifiers::SUPER);
        assert_eq!(parse_single_modifier("option").unwrap(), KeyModifiers::ALT);
        assert_eq!(parse_single_modifier("hyper").unwrap(), KeyModifiers::HYPER);
    }

    // --- key_event_match_overlaps ---

    #[test]
    fn test_overlap_exact_same_key() {
        let a = KeyEventMatch::Exact(key(KeyCode::Tab));
        let b = KeyEventMatch::Exact(key(KeyCode::Tab));
        assert!(key_event_a_shadows_b(&a, &b));
    }

    #[test]
    fn test_overlap_exact_different_keys() {
        let a = KeyEventMatch::Exact(key(KeyCode::Tab));
        let b = KeyEventMatch::Exact(key(KeyCode::Enter));
        assert!(!key_event_a_shadows_b(&a, &b));
    }

    #[test]
    fn test_overlap_exact_same_key_different_modifiers() {
        let a = KeyEventMatch::Exact(key_with_mods(KeyCode::Char('a'), KeyModifiers::CONTROL));
        let b = KeyEventMatch::Exact(key_with_mods(KeyCode::Char('a'), KeyModifiers::ALT));
        assert!(!key_event_a_shadows_b(&a, &b));
    }

    #[test]
    fn test_overlap_exact_same_key_different_modifiers_no_shadow() {
        let a = KeyEventMatch::Exact(key(KeyCode::Home));
        let b = KeyEventMatch::Exact(key_with_mods(KeyCode::Home, KeyModifiers::SHIFT));
        assert!(!key_event_a_shadows_b(&a, &b));
    }

    #[test]
    fn test_overlap_anychar_and_anychar() {
        let a = KeyEventMatch::AnyCharAndMods(KeyModifiers::empty());
        let b = KeyEventMatch::AnyCharAndMods(KeyModifiers::CONTROL);
        assert!(!key_event_a_shadows_b(&a, &b));
    }

    #[test]
    fn test_overlap_anychar_and_exact_char() {
        let a = KeyEventMatch::AnyCharAndMods(KeyModifiers::empty());
        let b = KeyEventMatch::Exact(key(KeyCode::Char('q')));
        assert!(key_event_a_shadows_b(&a, &b));
        assert!(key_event_a_shadows_b(&b, &a));
    }

    #[test]
    fn test_overlap_anychar_and_exact_char_different_modifiers() {
        let a = KeyEventMatch::AnyCharAndMods(KeyModifiers::empty());
        let b = KeyEventMatch::Exact(key_with_mods(KeyCode::Char('q'), KeyModifiers::SHIFT));
        assert!(!key_event_a_shadows_b(&a, &b));
    }

    #[test]
    fn test_overlap_anychar_and_exact_nonchar() {
        let a = KeyEventMatch::AnyCharAndMods(KeyModifiers::empty());
        let b = KeyEventMatch::Exact(key(KeyCode::Tab));
        assert!(!key_event_a_shadows_b(&a, &b));
        assert!(!key_event_a_shadows_b(&b, &a));
    }

    #[test]
    fn test_binding_matches_requires_exact_modifiers() {
        let binding = Binding::try_new_from_strs("Home", "always=moveLeftStartOfLine").unwrap();

        assert!(binding.matches(key(KeyCode::Home)));
        assert!(!binding.matches(key_with_mods(KeyCode::Home, KeyModifiers::SHIFT)));
    }

    #[test]
    fn test_context_expr_parse_single() {
        let e = ContextExpr::try_from("always").unwrap();
        assert!(e.literals.len() == 1);
        assert!(e.literals[0].var == ContextVar::Always);
        assert!(!e.literals[0].negated);
    }

    #[test]
    fn test_context_expr_parse_new_vars() {
        let e = ContextExpr::try_from(
            "bufferIsEmpty+tabCompletionEntrySelected+tabCompletionOneResult+tabCompletionNoFilteredResults+tabCompletionNoResults+multilineBuffer",
        )
        .unwrap();
        assert!(e.literals[0].var == ContextVar::BufferIsEmpty);
        assert!(e.literals[1].var == ContextVar::TabCompletionEntrySelected);
        assert!(e.literals[2].var == ContextVar::TabCompletionOneResult);
        assert!(e.literals[3].var == ContextVar::TabCompletionNoFilteredResults);
        assert!(e.literals[4].var == ContextVar::TabCompletionNoResults);
        assert!(e.literals[5].var == ContextVar::MultilineBuffer);
    }

    #[test]
    fn test_context_expr_parse_and_chain() {
        let e = ContextExpr::try_from("inlineSuggestionAvailable+cursorAtEnd").unwrap();
        assert!(e.literals.len() == 2);
        assert!(e.literals[0].var == ContextVar::InlineSuggestionAvailable);
        assert!(e.literals[1].var == ContextVar::CursorAtEnd);
    }

    #[test]
    fn test_context_expr_parse_negation() {
        let e = ContextExpr::try_from("!textSelected+cursorAtEnd").unwrap();
        assert!(e.literals[0].negated);
        assert!(e.literals[0].var == ContextVar::TextSelected);
        assert!(!e.literals[1].negated);
        assert!(e.literals[1].var == ContextVar::CursorAtEnd);
    }

    #[test]
    fn test_context_expr_rejects_or() {
        assert!(ContextExpr::try_from("a||b").is_err());
    }

    #[test]
    fn test_context_expr_rejects_old_and_separator() {
        assert!(ContextExpr::try_from("a&&b").is_err());
    }

    #[test]
    fn test_context_expr_rejects_parens() {
        assert!(ContextExpr::try_from("(a+b)").is_err());
    }

    #[test]
    fn test_context_expr_rejects_unknown_var() {
        assert!(ContextExpr::try_from("notAVariable").is_err());
    }

    #[test]
    fn test_context_expr_display_roundtrip() {
        let s = "inlineSuggestionAvailable+!textSelected+cursorAtEnd";
        let e = ContextExpr::try_from(s).unwrap();
        assert!(e.display() == s);
    }

    #[test]
    fn test_context_expr_operator_and_from_vars() {
        let e = ContextVar::FuzzyHistorySearch + ContextVar::CursorAtEnd;
        assert!(e.literals.len() == 2);
        assert!(e.literals[0] == ContextLiteral::new(ContextVar::FuzzyHistorySearch, false));
        assert!(e.literals[1] == ContextLiteral::new(ContextVar::CursorAtEnd, false));
    }

    #[test]
    fn test_context_expr_operator_not_and_chain() {
        let e = !ContextVar::TextSelected + ContextVar::CursorAtEnd;
        assert!(e.literals.len() == 2);
        assert!(e.literals[0] == ContextLiteral::new(ContextVar::TextSelected, true));
        assert!(e.literals[1] == ContextLiteral::new(ContextVar::CursorAtEnd, false));
    }

    #[test]
    fn test_context_expr_operator_chain_exprs() {
        let e = (ContextVar::InlineSuggestionAvailable + !ContextVar::TextSelected)
            + ContextVar::CursorAtEnd;
        assert!(e.display() == "inlineSuggestionAvailable+!textSelected+cursorAtEnd");
    }

    #[test]
    fn test_action_id_from_str_known() {
        assert!(
            KeyEventAction::try_from("submitOrNewline").unwrap() == KeyEventAction::SubmitOrNewline
        );
        assert!(
            KeyEventAction::try_from("inlineSuggestionAccept").unwrap()
                == KeyEventAction::InlineSuggestionAccept
        );
        assert!(KeyEventAction::try_from("clearBuffer").unwrap() == KeyEventAction::ClearBuffer);
    }

    #[test]
    fn test_action_id_from_str_unknown() {
        assert!(KeyEventAction::try_from("not_a_real_action").is_err());
    }

    #[test]
    fn test_binding_try_new_from_strs_basic() {
        let b = Binding::try_new_from_strs("Ctrl+Enter", "always=submitOrNewline").unwrap();
        assert_eq!(b.actions, vec![KeyEventAction::SubmitOrNewline]);
        assert!(b.context.literals.len() == 1);
        assert!(b.context.literals[0].var == ContextVar::Always);
    }

    #[test]
    fn test_binding_try_new_from_strs_compound_context() {
        let b = Binding::try_new_from_strs(
            "Tab",
            "inlineSuggestionAvailable+cursorAtEnd=inlineSuggestionAccept",
        )
        .unwrap();
        assert_eq!(b.actions, vec![KeyEventAction::InlineSuggestionAccept]);
        assert!(b.context.literals.len() == 2);
    }

    #[test]
    fn test_binding_try_new_from_strs_multi_actions() {
        let b = Binding::try_new_from_strs(
            "Ctrl+g",
            "always=submitOrNewline+inlineSuggestionAccept+escapeToNormalMode",
        )
        .unwrap();
        assert_eq!(
            b.actions,
            vec![
                KeyEventAction::SubmitOrNewline,
                KeyEventAction::InlineSuggestionAccept,
                KeyEventAction::EscapeToNormalMode
            ]
        );
        assert!(b.context.literals.len() == 1);
        assert!(b.context.literals[0].var == ContextVar::Always);
    }

    #[test]
    fn test_binding_try_new_from_strs_insert_string() {
        let b = Binding::try_new_from_strs("Ctrl+g", "always=insertString('git checkout -b ')")
            .unwrap();
        assert_eq!(
            b.actions,
            vec![KeyEventAction::InsertString("git checkout -b ".to_string())]
        );

        let b2 =
            Binding::try_new_from_strs("Ctrl+g", "always=insertString(\"hello\\nworld\")").unwrap();
        assert_eq!(
            b2.actions,
            vec![KeyEventAction::InsertString("hello\nworld".to_string())]
        );

        let b3 = Binding::try_new_from_strs("Ctrl+g", "always=insertString(hello)").unwrap();
        assert_eq!(
            b3.actions,
            vec![KeyEventAction::InsertString("hello".to_string())]
        );

        let b4 = Binding::try_new_from_strs("Ctrl+g", "always=insertString('hello')").unwrap();
        assert_eq!(
            b4.actions,
            vec![KeyEventAction::InsertString("hello".to_string())]
        );

        let b5 = Binding::try_new_from_strs("Ctrl+g", "always=insertString(\"hello\")").unwrap();
        assert_eq!(
            b5.actions,
            vec![KeyEventAction::InsertString("hello".to_string())]
        );

        assert!(Binding::try_new_from_strs("Ctrl+g", "always=insertString(hello").is_err());
    }

    #[test]
    fn test_possible_context_action_completions_exact_context_yields_separators() {
        let values = possible_context_action_completions(std::ffi::OsStr::new("always"))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(values.contains(&"PREFIX_DELIMalways+NO_SUFFIX".to_string()));
        assert!(values.contains(&"PREFIX_DELIMalways=NO_SUFFIX".to_string()));
    }

    #[test]
    fn test_possible_context_action_completions_partial_context_yields_bare_match() {
        let values = possible_context_action_completions(std::ffi::OsStr::new("inline"))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(values.contains(&"PREFIX_DELIMinlineSuggestionAvailableNO_SUFFIX".to_string()));
    }

    #[test]
    fn test_possible_context_action_completions_partial_action_yields_no_suffix() {
        let values = possible_context_action_completions(std::ffi::OsStr::new("always=submitOr"))
            .into_iter()
            .map(|c| c.get_value().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(values.contains(&"always=PREFIX_DELIMsubmitOrNewlineNO_SUFFIX".to_string()));
    }

    #[test]
    fn test_possible_context_action_completions_full_action_yields_space_or_plus() {
        let values =
            possible_context_action_completions(std::ffi::OsStr::new("always=submitOrNewline"))
                .into_iter()
                .map(|c| c.get_value().to_string_lossy().to_string())
                .collect::<Vec<_>>();

        assert!(values.contains(&"always=PREFIX_DELIMsubmitOrNewline".to_string()));
        assert!(values.contains(&"always=PREFIX_DELIMsubmitOrNewline+NO_SUFFIX".to_string()));
    }

    #[test]
    fn test_possible_context_action_completions_multi_actions() {
        let values = possible_context_action_completions(std::ffi::OsStr::new(
            "always=submitOrNewline+inlineSugge",
        ))
        .into_iter()
        .map(|c| c.get_value().to_string_lossy().to_string())
        .collect::<Vec<_>>();

        assert!(values.contains(
            &"always=PREFIX_DELIMsubmitOrNewline+inlineSuggestionAcceptNO_SUFFIX".to_string()
        ));
    }

    #[test]
    fn test_binding_try_new_from_strs_missing_equals() {
        assert!(Binding::try_new_from_strs("Tab", "alwayssubmitOrNewline").is_err());
    }

    #[test]
    fn test_all_action_strings_are_unique_and_roundtrip() {
        // Ensure as_str() is unique and round-trips for every variant.
        let mut seen = std::collections::HashSet::new();
        for a in KeyEventAction::iter() {
            let s = a.as_str();
            assert!(seen.insert(s));
            assert!(KeyEventAction::try_from(s).unwrap() == a);
        }
    }

    #[test]
    fn test_action_descriptions_are_non_empty() {
        for a in KeyEventAction::iter() {
            assert!(!a.description().is_empty());
        }
    }

    #[test]
    fn test_binding_new_deduplicates_key_events() {
        let events = vec![
            KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
        ];
        let binding = Binding::new(
            &events,
            ContextExpr::from(ContextVar::Always),
            &[KeyEventAction::MoveLeftStartOfLine],
        );
        assert_eq!(binding.key_events.len(), 2);
        assert_eq!(
            binding.key_events[0],
            KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            binding.key_events[1],
            KeyEventMatch::Exact(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn test_remap_key_completer() {
        use std::ffi::OsStr;
        // Test that key_sequence completions are included
        let res_z = remap_key_completer(OsStr::new("z"));
        assert!(res_z.iter().any(|c| c.get_value() == "z"));

        // Test that capitalized standalone modifiers are suggested
        let res_ct = remap_key_completer(OsStr::new("ct"));
        assert!(res_ct.iter().any(|c| c.get_value() == "Ctrl"));

        let res_al = remap_key_completer(OsStr::new("al"));
        assert!(res_al.iter().any(|c| c.get_value() == "Alt"));
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, IntoStaticStr, EnumMessage, VariantArray,
)]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub(crate) enum ContextVar {
    #[strum(message = "Always true; the catch-all context for unconditional bindings")]
    Always,
    #[strum(message = "The command buffer is empty")]
    BufferIsEmpty,
    #[strum(message = "Fuzzy history search overlay is active")]
    FuzzyHistorySearch,
    #[strum(message = "Fuzzy history search overlay for normal commands is active")]
    FuzzyHistorySearchNormalCommands,
    #[strum(message = "Fuzzy history search overlay for cancelled commands is active")]
    FuzzyHistorySearchCancelledCommands,
    #[strum(message = "Fuzzy history search overlay for agent commands is active")]
    FuzzyHistorySearchAgentCommands,
    #[strum(message = "Waiting for tab completion candidates to be produced")]
    TabCompletionWaiting,
    #[strum(message = "Tab completion overlay is active (any state)")]
    TabCompletion,
    #[strum(message = "Tab completion overlay is active and has at least one candidate")]
    TabCompletionAvailable,
    #[strum(message = "Tab completion overlay has at least one candidate and a selected entry")]
    TabCompletionEntrySelected,
    #[strum(message = "Tab completion overlay is active and has exactly one filtered candidate")]
    TabCompletionOneResult,
    #[strum(message = "Tab completion overlay is showing more than one column of candidates")]
    TabCompletionMultiColAvailable,
    #[strum(message = "Tab completion overlay is active but fuzzy filtering has no matches")]
    TabCompletionNoFilteredResults,
    #[strum(message = "Tab completion overlay is active and has no candidates at all")]
    TabCompletionNoResults,
    #[strum(message = "Tab completion was triggered by the user (not auto-started)")]
    UserTriggeredSuggestions,
    #[strum(message = "Waiting for the agent mode subprocess to finish")]
    AgentModeWaiting,
    #[strum(message = "Agent mode finished and is showing a list of selectable suggestions")]
    AgentOutputSelection,
    #[strum(message = "Agent mode failed and is showing an error message")]
    AgentModeError,
    #[strum(message = "An inline history suggestion is available to be accepted")]
    InlineSuggestionAvailable,
    #[strum(message = "Cursor is at the end of the buffer")]
    CursorAtEnd,
    #[strum(message = "Cursor is at the end of the trimmed buffer")]
    CursorAtEndTrimmed,
    #[strum(message = "Cursor is at the start of the buffer")]
    CursorAtStart,
    #[strum(message = "Cursor is on the first line of the buffer")]
    CursorOnFirstLine,
    #[strum(message = "Cursor is on the final line of the buffer")]
    CursorOnFinalLine,
    #[strum(message = "Prompt directory selection mode is active")]
    PromptDirSelection,
    #[strum(message = "There is an active text selection in the buffer")]
    TextSelected,
    #[strum(message = "The command buffer contains at least one newline")]
    MultilineBuffer,
    #[strum(message = "The command buffer starts with an agent mode prefix")]
    BufferHasAgentModePrefix,
    #[strum(message = "The content mode is normal editing (no overlay is active)")]
    EditingBufferMode,
    #[strum(message = "Prompting the user whether they want to run flycomp")]
    TabCompletionAskForFlycomp,
    #[strum(message = "Flycomp completion synthesis is currently running in the background")]
    TabCompletionRunningFlycomp,
    #[strum(message = "Flycomp completion synthesis finished and has a result or error")]
    TabCompletionFlycompResult,
    #[strum(message = "Fuzzy history search overlay is active and no entry is currently selected")]
    FuzzyHistorySearchNoneSelected,
    #[strum(message = "Agent output selection is active and no suggestion is currently selected")]
    AgentOutputNoneSelected,
    #[strum(message = "The leader key is currently active")]
    LeaderKeyActive,
}

impl ContextVar {
    pub(crate) fn as_str(&self) -> &'static str {
        <&'static str>::from(*self)
    }

    pub(crate) fn evaluate(&self, app: &App) -> bool {
        match self {
            ContextVar::Always => true,
            ContextVar::BufferIsEmpty => app.buffer.buffer().is_empty(),
            ContextVar::FuzzyHistorySearch => {
                matches!(app.content_mode, ContentMode::FuzzyHistorySearch(_))
            }
            ContextVar::FuzzyHistorySearchNormalCommands => {
                matches!(
                    app.content_mode,
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::PastCommands)
                )
            }
            ContextVar::FuzzyHistorySearchCancelledCommands => {
                matches!(
                    app.content_mode,
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::CancelledCommands)
                )
            }
            ContextVar::FuzzyHistorySearchAgentCommands => {
                matches!(
                    app.content_mode,
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts)
                )
            }
            ContextVar::TabCompletionWaiting => {
                matches!(app.content_mode, ContentMode::TabCompletionWaiting { .. })
            }
            ContextVar::TabCompletion => {
                matches!(app.content_mode, ContentMode::TabCompletion { .. })
            }
            ContextVar::TabCompletionAvailable => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() > 0
            ),
            ContextVar::TabCompletionEntrySelected => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() > 0
                        && active_suggestions.selected_coord.is_some()
            ),
            ContextVar::TabCompletionOneResult => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() == 1
            ),
            ContextVar::TabCompletionMultiColAvailable => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.last_num_data_cols > 1
            ),
            ContextVar::TabCompletionNoFilteredResults => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() == 0
            ),
            ContextVar::TabCompletionNoResults => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.all_suggestions_len() == 0
            ),
            ContextVar::UserTriggeredSuggestions => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if !active_suggestions.auto_started
            ),
            ContextVar::AgentModeWaiting => {
                matches!(app.content_mode, ContentMode::AgentModeWaiting { .. })
            }
            ContextVar::AgentOutputSelection => {
                matches!(app.content_mode, ContentMode::AgentOutputSelection { .. })
            }
            ContextVar::AgentModeError => {
                matches!(app.content_mode, ContentMode::AgentError { .. })
            }
            ContextVar::InlineSuggestionAvailable => app.inline_history_suggestion.is_some(),
            ContextVar::CursorAtEnd => app.buffer.is_cursor_at_end(),
            ContextVar::CursorAtEndTrimmed => app.buffer.is_cursor_at_trimmed_end(),
            ContextVar::CursorAtStart => app.buffer.is_cursor_at_start(),
            ContextVar::CursorOnFirstLine => app.buffer.cursor_row() == 0,
            ContextVar::CursorOnFinalLine => app.buffer.is_cursor_on_final_line(),
            ContextVar::PromptDirSelection => {
                matches!(app.content_mode, ContentMode::PromptDirSelect(_))
            }
            ContextVar::TextSelected => app.buffer.selection_range().is_some(),
            ContextVar::MultilineBuffer => app.buffer.buffer().contains('\n'),
            ContextVar::BufferHasAgentModePrefix => {
                app.buffer_starts_with_agent_command_prefix().is_some()
            }
            ContextVar::EditingBufferMode => matches!(app.content_mode, ContentMode::Normal),
            ContextVar::TabCompletionAskForFlycomp => {
                matches!(
                    app.content_mode,
                    ContentMode::TabCompletionAskForFlycomp { .. }
                )
            }
            ContextVar::TabCompletionRunningFlycomp => {
                matches!(
                    app.content_mode,
                    ContentMode::TabCompletionRunningFlycomp { .. }
                )
            }
            ContextVar::TabCompletionFlycompResult => {
                matches!(
                    app.content_mode,
                    ContentMode::TabCompletionFlycompResult { .. }
                )
            }
            ContextVar::FuzzyHistorySearchNoneSelected => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    app.select_fuzzy_history_manager(source)
                        .fuzzy_search_idx()
                        .is_none()
                } else {
                    false
                }
            }
            ContextVar::AgentOutputNoneSelected => {
                if let ContentMode::AgentOutputSelection(ref selection) = app.content_mode {
                    selection.selected_idx.is_none()
                } else {
                    false
                }
            }
            ContextVar::LeaderKeyActive => app.leader_key_active_at.map_or(false, |t| {
                t.elapsed() < std::time::Duration::from_millis(1000)
            }),
        }
    }
}

impl super::ContextVar for ContextVar {
    fn evaluate(&self, app: &App) -> bool {
        self.evaluate(app)
    }

    fn display(&self) -> String {
        self.as_str().to_string()
    }
}

impl std::ops::Not for ContextVar {
    type Output = super::ContextLiteral<ContextVar>;

    fn not(self) -> Self::Output {
        super::ContextLiteral::new(self, true)
    }
}

impl<Rhs> std::ops::Add<Rhs> for ContextVar
where
    Rhs: Into<ContextExpr>,
{
    type Output = ContextExpr;

    fn add(self, rhs: Rhs) -> Self::Output {
        ContextExpr::from(self) + rhs
    }
}
