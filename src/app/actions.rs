use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::LazyLock;
use crate::text_buffer::{WordDelim, TextBuffer};
use crate::app::App;

#[derive(Clone, Debug)]
pub enum Scope {
    Normal,
    FuzzyHistorySearch,
    TabCompletion,
    AgentMode,
    AgentOutputSelection,
    InlineHistorySuggestion,
}

impl Scope {
    pub fn is_active(&self, app: &App) -> bool {
        match self {
            Scope::Normal => true,
            Scope::FuzzyHistorySearch => matches!(
                app.content_mode,
                crate::app::ContentMode::FuzzyHistorySearch
            ),
            Scope::TabCompletion => matches!(
                app.content_mode,
                crate::app::ContentMode::TabCompletion { .. }
            ),
            Scope::AgentMode => {
                matches!(app.content_mode, crate::app::ContentMode::AgentMode { .. })
            }
            Scope::AgentOutputSelection => matches!(
                app.content_mode,
                crate::app::ContentMode::AgentOutputSelection { .. }
            ),
            Scope::InlineHistorySuggestion => app.inline_history_suggestion.is_some(),
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
                "tab" => KeyCode::Tab,
                "delete" => KeyCode::Delete,
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
    pub fn try_new(key_events: &[&str], action: Action) -> Result<Self> {
        let mut events = Vec::new();
        for &key_event in key_events {
            events.push(KeyEventMatch::try_from(key_event)?);
        }
        Ok(Self {
            key_events: events,
            action,
        })
    }

    pub fn matches(&self, key: KeyEvent) -> bool {
        self.key_events.iter().any(|k| match k {
            KeyEventMatch::Exact(k_event) => {
                k_event.code == key.code && k_event.modifiers.contains(key.modifiers)
            }
            KeyEventMatch::AnyCharEitherMod(mods) => {
                matches!(key.code, KeyCode::Char(_))
                    && mods.iter().any(|m| key.modifiers.contains(*m))
            }
        })
    }
}

// Handle basic text editing keypresses
// Useful reference:
// https://en.wikipedia.org/wiki/Table_of_keyboard_shortcuts#Command_line_shortcuts
// From highest priority to lowest
static DEFAULT_BINDINGS: LazyLock<[Binding; 19]> = LazyLock::new(|| {
    [
        Binding::try_new(
            &["Super+Backspace", "Ctrl+u", "Ctrl+Shift+Backspace"],
            Action::new(
                "delete_until_start_of_line",
                "Delete until start of line",
                Scope::Normal,
                |app, _key| app.buffer.delete_until_start_of_line(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Alt+Backspace", "Meta+Backspace"],
            Action::new(
                "delete_one_word_left",
                "Delete one word to the left",
                Scope::Normal,
                |app, _key| app.buffer.delete_one_word_left(WordDelim::LessStrict),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Backspace", "Ctrl+H", "Alt+W", "Ctrl+w", "Meta+W"],
            Action::new(
                "delete_one_word_left_whitespace",
                "Delete one word to the left, using whitespace as delimiter",
                Scope::Normal,
                |app, _key| app.buffer.delete_one_word_left(WordDelim::WhiteSpace),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Backspace"],
            Action::new(
                "delete_backwards",
                "Delete character before cursor",
                Scope::Normal,
                |app, _key| app.buffer.delete_backwards(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Super+Delete", "Ctrl+Shift+Delete", "Ctrl+k"],
            Action::new(
                "delete_until_end_of_line",
                "Delete until end of line",
                Scope::Normal,
                |app, _key| app.buffer.delete_until_end_of_line(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Alt+Delete", "Meta+Delete"],
            Action::new(
                "delete_one_word_right",
                "Delete one word to the right",
                Scope::Normal,
                |app, _key| app.buffer.delete_one_word_right(WordDelim::LessStrict),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Delete", "Alt+D", "Meta+D"],
            Action::new(
                "delete_one_word_right_whitespace",
                "Delete one word to the right, using whitespace as delimiter",
                Scope::Normal,
                |app, _key| app.buffer.delete_one_word_right(WordDelim::WhiteSpace),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Delete"],
            Action::new(
                "delete_forwards",
                "Delete character after cursor",
                Scope::Normal,
                |app, _key| app.buffer.delete_forwards(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Home", "Super+Left", "Ctrl+A", "Super+A"],
            Action::new(
                "move_start_of_line",
                "Move cursor to start of line",
                Scope::Normal,
                |app, _key| app.buffer.move_start_of_line(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Left", "Alt+Left", "Meta+Left", "Alt+b", "Meta+b"], // Emacs-style. ghostty sends this for Alt+Left by default
            Action::new(
                "move_one_word_left_whitespace",
                "Move one word left, using whitespace as delimiter",
                Scope::Normal,
                |app, _key| app.buffer.move_one_word_left(WordDelim::WhiteSpace),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Left"],
            Action::new(
                "move_left",
                "Move cursor left",
                Scope::Normal,
                |app, _key| app.buffer.move_left(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["End", "Super+Right", "Ctrl+E", "Super+E"],
            Action::new(
                "move_end_of_line",
                "Move cursor to end of line",
                Scope::Normal,
                |app, _key| app.buffer.move_end_of_line(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Right", "Alt+Right", "Meta+Right", "Alt+f", "Meta+f"], // Emacs-style. ghostty sends Alt+Right as Meta+Right by default
            Action::new(
                "move_one_word_right_whitespace",
                "Move one word right, using whitespace as delimiter",
                Scope::Normal,
                |app, _key| app.buffer.move_one_word_right(WordDelim::WhiteSpace),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Right"],
            Action::new(
                "move_right",
                "Move cursor right",
                Scope::Normal,
                |app, _key| app.buffer.move_right(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Up"],
            Action::new(
                "move_line_up",
                "Move cursor up one line",
                Scope::Normal,
                |app, _key| app.buffer.move_line_up(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Down"],
            Action::new(
                "move_line_down",
                "Move cursor down one line",
                Scope::Normal,
                |app, _key| app.buffer.move_line_down(),
            ),
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+z", "Super+Shift+Z"],
            Action::new("undo", "Undo last action", Scope::Normal, |app, _key| {
                app.buffer.undo()
            }),
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+y", "Super+Shift+Z"],
            Action::new("redo", "Redo last action", Scope::Normal, |app, _key| {
                app.buffer.redo()
            }),
        )
        .unwrap(),
        Binding::try_new(
            &["AnyChar", "Shift+AnyChar"],
            Action::new(
                "insert_char",
                "Insert character",
                Scope::Normal,
                |app, key| {
                    if let KeyCode::Char(c) = key.code {
                        app.buffer.insert_char(c);
                    }
                },
            ),
        )
        .unwrap(),
    ]
});

impl<'a> App<'a> {
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        for binding in DEFAULT_BINDINGS.iter() {
            if binding.action.scope.is_active(self) && binding.matches(key) {
                log::trace!("Matched binding: {}", binding.action.name);
                (binding.action.action)(self, key);
                break;
            }
        }
    }
}
