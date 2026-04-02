use crate::app::{App, ContentMode};
use crate::bash_symbols;
use crate::history::HistorySearchDirection;
use crate::settings::MouseMode;
use crate::text_buffer::WordDelim;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope(u16);

impl Scope {
    pub const NORMAL: Self = Self(1 << 0);
    pub const FUZZY_HISTORY_SEARCH: Self = Self(1 << 1);
    pub const TAB_COMPLETION: Self = Self(1 << 2);
    pub const AGENT_MODE_WAITING: Self = Self(1 << 3);
    pub const AGENT_OUTPUT_SELECTION: Self = Self(1 << 4);
    pub const AGENT_ERROR: Self = Self(1 << 5);
    pub const INLINE_HISTORY_ACCEPTABLE: Self = Self(1 << 6);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Scope {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl Scope {
    pub fn is_active(&self, app: &App) -> bool {
        if self.contains(Scope::NORMAL) {
            true
        } else if self.contains(Scope::FUZZY_HISTORY_SEARCH) {
            matches!(
                app.content_mode,
                crate::app::ContentMode::FuzzyHistorySearch
            )
        } else if self.contains(Scope::TAB_COMPLETION) {
            matches!(
                app.content_mode,
                crate::app::ContentMode::TabCompletion { .. }
            )
        } else if self.contains(Scope::AGENT_MODE_WAITING) {
            matches!(
                app.content_mode,
                crate::app::ContentMode::AgentModeWaiting { .. }
            )
        } else if self.contains(Scope::AGENT_OUTPUT_SELECTION) {
            matches!(
                app.content_mode,
                crate::app::ContentMode::AgentOutputSelection { .. }
            )
        } else if self.contains(Scope::AGENT_ERROR) {
            matches!(app.content_mode, crate::app::ContentMode::AgentError { .. })
        } else if self.contains(Scope::INLINE_HISTORY_ACCEPTABLE) {
            app.buffer.is_cursor_at_end() && app.inline_history_suggestion.is_some()
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct Action {
    pub name: &'static str,
    pub description: &'static str,
    pub scope: Scope,
    pub action: fn(app: &mut App, key: KeyEvent),
}

impl Action {
    pub fn new(
        name: &'static str,
        description: &'static str,
        scope: Scope,
        action: fn(app: &mut App, key: KeyEvent),
    ) -> Self {
        Self {
            name,
            description,
            scope,
            action,
        }
    }
}

#[derive(Debug, Clone)]
pub enum KeyEventMatch {
    Exact(KeyEvent),
    AnyCharEitherMod(Vec<KeyModifiers>),
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
            match mod_part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                "alt" => modifiers |= KeyModifiers::ALT,
                "meta" => modifiers |= KeyModifiers::META,
                "super" | "cmd" | "win" => modifiers |= KeyModifiers::SUPER,
                _ => return Err(anyhow::anyhow!("Unknown modifier: '{}'", mod_part)),
            }
        }
        let code = if key_part.len() == 1 {
            KeyCode::Char(key_part.chars().next().unwrap())
        } else {
            match key_part.to_lowercase().as_str() {
                "enter" => KeyCode::Enter,
                "backspace" => KeyCode::Backspace,
                "left" => KeyCode::Left,
                "right" => KeyCode::Right,
                "up" => KeyCode::Up,
                "down" => KeyCode::Down,
                "home" => KeyCode::Home,
                "end" => KeyCode::End,
                "pageup" => KeyCode::PageUp,
                "pagedown" => KeyCode::PageDown,
                "tab" => KeyCode::Tab,
                "backtab" => KeyCode::BackTab,
                "delete" => KeyCode::Delete,
                "insert" => KeyCode::Insert,
                // "f"
                "esc" | "escape" => KeyCode::Esc,
                "capslock" => KeyCode::CapsLock,
                "scrolllock" => KeyCode::ScrollLock,
                "numlock" => KeyCode::NumLock,
                "printscreen" => KeyCode::PrintScreen,
                "pause" => KeyCode::Pause,
                "menu" => KeyCode::Menu,
                "keypadbegin" => KeyCode::KeypadBegin,
                // media
                // modifers?
                "anychar" => return Ok(KeyEventMatch::AnyCharEitherMod(vec![modifiers])),
                other => return Err(anyhow::anyhow!("Unknown key code: '{}'", other)),
            }
        };
        Ok(KeyEventMatch::Exact(KeyEvent::new(code, modifiers)))
    }
}

#[derive(Debug, Clone)]
pub struct Binding {
    key_events: Vec<KeyEventMatch>,
    action: Action,
}

impl Binding {
    pub fn try_new(key_events: &[&str], scope: Scope, action_name: &str) -> Result<Self> {
        let mut events = Vec::new();
        for &key_event in key_events {
            events.push(KeyEventMatch::try_from(key_event)?);
        }
        let action = POSSIBLE_ACTIONS
            .iter()
            .find(|a| a.scope == scope && a.name == action_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown action: '{}'", action_name))?;
        Ok(Self {
            key_events: events,
            action,
        })
    }

    pub fn matches(&self, key: KeyEvent) -> bool {
        self.key_events.iter().any(|k| match k {
            KeyEventMatch::Exact(action_binding) => {
                action_binding.code == key.code && key.modifiers.contains(action_binding.modifiers)
            }
            KeyEventMatch::AnyCharEitherMod(mods) => {
                matches!(key.code, KeyCode::Char(_))
                    && mods.iter().any(|m| key.modifiers.contains(*m))
            }
        })
    }
}

static POSSIBLE_ACTIONS: LazyLock<Vec<Action>> = LazyLock::new(|| {
    vec![
        Action::new(
            "accept_suggestion",
            "Accept inline history suggestion",
            Scope::INLINE_HISTORY_ACCEPTABLE,
            |app, _key| {
                if let Some((_, suf)) = &app.inline_history_suggestion {
                    app.buffer.insert_str(suf);
                    app.buffer.move_to_end();
                }
            },
        ),
        Action::new(
            "move_down_in_agent_output_selection",
            "Move down in agent output selection",
            Scope::AGENT_OUTPUT_SELECTION,
            |app, _key| {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    selection.move_down();
                }
            },
        ),
        Action::new(
            "move_up_in_agent_output_selection",
            "Move up in agent output selection",
            Scope::AGENT_OUTPUT_SELECTION,
            |app, _key| {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    selection.move_up();
                }
            },
        ),
        Action::new(
            "move_up",
            "Move up in tab completion suggestions",
            Scope::TAB_COMPLETION,
            |app, _key| {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_up_arrow();
                }
            },
        ),
        Action::new(
            "move_down",
            "Move down in tab completion suggestions",
            Scope::TAB_COMPLETION,
            |app, _key| {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_down_arrow(); // TODO combine this with tab?
                }
            },
        ),
        Action::new(
            "move_left",
            "Move left in tab completion suggestions",
            Scope::TAB_COMPLETION,
            |app, _key| {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_left_arrow();
                }
            },
        ),
        Action::new(
            "move_right",
            "Move right in tab completion suggestions",
            Scope::TAB_COMPLETION,
            |app, _key| {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_right_arrow();
                }
            },
        ),
        Action::new(
            "history_search_up",
            "Scroll up through fuzzy history search results",
            Scope::FUZZY_HISTORY_SEARCH,
            |app, _key| {
                app.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::Forward);
            },
        ),
        Action::new(
            "history_search_down",
            "Scroll down through fuzzy history search results",
            Scope::FUZZY_HISTORY_SEARCH,
            |app, _key| {
                app.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::Backward);
            },
        ),
        Action::new(
            "page_up",
            "Scroll up one page",
            Scope::FUZZY_HISTORY_SEARCH,
            |app, _key| {
                app.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::PageForward);
            },
        ),
        Action::new(
            "page_down",
            "Scroll down one page",
            Scope::FUZZY_HISTORY_SEARCH,
            |app, _key| {
                app.history_manager
                    .fuzzy_search_onkeypress(HistorySearchDirection::PageBackward);
            },
        ),
        Action::new(
            "run_agent_mode",
            "Run the agent mode command",
            Scope::NORMAL,
            |app, _key| {
                if let Some((agent_cmd, buffer)) = app.resolve_agent_command(false) {
                    app.start_agent_mode(agent_cmd, &buffer);
                } else {
                    app.show_agent_mode_not_configured_error();
                }
            },
        ),
        Action::new(
            "accept_entry",
            "Accept the currently selected entry",
            Scope::FUZZY_HISTORY_SEARCH,
            |app, _key| {
                app.accept_fuzzy_history_search();
            },
        ),
        Action::new(
            "accept_entry",
            "Accept the currently selected suggestion",
            Scope::TAB_COMPLETION,
            |app, _key| {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.accept_currently_selected(&mut app.buffer);
                    app.content_mode = ContentMode::Normal;
                }
            },
        ),
        Action::new(
            "run_help_command",
            "Run the agent mode help command",
            Scope::AGENT_ERROR,
            |app, _key| {
                app.content_mode = ContentMode::Normal;
                app.buffer.replace_buffer("flyline agent-mode --help");
                app.on_possible_buffer_change(); // TODO: is this needed?
                app.try_submit_current_buffer();
            },
        ),
        Action::new(
            "accept_entry",
            "Accept the currently selected agent output",
            Scope::AGENT_OUTPUT_SELECTION,
            |app, _key| {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    if let Some(cmd) = selection.selected_command() {
                        let cmd = cmd.to_string();
                        app.buffer.replace_buffer(&cmd);
                        app.on_possible_buffer_change(); // TODO: is this needed?
                    }
                    app.content_mode = ContentMode::Normal;
                }
            },
        ),
        Action::new(
            "submit_or_newline", // TODO name
            "Submit the current command. Insert a newline if the buffer has unclosed quotes, brackets, or parentheses.",
            Scope::NORMAL,
            |app, _key| {
                if let Some((agent_cmd, buffer)) = app.resolve_agent_command(true) {
                    app.start_agent_mode(agent_cmd, &buffer);
                } else {
                    app.try_submit_current_buffer();
                }
            },
        ),
        Action::new(
            "prev_suggestion",
            "Move to the previous tab completion suggestion",
            Scope::TAB_COMPLETION,
            |app, _key| {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_tab(true);
                }
            },
        ),
        Action::new(
            "accept_and_edit",
            "Accept the current fuzzy history search suggestion for editing",
            Scope::FUZZY_HISTORY_SEARCH,
            |app, _key| {
                app.accept_fuzzy_history_search();
            },
        ),
        Action::new(
            "next_suggestion",
            "Move to the next tab completion suggestion",
            Scope::AGENT_OUTPUT_SELECTION,
            |app, _key| {
                if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                    selection.move_down(); // TODO: cycle through
                }
            },
        ),
        Action::new(
            "next_suggestion",
            "Move to the next tab completion suggestion",
            Scope::TAB_COMPLETION,
            |app, _key| {
                if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                    active_suggestions.on_tab(false);
                }
            },
        ),
        Action::new(
            "trigger_tab_completion",
            "Trigger tab completion or cycle through suggestions if already active",
            Scope::NORMAL,
            |app, _key| app.start_tab_complete(),
        ),
        Action::new(
            "escape_to_normal_mode",
            "Escape - clear suggestions or toggle mouse (Simple and Smart modes)",
            Scope::NORMAL,
            |app, _key| {
                app.content_mode = ContentMode::Normal;
            },
        ),
        Action::new(
            "toggle_mouse",
            "Toggle mouse state (Simple and Smart modes)",
            Scope::NORMAL,
            |app, _key| {
                if matches!(
                    app.settings.mouse_mode,
                    MouseMode::Simple | MouseMode::Smart
                ) {
                    app.toggle_mouse_state("Escape pressed");
                }
            },
        ),
        Action::new(
            "exit",
            "Exit the application",
            Scope::NORMAL,
            |app, _key| {
                if app.buffer.buffer().is_empty() && unsafe { bash_symbols::ignoreeof != 0 } {
                    app.mode = crate::app::AppRunningState::Exiting(crate::app::ExitState::EOF);
                } else {
                    app.buffer.delete_forwards();
                }
            },
        ),
        Action::new(
            "cancel",
            "Cancel the current command or exit if no command is running",
            Scope::NORMAL,
            |app, _key| {
                app.mode =
                    crate::app::AppRunningState::Exiting(crate::app::ExitState::WithoutCommand);
            },
        ),
        Action::new(
            "comment_line",
            "Comment out the current line and submit",
            Scope::NORMAL,
            |app, _key| {
                app.buffer.move_to_start();
                app.buffer.insert_str("#");
                app.try_submit_current_buffer();
            },
        ),
        Action::new(
            "toggle_fuzzy_history_search",
            "Toggle fuzzy search through command history",
            Scope::NORMAL | Scope::FUZZY_HISTORY_SEARCH, // TODO: allow multiple scopes her
            |app, _key| {
                if matches!(app.content_mode, ContentMode::FuzzyHistorySearch) {
                    app.content_mode = ContentMode::Normal;
                } else {
                    app.content_mode = ContentMode::FuzzyHistorySearch;
                    app.history_manager
                        .warm_fuzzy_search_cache(app.buffer.buffer());
                }
            },
        ),
        Action::new(
            "clear_screen",
            "Clear the screen",
            Scope::NORMAL,
            |app, _key| {
                app.needs_screen_cleared = true;
            },
        ),
        Action::new(
            "delete_until_start_of_line",
            "Delete until start of line",
            Scope::NORMAL,
            |app, _key| app.buffer.delete_until_start_of_line(),
        ),
        Action::new(
            "delete_one_word_left",
            "Delete one word to the left",
            Scope::NORMAL,
            |app, _key| app.buffer.delete_one_word_left(WordDelim::LessStrict),
        ),
        Action::new(
            "delete_one_word_left_whitespace",
            "Delete one word to the left, using whitespace as delimiter",
            Scope::NORMAL,
            |app, _key| app.buffer.delete_one_word_left(WordDelim::WhiteSpace),
        ),
        Action::new(
            "delete_backwards",
            "Delete character before cursor",
            Scope::NORMAL,
            |app, _key| {
                if app.settings.auto_close_chars {
                    // Backspace: if the char to the right of the cursor is an auto-inserted closing token
                    // paired with the char about to be deleted, remove it as well.
                    app.delete_auto_inserted_closing_if_present();
                }
                app.buffer.delete_backwards()
            },
        ),
        Action::new(
            "delete_until_end_of_line",
            "Delete until end of line",
            Scope::NORMAL,
            |app, _key| app.buffer.delete_until_end_of_line(),
        ),
        Action::new(
            "delete_one_word_right",
            "Delete one word to the right",
            Scope::NORMAL,
            |app, _key| app.buffer.delete_one_word_right(WordDelim::LessStrict),
        ),
        Action::new(
            "delete_one_word_right_whitespace",
            "Delete one word to the right, using whitespace as delimiter",
            Scope::NORMAL,
            |app, _key| app.buffer.delete_one_word_right(WordDelim::WhiteSpace),
        ),
        Action::new(
            "delete_forwards",
            "Delete character after cursor",
            Scope::NORMAL,
            |app, _key| app.buffer.delete_forwards(),
        ),
        Action::new(
            "move_start_of_line",
            "Move cursor to start of line",
            Scope::NORMAL,
            |app, _key| app.buffer.move_start_of_line(),
        ),
        Action::new(
            "move_one_word_left_whitespace",
            "Move one word left, using whitespace as delimiter",
            Scope::NORMAL,
            |app, _key| app.buffer.move_one_word_left(WordDelim::WhiteSpace),
        ),
        Action::new(
            "move_left",
            "Move cursor left",
            Scope::NORMAL,
            |app, _key| app.buffer.move_left(),
        ),
        Action::new(
            "move_end_of_line",
            "Move cursor to end of line",
            Scope::NORMAL,
            |app, _key| app.buffer.move_end_of_line(),
        ),
        Action::new(
            "move_one_word_right_whitespace",
            "Move one word right, using whitespace as delimiter",
            Scope::NORMAL,
            |app, _key| app.buffer.move_one_word_right(WordDelim::WhiteSpace),
        ),
        Action::new(
            "move_right",
            "Move cursor right",
            Scope::NORMAL,
            |app, _key| app.buffer.move_right(),
        ),
        Action::new(
            "move_line_up_or_history_up",
            "Move cursor up one line or navigate history if on the first buffer line",
            Scope::NORMAL,
            |app, _key| {
                if app.buffer.cursor_row() == 0 {
                    app.buffer_before_history_navigation
                        .get_or_insert_with(|| app.buffer.buffer().to_string());
                    if let Some(entry) = app
                        .history_manager
                        .search_in_history(app.buffer.buffer(), HistorySearchDirection::Backward)
                    {
                        app.buffer.replace_buffer(&entry.command);
                    }
                } else {
                    app.buffer.move_line_up()
                }
            },
        ),
        Action::new(
            "move_line_down_or_history_down",
            "Move cursor down one line or navigate history if the on final buffer line",
            Scope::NORMAL,
            |app, _key| {
                if app.buffer.is_cursor_on_final_line() {
                    match app
                        .history_manager
                        .search_in_history(app.buffer.buffer(), HistorySearchDirection::Forward)
                    {
                        Some(entry) => {
                            app.buffer.replace_buffer(&entry.command);
                        }
                        None => {
                            if let Some(original_buffer) =
                                app.buffer_before_history_navigation.take()
                            {
                                app.buffer.replace_buffer(&original_buffer);
                            }
                        }
                    }
                } else {
                    app.buffer.move_line_down()
                }
            },
        ),
        Action::new("undo", "Undo last action", Scope::NORMAL, |app, _key| {
            app.buffer.undo()
        }),
        Action::new("redo", "Redo last action", Scope::NORMAL, |app, _key| {
            app.buffer.redo()
        }),
        Action::new(
            "insert_char",
            "Insert character",
            Scope::NORMAL,
            |app, key| {
                if let KeyCode::Char(c) = key.code {
                    if app.settings.auto_close_chars {
                        app.last_keypress_action = app.handle_char_insertion(c);
                    } else {
                        app.buffer.insert_char(c);
                    }
                }
            },
        ),
    ]
});

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
/// On iterm2, there is a seetitng in Porfiles->Keys->Left option key.
/// Choices are Normal or  (Set high bit (not recommended) or Esc+).
/// Set high bit gives you a warning: "You have chosen to have an option key as Meta. This is
/// useful for backward compatibility with old applications. The "Esc+" option is recommended for most users"
/// In text_buffer.rs, I check if either of them are set for maximal compatibility.
/// From highest priority to lowest
static DEFAULT_BINDINGS: LazyLock<[Binding; 48]> = LazyLock::new(|| {
    [
        Binding::try_new(
            &["Right", "End"],
            Scope::INLINE_HISTORY_ACCEPTABLE,
            "accept_suggestion",
        )
        .unwrap(),
        Binding::try_new(
            &["Down"],
            Scope::AGENT_OUTPUT_SELECTION,
            "move_down_in_agent_output_selection",
        )
        .unwrap(),
        Binding::try_new(
            &["Up"],
            Scope::AGENT_OUTPUT_SELECTION,
            "move_up_in_agent_output_selection",
        )
        .unwrap(),
        Binding::try_new(&["Up"], Scope::TAB_COMPLETION, "move_up").unwrap(),
        Binding::try_new(&["Down"], Scope::TAB_COMPLETION, "move_down").unwrap(),
        Binding::try_new(&["Left"], Scope::TAB_COMPLETION, "move_left").unwrap(),
        Binding::try_new(&["Right"], Scope::TAB_COMPLETION, "move_right").unwrap(),
        Binding::try_new(&["Up"], Scope::FUZZY_HISTORY_SEARCH, "history_search_up").unwrap(),
        Binding::try_new(
            &["Down", "Ctrl+s"],
            Scope::FUZZY_HISTORY_SEARCH,
            "history_search_down",
        )
        .unwrap(),
        Binding::try_new(&["PageUp"], Scope::FUZZY_HISTORY_SEARCH, "page_up").unwrap(),
        Binding::try_new(&["PageDown"], Scope::FUZZY_HISTORY_SEARCH, "page_down").unwrap(),
        Binding::try_new(&["Alt+Enter"], Scope::NORMAL, "run_agent_mode").unwrap(),
        Binding::try_new(
            &[
                "Enter",
                "Ctrl+j", // Without this, when I hold enter, sometimes 'j' is read as input
            ],
            Scope::FUZZY_HISTORY_SEARCH,
            "accept_entry",
        )
        .unwrap(),
        Binding::try_new(&["Enter", "Ctrl+j"], Scope::TAB_COMPLETION, "accept_entry").unwrap(),
        Binding::try_new(&["Enter", "Ctrl+j"], Scope::AGENT_ERROR, "run_help_command").unwrap(),
        Binding::try_new(
            &["Enter", "Ctrl+j"],
            Scope::AGENT_OUTPUT_SELECTION,
            "accept_entry",
        )
        .unwrap(),
        Binding::try_new(
            &["Enter", "Ctrl+j"],
            Scope::NORMAL,
            "submit_or_newline", // TODO name
        )
        .unwrap(),
        Binding::try_new(
            &["Shift+Tab", "Backtab"], // TODO backtab and shift tab for agent output selection
            Scope::TAB_COMPLETION,
            "prev_suggestion",
        )
        .unwrap(),
        Binding::try_new(&["Tab"], Scope::FUZZY_HISTORY_SEARCH, "accept_and_edit").unwrap(),
        Binding::try_new(&["Tab"], Scope::AGENT_OUTPUT_SELECTION, "next_suggestion").unwrap(),
        Binding::try_new(&["Tab"], Scope::TAB_COMPLETION, "next_suggestion").unwrap(),
        Binding::try_new(&["Tab"], Scope::NORMAL, "trigger_tab_completion").unwrap(),
        Binding::try_new(&["Esc"], Scope::NORMAL, "escape_to_normal_mode").unwrap(),
        Binding::try_new(&["Esc"], Scope::NORMAL, "toggle_mouse").unwrap(),
        Binding::try_new(&["Ctrl+d"], Scope::NORMAL, "exit").unwrap(),
        Binding::try_new(&["Ctrl+c", "Meta+c"], Scope::NORMAL, "cancel").unwrap(),
        Binding::try_new(
            // Ctrl+/ (shows as Ctrl+7) - comment out and execute
            &["Ctrl+/", "Meta+/", "Super+/", "Ctrl+7"],
            Scope::NORMAL,
            "comment_line",
        )
        .unwrap(),
        Binding::try_new(
            &["ctrl+r", "meta+r"],
            Scope::NORMAL | Scope::FUZZY_HISTORY_SEARCH, // TODO: allow multiple scopes her
            "toggle_fuzzy_history_search",
        )
        .unwrap(),
        Binding::try_new(&["Ctrl+l"], Scope::NORMAL, "clear_screen").unwrap(),
        Binding::try_new(
            &["Super+Backspace", "Ctrl+u", "Ctrl+Shift+Backspace"],
            Scope::NORMAL,
            "delete_until_start_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &["Alt+Backspace", "Meta+Backspace"],
            Scope::NORMAL,
            "delete_one_word_left",
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Backspace", "Ctrl+H", "Alt+W", "Ctrl+w", "Meta+W"],
            Scope::NORMAL,
            "delete_one_word_left_whitespace",
        )
        .unwrap(),
        Binding::try_new(&["Backspace"], Scope::NORMAL, "delete_backwards").unwrap(),
        Binding::try_new(
            &["Super+Delete", "Ctrl+Shift+Delete", "Ctrl+k"],
            Scope::NORMAL,
            "delete_until_end_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &["Alt+Delete", "Meta+Delete"],
            Scope::NORMAL,
            "delete_one_word_right",
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Delete", "Alt+D", "Meta+D"],
            Scope::NORMAL,
            "delete_one_word_right_whitespace",
        )
        .unwrap(),
        Binding::try_new(&["Delete"], Scope::NORMAL, "delete_forwards").unwrap(),
        Binding::try_new(
            &["Home", "Super+Left", "Ctrl+A", "Super+A"],
            Scope::NORMAL,
            "move_start_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Left", "Alt+Left", "Meta+Left", "Alt+b", "Meta+b"], // Emacs-style. ghostty sends this for Alt+Left by default
            Scope::NORMAL,
            "move_one_word_left_whitespace",
        )
        .unwrap(),
        Binding::try_new(&["Left"], Scope::NORMAL, "move_left").unwrap(),
        Binding::try_new(
            &["End", "Super+Right", "Ctrl+E", "Super+E"],
            Scope::NORMAL,
            "move_end_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Right", "Alt+Right", "Meta+Right", "Alt+f", "Meta+f"], // Emacs-style. ghostty sends Alt+Right as Meta+Right by default
            Scope::NORMAL,
            "move_one_word_right_whitespace",
        )
        .unwrap(),
        Binding::try_new(&["Right"], Scope::NORMAL, "move_right").unwrap(),
        Binding::try_new(&["Up"], Scope::NORMAL, "move_line_up_or_history_up").unwrap(),
        Binding::try_new(&["Down"], Scope::NORMAL, "move_line_down_or_history_down").unwrap(),
        Binding::try_new(&["Ctrl+z", "Super+Shift+Z"], Scope::NORMAL, "undo").unwrap(),
        Binding::try_new(&["Ctrl+y", "Super+Shift+Z"], Scope::NORMAL, "redo").unwrap(),
        Binding::try_new(&["AnyChar", "Shift+AnyChar"], Scope::NORMAL, "insert_char").unwrap(),
    ]
});

impl<'a> App<'a> {
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        log::trace!("Key event: {:?}", key);

        self.last_keypress_action = None; // reset last keypress action, to be set by specific actions as needed

        // Smart mode: any keypress re-enables mouse capture, unless the user has
        // explicitly disabled it via a toggle action.
        if self.settings.mouse_mode == MouseMode::Smart
            && !self.mouse_state.is_explicitly_disabled_by_user()
        {
            self.mouse_state.enable("smart mode: keypress detected");
        }

        for binding in DEFAULT_BINDINGS.iter() {
            if binding.action.scope.is_active(self) && binding.matches(key) {
                log::trace!("Matched binding: {}", binding.action.name);
                (binding.action.action)(self, key);
                break;
            }
        }

        self.on_possible_buffer_change();
    }
}
