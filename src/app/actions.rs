use crate::app::{App, ContentMode, FuzzyHistorySource};
use crate::bash_symbols;
use crate::history::HistorySearchDirection;
use crate::settings::MouseMode;
use crate::text_buffer::WordDelim;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Any,
    FuzzyHistorySearch,
    TabCompletionWaiting,
    TabCompletion,
    AgentModeWaiting,
    AgentOutputSelection,
    AgentError,
    InlineHistoryAcceptable,
    PromptDirSelect,
}

impl Scope {
    pub fn is_active(&self, app: &App) -> bool {
        match self {
            Scope::Any => true,
            Scope::FuzzyHistorySearch => matches!(
                app.content_mode,
                crate::app::ContentMode::FuzzyHistorySearch(_)
            ),
            Scope::TabCompletionWaiting => matches!(
                app.content_mode,
                crate::app::ContentMode::TabCompletionWaiting { .. }
            ),
            Scope::TabCompletion => matches!(
                app.content_mode,
                crate::app::ContentMode::TabCompletion { .. }
            ),
            Scope::AgentModeWaiting => matches!(
                app.content_mode,
                crate::app::ContentMode::AgentModeWaiting { .. }
            ),
            Scope::AgentOutputSelection => matches!(
                app.content_mode,
                crate::app::ContentMode::AgentOutputSelection { .. }
            ),
            Scope::AgentError => {
                matches!(app.content_mode, crate::app::ContentMode::AgentError { .. })
            }
            Scope::InlineHistoryAcceptable => {
                app.buffer.is_cursor_at_end() && app.inline_history_suggestion.is_some()
            }
            Scope::PromptDirSelect => {
                matches!(
                    app.content_mode,
                    crate::app::ContentMode::PromptDirSelect(_)
                )
            }
        }
    }
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        match self {
            Scope::Any => "any",
            Scope::FuzzyHistorySearch => "fuzzy_history_search",
            Scope::TabCompletionWaiting => "tab_completion_waiting",
            Scope::TabCompletion => "tab_completion",
            Scope::AgentModeWaiting => "agent_mode_waiting",
            Scope::AgentOutputSelection => "agent_output_selection",
            Scope::AgentError => "agent_error",
            Scope::InlineHistoryAcceptable => "inline_history_acceptable",
            Scope::PromptDirSelect => "prompt_dir_select",
        }
    }
}

impl TryFrom<&str> for Scope {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "any" => Ok(Scope::Any),
            "fuzzy_history_search" => Ok(Scope::FuzzyHistorySearch),
            "tab_completion_waiting" => Ok(Scope::TabCompletionWaiting),
            "tab_completion" => Ok(Scope::TabCompletion),
            "agent_mode_waiting" => Ok(Scope::AgentModeWaiting),
            "agent_output_selection" => Ok(Scope::AgentOutputSelection),
            "agent_error" => Ok(Scope::AgentError),
            "inline_history_acceptable" => Ok(Scope::InlineHistoryAcceptable),
            "prompt_dir_select" => Ok(Scope::PromptDirSelect),
            other => Err(anyhow::anyhow!("Unknown scope: '{}'", other)),
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
    pub const fn new(
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

    pub fn scoped_action_name(&self) -> String {
        format!("{}::{}", self.scope.as_ref(), self.name)
    }
}

#[derive(Debug, Clone)]
pub enum KeyEventMatch {
    Exact(KeyEvent),
    AnyCharAndMods(Vec<KeyModifiers>),
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
            return Ok(KeyEventMatch::AnyCharAndMods(vec![modifiers]));
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
}

/// Parse a single key-code name (no modifiers) into a [`KeyCode`].
fn parse_single_keycode(s: &str) -> Result<KeyCode> {
    use crossterm::event::{MediaKeyCode, ModifierKeyCode};
    let s = s.trim();
    if s.len() == 1 {
        return Ok(KeyCode::Char(s.chars().next().unwrap()));
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

/// Parse a single modifier name into a single-bit [`KeyModifiers`] value.
fn parse_single_modifier(s: &str) -> Result<KeyModifiers> {
    match s.to_lowercase().as_str() {
        "ctrl" | "control" => Ok(KeyModifiers::CONTROL),
        "shift" => Ok(KeyModifiers::SHIFT),
        "alt" | "option" => Ok(KeyModifiers::ALT),
        "meta" => Ok(KeyModifiers::META),
        "super" | "cmd" | "command" | "gui" | "win" => Ok(KeyModifiers::SUPER),
        "hyper" => Ok(KeyModifiers::HYPER),
        _ => Err(anyhow::anyhow!("Unknown modifier: '{}'", s)),
    }
}

/// Parse and validate a remap pair (from, to).  Modifiers may only be remapped
/// to modifiers; keys may only be remapped to keys.
pub fn try_parse_remap(from: &str, to: &str) -> Result<KeyRemap> {
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

    // Modifier remaps are applied simultaneously from the original modifier set.
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
            .ok_or_else(|| {
                println!(
                    "Unknown action name '{}' for scope '{}'",
                    action_name,
                    scope.as_ref()
                );
                anyhow::anyhow!("Unknown action: '{}'", action_name)
            })?;

        Ok(Self {
            key_events: events,
            action,
        })
    }

    pub fn try_new_from_strs(key_event: &str, scope_and_action: &str) -> Result<Self> {
        let parts = scope_and_action.split("::").collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid scope and action format: '{}'. Expected 'scope::action'",
                scope_and_action
            ));
        }
        let scope_str = parts[0];
        let scope = Scope::try_from(scope_str)?;

        let action_str = parts[1];

        Ok(Self::try_new(&[key_event], scope, action_str)?)
    }

    pub fn matches(&self, key: KeyEvent) -> bool {
        self.key_events.iter().any(|k| match k {
            KeyEventMatch::Exact(action_binding) => {
                action_binding.code == key.code && key.modifiers.contains(action_binding.modifiers)
            }
            KeyEventMatch::AnyCharAndMods(mods) => {
                matches!(key.code, KeyCode::Char(_))
                    && mods.iter().any(|m| key.modifiers.contains(*m))
            }
        })
    }
}

/// Internal helper for [`expand_variations!`].
///
/// Pushes all terminal-equivalent spellings for a single key literal into
/// `$v: Vec<&'static str>`.  Both the canonical casing used in the default
/// bindings and a fully-lowercase alias are listed for each rule so that
/// callers are case-insensitive.
macro_rules! expand_variation_push {
    // ── Enter ─────────────────────────────────────────────────────────────
    // Ctrl+j is the ASCII LF (line-feed) code, identical to Enter in most
    // terminals.
    ($v:ident, "Enter") => {
        $v.extend_from_slice(&["Enter", "Ctrl+j"]);
    };
    ($v:ident, "enter") => {
        $v.extend_from_slice(&["Enter", "Ctrl+j"]);
    };
    // ── Word-left group: Alt+Left / Alt+b / Meta+Left / Meta+b ────────────
    // Alt+b is the Emacs backward-word shortcut; ghostty and other modern
    // terminal emulators send Meta+Left for the same key chord.
    ($v:ident, "Alt+Left") => {
        $v.extend_from_slice(&["Alt+Left", "Alt+b", "Meta+Left", "Meta+b"]);
    };
    ($v:ident, "alt+left") => {
        $v.extend_from_slice(&["Alt+Left", "Alt+b", "Meta+Left", "Meta+b"]);
    };
    ($v:ident, "Meta+Left") => {
        $v.extend_from_slice(&["Meta+Left", "Meta+b", "Alt+Left", "Alt+b"]);
    };
    ($v:ident, "meta+left") => {
        $v.extend_from_slice(&["Meta+Left", "Meta+b", "Alt+Left", "Alt+b"]);
    };
    ($v:ident, "Alt+b") => {
        $v.extend_from_slice(&["Alt+b", "Alt+Left", "Meta+b", "Meta+Left"]);
    };
    ($v:ident, "alt+b") => {
        $v.extend_from_slice(&["Alt+b", "Alt+Left", "Meta+b", "Meta+Left"]);
    };
    ($v:ident, "Meta+b") => {
        $v.extend_from_slice(&["Meta+b", "Meta+Left", "Alt+b", "Alt+Left"]);
    };
    ($v:ident, "meta+b") => {
        $v.extend_from_slice(&["Meta+b", "Meta+Left", "Alt+b", "Alt+Left"]);
    };
    // ── Word-right group: Alt+Right / Alt+f / Meta+Right / Meta+f ─────────
    // Alt+f is the Emacs forward-word shortcut.
    ($v:ident, "Alt+Right") => {
        $v.extend_from_slice(&["Alt+Right", "Alt+f", "Meta+Right", "Meta+f"]);
    };
    ($v:ident, "alt+right") => {
        $v.extend_from_slice(&["Alt+Right", "Alt+f", "Meta+Right", "Meta+f"]);
    };
    ($v:ident, "Meta+Right") => {
        $v.extend_from_slice(&["Meta+Right", "Meta+f", "Alt+Right", "Alt+f"]);
    };
    ($v:ident, "meta+right") => {
        $v.extend_from_slice(&["Meta+Right", "Meta+f", "Alt+Right", "Alt+f"]);
    };
    ($v:ident, "Alt+f") => {
        $v.extend_from_slice(&["Alt+f", "Alt+Right", "Meta+f", "Meta+Right"]);
    };
    ($v:ident, "alt+f") => {
        $v.extend_from_slice(&["Alt+f", "Alt+Right", "Meta+f", "Meta+Right"]);
    };
    ($v:ident, "Meta+f") => {
        $v.extend_from_slice(&["Meta+f", "Meta+Right", "Alt+f", "Alt+Right"]);
    };
    ($v:ident, "meta+f") => {
        $v.extend_from_slice(&["Meta+f", "Meta+Right", "Alt+f", "Alt+Right"]);
    };
    // ── Alt+X  →  also Meta+X (Alt/Meta terminal equivalence) ────────────
    ($v:ident, "Alt+Enter") => {
        $v.extend_from_slice(&["Alt+Enter", "Meta+Enter"]);
    };
    ($v:ident, "alt+enter") => {
        $v.extend_from_slice(&["Alt+Enter", "Meta+Enter"]);
    };
    ($v:ident, "Alt+Backspace") => {
        $v.extend_from_slice(&["Alt+Backspace", "Meta+Backspace"]);
    };
    ($v:ident, "alt+backspace") => {
        $v.extend_from_slice(&["Alt+Backspace", "Meta+Backspace"]);
    };
    ($v:ident, "Alt+Delete") => {
        $v.extend_from_slice(&["Alt+Delete", "Meta+Delete", "Alt+d", "Meta+d"]);
    };
    ($v:ident, "alt+delete") => {
        $v.extend_from_slice(&["Alt+Delete", "Meta+Delete", "Alt+d", "Meta+d"]);
    };
    ($v:ident, "Alt+D") => {
        $v.extend_from_slice(&["Alt+D", "Meta+D"]);
    };
    ($v:ident, "alt+d") => {
        $v.extend_from_slice(&["Alt+D", "Meta+D"]);
    };
    ($v:ident, "Alt+W") => {
        $v.extend_from_slice(&["Alt+W", "Meta+W"]);
    };
    ($v:ident, "alt+w") => {
        $v.extend_from_slice(&["Alt+W", "Meta+W"]);
    };
    // ── Home / End ────────────────────────────────────────────────────────
    // Ctrl+a (Emacs move-beginning-of-line) is treated as an alias for Home.
    // Ctrl+e (Emacs move-end-of-line) is treated as an alias for End.
    ($v:ident, "Home") => {
        $v.extend_from_slice(&["Home", "Ctrl+a"]);
    };
    ($v:ident, "home") => {
        $v.extend_from_slice(&["Home", "Ctrl+a"]);
    };
    ($v:ident, "End") => {
        $v.extend_from_slice(&["End", "Ctrl+e"]);
    };
    ($v:ident, "end") => {
        $v.extend_from_slice(&["End", "Ctrl+e"]);
    };
    // ── Shift+Tab / Backtab ───────────────────────────────────────────────
    // BackTab (Shift+Tab) is sent as either "Shift+Tab" or the dedicated
    // "Backtab" keycode depending on the terminal emulator.
    ($v:ident, "Shift+Tab") => {
        $v.extend_from_slice(&["Shift+Tab", "Backtab"]);
    };
    ($v:ident, "shift+tab") => {
        $v.extend_from_slice(&["Shift+Tab", "Backtab"]);
    };
    ($v:ident, "Backtab") => {
        $v.extend_from_slice(&["Backtab", "Shift+Tab"]);
    };
    ($v:ident, "backtab") => {
        $v.extend_from_slice(&["Backtab", "Shift+Tab"]);
    };
    // ── Fallthrough: pass through unchanged ───────────────────────────────
    ($v:ident, $key:literal) => {
        $v.push($key);
    };
}

/// Expand a list of keybinding key strings to include their common terminal
/// equivalents.
///
/// Returns a [`Vec<&'static str>`] that coerces to `&[&str]` via deref, so it
/// can be passed directly as `&expand_variations![...]` to
/// [`Binding::try_new`].
///
/// # Expansion rules
///
/// | Input            | Expands to                                          |
/// |------------------|-----------------------------------------------------|
/// | `"Enter"`        | `"Enter"`, `"Ctrl+j"`                               |
/// | `"Shift+Tab"`    | `"Shift+Tab"`, `"Backtab"`                          |
/// | `"Backtab"`      | `"Backtab"`, `"Shift+Tab"`                          |
/// | `"Alt+Left"`     | `"Alt+Left"`, `"Alt+b"`, `"Meta+Left"`, `"Meta+b"` |
/// | `"Alt+Right"`    | `"Alt+Right"`, `"Alt+f"`, `"Meta+Right"`, `"Meta+f"`|
/// | `"Meta+Left"`    | same four-way word-left group                       |
/// | `"Alt+b"` / `"Meta+b"` | same four-way word-left group               |
/// | `"Meta+Right"`   | same four-way word-right group                      |
/// | `"Alt+f"` / `"Meta+f"` | same four-way word-right group              |
/// | `"Alt+X"` (other)| `"Alt+X"`, `"Meta+X"`                               |
/// | anything else    | unchanged                                           |
///
/// # Example
///
/// ```ignore
/// // expand_variations!["Enter"]               →  ["Enter", "Ctrl+j"]
/// // expand_variations!["Shift+Tab"]           →  ["Shift+Tab", "Backtab"]
/// // expand_variations!["Alt+Left"]            →  ["Alt+Left", "Alt+b", "Meta+Left", "Meta+b"]
/// // expand_variations!["Ctrl+Left", "Alt+Left"] →  ["Ctrl+Left", "Alt+Left", "Alt+b", "Meta+Left", "Meta+b"]
/// ```
macro_rules! expand_variations {
    [$($key:literal),+ $(,)?] => {{
        let mut v: Vec<&'static str> = Vec::new();
        $(expand_variation_push!(v, $key);)+
        v
    }};
}

/// Build the [`POSSIBLE_ACTIONS`] slice from a list of [`Action::new`] calls,
/// where any entry written as `Action::expand_new([scope1, scope2, …], …)`
/// is automatically expanded into one [`Action::new`] per listed scope.
///
/// The output is an array literal that can be coerced to `&[Action]` in a
/// `const` context.
///
/// # Syntax
///
/// ```ignore
/// expand_actions![
///     // ordinary action — one scope already baked in
///     Action::new("name", "desc", Scope::Any, |app, _key| { /* … */ }),
///
///     // multi-scope action — expands to N Action::new entries
///     Action::expand_new(
///         [Scope::Any, Scope::FuzzyHistorySearch],
///         "name", "desc",
///         |app, _key| { /* … */ },
///     ),
/// ]
/// ```
macro_rules! expand_actions {
    // ── Base case: accumulator exhausted → produce the slice ──────────────
    (@acc [ $($acc:tt)* ]) => {
        &[ $($acc)* ]
    };

    // ── Action::expand_new with a following comma (not the last item) ─────
    (
        @acc [ $($acc:tt)* ]
        Action::expand_new(
            [$($scopes:expr),+ $(,)?],
            $name:literal,
            $desc:literal,
            $action:expr $(,)?
        ),
        $($rest:tt)*
    ) => {
        expand_actions!(@acc [
            $($acc)*
            $(Action::new($name, $desc, $scopes, $action),)+
        ] $($rest)*)
    };

    // ── Action::expand_new as the last item (optional trailing comma) ─────
    (
        @acc [ $($acc:tt)* ]
        Action::expand_new(
            [$($scopes:expr),+ $(,)?],
            $name:literal,
            $desc:literal,
            $action:expr $(,)?
        ) $(,)?
    ) => {
        expand_actions!(@acc [
            $($acc)*
            $(Action::new($name, $desc, $scopes, $action),)+
        ])
    };

    // ── Action::new with a following comma (not the last item) ───────────
    (
        @acc [ $($acc:tt)* ]
        Action::new( $($args:tt)* ),
        $($rest:tt)*
    ) => {
        expand_actions!(@acc [
            $($acc)*
            Action::new($($args)*),
        ] $($rest)*)
    };

    // ── Action::new as the last item (optional trailing comma) ───────────
    (
        @acc [ $($acc:tt)* ]
        Action::new( $($args:tt)* ) $(,)?
    ) => {
        expand_actions!(@acc [
            $($acc)*
            Action::new($($args)*),
        ])
    };

    // ── Public entry point ────────────────────────────────────────────────
    [ $($input:tt)* ] => {
        expand_actions!(@acc [] $($input)*)
    };
}

const POSSIBLE_ACTIONS: &[Action] = expand_actions![
    Action::new(
        "accept_suggestion",
        "Accept inline history suggestion",
        Scope::InlineHistoryAcceptable,
        |app, _key| {
            if let Some((_, suf)) = &app.inline_history_suggestion {
                app.buffer.insert_str(suf);
                app.buffer.move_to_end();
            }
        },
    ),
    Action::new(
        "select_next",
        "Move down in agent output selection",
        Scope::AgentOutputSelection,
        |app, _key| {
            if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                selection.move_down();
            }
        },
    ),
    Action::new(
        "select_prev",
        "Move up in agent output selection",
        Scope::AgentOutputSelection,
        |app, _key| {
            if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                selection.move_up();
            }
        },
    ),
    Action::new(
        "move_up",
        "Move up in tab completion suggestions",
        Scope::TabCompletion,
        |app, _key| {
            if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                active_suggestions.on_up_arrow();
            }
        },
    ),
    Action::new(
        "move_down",
        "Move down in tab completion suggestions",
        Scope::TabCompletion,
        |app, _key| {
            if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                active_suggestions.on_down_arrow(); // TODO combine this with tab?
            }
        },
    ),
    Action::new(
        "move_left",
        "Move left in tab completion suggestions",
        Scope::TabCompletion,
        |app, _key| {
            if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                active_suggestions.on_left_arrow();
            }
        },
    ),
    Action::new(
        "move_right",
        "Move right in tab completion suggestions",
        Scope::TabCompletion,
        |app, _key| {
            if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                active_suggestions.on_right_arrow();
            }
        },
    ),
    Action::new(
        "select_prev",
        "Scroll up through fuzzy history search results",
        Scope::FuzzyHistorySearch,
        |app, _key| {
            let source = match &app.content_mode {
                ContentMode::FuzzyHistorySearch(s) => s.clone(),
                _ => return,
            };
            app.select_fuzzy_history_manager_mut(&source)
                .fuzzy_search_onkeypress(HistorySearchDirection::Forward);
        },
    ),
    Action::new(
        "select_next",
        "Scroll down through fuzzy history search results",
        Scope::FuzzyHistorySearch,
        |app, _key| {
            let source = match &app.content_mode {
                ContentMode::FuzzyHistorySearch(s) => s.clone(),
                _ => return,
            };
            app.select_fuzzy_history_manager_mut(&source)
                .fuzzy_search_onkeypress(HistorySearchDirection::Backward);
        },
    ),
    Action::new(
        "scroll_page_up",
        "Scroll up one page",
        Scope::FuzzyHistorySearch,
        |app, _key| {
            let source = match &app.content_mode {
                ContentMode::FuzzyHistorySearch(s) => s.clone(),
                _ => return,
            };
            app.select_fuzzy_history_manager_mut(&source)
                .fuzzy_search_onkeypress(HistorySearchDirection::PageForward);
        },
    ),
    Action::new(
        "scroll_page_down",
        "Scroll down one page",
        Scope::FuzzyHistorySearch,
        |app, _key| {
            let source = match &app.content_mode {
                ContentMode::FuzzyHistorySearch(s) => s.clone(),
                _ => return,
            };
            app.select_fuzzy_history_manager_mut(&source)
                .fuzzy_search_onkeypress(HistorySearchDirection::PageBackward);
        },
    ),
    Action::new(
        "run_agent_mode",
        "Run the agent mode command",
        Scope::Any,
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
        Scope::FuzzyHistorySearch,
        |app, _key| {
            app.accept_fuzzy_history_search();
        },
    ),
    Action::new(
        "accept_entry",
        "Accept the currently selected suggestion",
        Scope::TabCompletion,
        |app, _key| {
            if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                active_suggestions.accept_selected_filtered_item(&mut app.buffer);
                app.content_mode = ContentMode::Normal;
            }
        },
    ),
    Action::new(
        "run_help_command",
        "Run the agent mode help command",
        Scope::AgentError,
        |app, _key| match &app.content_mode {
            ContentMode::AgentError {
                suggested_buffer: Some(buf),
                ..
            } => {
                let buf = buf.clone();
                app.buffer.replace_buffer(&buf);
                app.on_possible_buffer_change();
                app.content_mode = ContentMode::Normal;
                if let Some((agent_cmd, buffer)) = app.resolve_agent_command(true) {
                    app.start_agent_mode(agent_cmd, &buffer);
                }
            }
            ContentMode::AgentError { .. } => {
                app.content_mode = ContentMode::Normal;
                app.buffer.replace_buffer("flyline agent-mode --help");
                app.on_possible_buffer_change();
                app.try_submit_current_buffer();
            }
            _ => {}
        },
    ),
    Action::new(
        "accept_entry",
        "Accept the currently selected agent output",
        Scope::AgentOutputSelection,
        |app, _key| {
            if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                if let Some(cmd) = selection.selected_command() {
                    let cmd = cmd.to_string();
                    app.buffer.replace_buffer(&cmd);
                }
                app.content_mode = ContentMode::Normal;
            }
        },
    ),
    Action::new(
        "submit_or_newline", // TODO name
        "Submit the current command. Insert a newline if the buffer has unclosed \",',[,(.",
        Scope::Any,
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
        Scope::TabCompletion,
        |app, _key| {
            if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                active_suggestions.on_tab(true);
            }
        },
    ),
    Action::new(
        "accept_and_edit",
        "Accept the current fuzzy history search suggestion for editing",
        Scope::FuzzyHistorySearch,
        |app, _key| {
            app.accept_fuzzy_history_search();
        },
    ),
    Action::new(
        "next_suggestion",
        "Move to the next tab completion suggestion",
        Scope::AgentOutputSelection,
        |app, _key| {
            if let ContentMode::AgentOutputSelection(selection) = &mut app.content_mode {
                selection.move_down(); // TODO: cycle through
            }
        },
    ),
    Action::new(
        "next_suggestion",
        "Move to the next tab completion suggestion",
        Scope::TabCompletion,
        |app, _key| {
            if let ContentMode::TabCompletion(active_suggestions) = &mut app.content_mode {
                active_suggestions.on_tab(false);
            }
        },
    ),
    Action::new(
        "run_tab_completion",
        "Start tab completion",
        Scope::Any,
        |app, _key| app.start_tab_complete(),
    ),
    Action::new(
        "toggle_mouse",
        "Toggle mouse state (Simple and Smart modes)",
        Scope::Any,
        |app, _key| {
            if matches!(
                app.settings.mouse_mode,
                MouseMode::Simple | MouseMode::Smart
            ) {
                app.toggle_mouse_state("Escape pressed");
            }
        },
    ),
    Action::new("exit", "Exit the application", Scope::Any, |app, _key| {
        if app.buffer.buffer().is_empty() && unsafe { bash_symbols::ignoreeof != 0 } {
            app.mode = crate::app::AppRunningState::Exiting(crate::app::ExitState::EOF);
        } else {
            app.buffer.delete_right();
        }
    }),
    Action::new(
        "cancel",
        "Cancel the current command or exit if no command is running",
        Scope::Any,
        |app, _key| {
            let buf = app.buffer.buffer().to_string();
            if false && buf.is_empty() {
                // TODO think of good UX for this
                // Warm with "" to display all cancelled commands regardless of buffer.
                app.settings
                    .cancelled_command_history_manager
                    .warm_fuzzy_search_cache("");
                app.content_mode =
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::CancelledCommands);
            } else {
                if false {
                    app.settings
                        .cancelled_command_history_manager
                        .push_entry(buf);
                }
                app.mode =
                    crate::app::AppRunningState::Exiting(crate::app::ExitState::WithoutCommand);
            }
        },
    ),
    Action::new(
        "comment_line_submit",
        "Comment out the current line and submit",
        Scope::Any,
        |app, _key| {
            app.buffer.move_to_start();
            app.buffer.insert_str("#");
            app.try_submit_current_buffer();
        },
    ),
    Action::new(
        "run_fuzzy_history_search",
        "Start fuzzy search through command history",
        Scope::Any,
        |app, _key| {
            let history_buffer = app.buffer_for_history().to_owned();
            app.history_manager.warm_fuzzy_search_cache(&history_buffer);
            app.content_mode = ContentMode::FuzzyHistorySearch(FuzzyHistorySource::PastCommands);
        },
    ),
    Action::new(
        "clear_screen",
        "Clear the screen",
        Scope::Any,
        |app, _key| {
            app.needs_screen_cleared = true;
        },
    ),
    Action::new(
        "delete_left_until_start_of_line",
        "Delete until start of line",
        Scope::Any,
        |app, _key| app.buffer.delete_until_start_of_line(),
    ),
    Action::new(
        "delete_left_one_word_fine_grained",
        "Delete one word to the left stopping at punctuation or path segment boundaries",
        Scope::Any,
        |app, _key| app.buffer.delete_one_word_left(WordDelim::FineGrained),
    ),
    Action::new(
        "delete_left_one_word_whitespace",
        "Delete one word to the left, using whitespace as delimiter",
        Scope::Any,
        |app, _key| app.buffer.delete_one_word_left(WordDelim::WhiteSpace),
    ),
    Action::new(
        "delete_left",
        "Delete character before cursor",
        Scope::Any,
        |app, _key| {
            if app.settings.auto_close_chars {
                // Backspace: if the char to the right of the cursor is an auto-inserted closing token
                // paired with the char about to be deleted, remove it as well.
                app.delete_auto_inserted_closing_if_present();
            }
            app.buffer.delete_left()
        },
    ),
    Action::new(
        "delete_right_until_end_of_line",
        "Delete until end of line",
        Scope::Any,
        |app, _key| app.buffer.delete_until_end_of_line(),
    ),
    Action::new(
        "delete_right_one_word_fine_grained",
        "Delete one word to the right stopping at punctuation or path segment boundaries",
        Scope::Any,
        |app, _key| app.buffer.delete_right_one_word(WordDelim::FineGrained),
    ),
    Action::new(
        "delete_right_one_word_whitespace",
        "Delete one word to the right, using whitespace as delimiter",
        Scope::Any,
        |app, _key| app.buffer.delete_right_one_word(WordDelim::WhiteSpace),
    ),
    Action::new(
        "delete_right",
        "Delete character after cursor",
        Scope::Any,
        |app, _key| app.buffer.delete_right(),
    ),
    Action::new(
        "move_left_start_of_line",
        "Move cursor to start of line",
        Scope::Any,
        |app, _key| app.buffer.move_start_of_line(),
    ),
    Action::new(
        "move_left_one_word_whitespace",
        "Move one word left, using whitespace as delimiter",
        Scope::Any,
        |app, _key| app.buffer.move_one_word_left(WordDelim::WhiteSpace),
    ),
    Action::new(
        "move_left_one_word_fine_grained",
        "Move one word left, stopping at punctuation or path segment boundaries",
        Scope::Any,
        |app, _key| app.buffer.move_one_word_left_fine_grained(),
    ),
    Action::new("move_left", "Move cursor left", Scope::Any, |app, _key| {
        if app.buffer.cursor_byte_pos() == 0 && app.prompt_manager.cwd_display_segment_count() > 0 {
            app.content_mode = ContentMode::PromptDirSelect(0);
        } else {
            app.buffer.move_left();
        }
    }),
    Action::new(
        "move_right_end_of_line",
        "Move cursor to end of line",
        Scope::Any,
        |app, _key| app.buffer.move_end_of_line(),
    ),
    Action::new(
        "move_right_one_word_whitespace",
        "Move one word right, using whitespace as delimiter",
        Scope::Any,
        |app, _key| app.buffer.move_one_word_right(WordDelim::WhiteSpace),
    ),
    Action::new(
        "move_right_one_word_fine_grained",
        "Move one word right, stopping at punctuation or path segment boundaries",
        Scope::Any,
        |app, _key| app.buffer.move_one_word_right_fine_grained(),
    ),
    Action::new(
        "move_right",
        "Move cursor right",
        Scope::Any,
        |app, _key| app.buffer.move_right(),
    ),
    Action::new(
        "move_line_up_or_history_up",
        "Move cursor up one line or navigate history if on the first buffer line",
        Scope::Any,
        |app, _key| {
            if app.buffer.cursor_row() == 0 {
                app.buffer_before_history_navigation
                    .get_or_insert_with(|| app.buffer.buffer().to_string());
                let history_buffer = app.buffer_for_history().to_owned();
                if let Some(entry) = app
                    .history_manager
                    .search_in_history(&history_buffer, HistorySearchDirection::Backward)
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
        "Move cursor down one line or navigate history if on the final buffer line",
        Scope::Any,
        |app, _key| {
            if app.buffer.is_cursor_on_final_line() {
                let history_buffer = app.buffer_for_history().to_owned();
                match app
                    .history_manager
                    .search_in_history(&history_buffer, HistorySearchDirection::Forward)
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
            } else {
                app.buffer.move_line_down()
            }
        },
    ),
    Action::new("undo", "Undo last action", Scope::Any, |app, _key| {
        app.buffer.undo()
    }),
    Action::new("redo", "Redo last action", Scope::Any, |app, _key| {
        app.buffer.redo()
    }),
    Action::new("insert_char", "Insert character", Scope::Any, |app, key| {
        if let KeyCode::Char(c) = key.code {
            if app.settings.auto_close_chars {
                app.last_keypress_action = app.handle_char_insertion(c);
            } else {
                app.buffer.insert_char(c);
            }
        }
    }),
    // ── PromptCwdEdit actions ─────────────────────────────────────────
    Action::new(
        "move_left",
        "Navigate to the parent directory segment in the prompt",
        Scope::PromptDirSelect,
        |app, _key| {
            if let ContentMode::PromptDirSelect(ref mut index) = app.content_mode {
                let max_index = app
                    .prompt_manager
                    .cwd_display_segment_count()
                    .saturating_sub(1);
                if *index < max_index {
                    *index += 1;
                }
            }
        },
    ),
    Action::new(
        "move_right",
        "Navigate to the child directory segment or exit prompt CWD edit mode",
        Scope::PromptDirSelect,
        |app, _key| match app.content_mode {
            ContentMode::PromptDirSelect(0) => {
                app.content_mode = ContentMode::Normal;
            }
            ContentMode::PromptDirSelect(ref mut index) => {
                *index -= 1;
            }
            _ => {}
        },
    ),
    Action::new(
        "accept_entry",
        "Replace the buffer with `cd <selected path>` and exit prompt CWD edit mode",
        Scope::PromptDirSelect,
        |app, _key| {
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
        },
    ),
    Action::new(
        "cancel",
        "Exit prompt CWD edit mode without changing the buffer",
        Scope::PromptDirSelect,
        |app, _key| {
            if matches!(app.content_mode, ContentMode::PromptDirSelect(_)) {
                app.content_mode = ContentMode::Normal;
            }
        },
    ),
    Action::new(
        "move_to_start",
        "Move selection to the leftmost directory segment in the prompt",
        Scope::PromptDirSelect,
        |app, _key| {
            if let ContentMode::PromptDirSelect(ref mut index) = app.content_mode {
                *index = app
                    .prompt_manager
                    .cwd_display_segment_count()
                    .saturating_sub(1);
            }
        },
    ),
    Action::new(
        "move_to_end",
        "Move selection to the rightmost (current) directory segment in the prompt",
        Scope::PromptDirSelect,
        |app, _key| {
            if let ContentMode::PromptDirSelect(ref mut index) = app.content_mode {
                *index = 0;
            }
        },
    ),
    Action::expand_new(
        [
            Scope::Any,
            Scope::FuzzyHistorySearch,
            Scope::TabCompletionWaiting,
            Scope::TabCompletion,
            Scope::AgentModeWaiting,
            Scope::AgentOutputSelection,
            Scope::AgentError,
            Scope::InlineHistoryAcceptable,
            Scope::PromptDirSelect,
        ],
        "escape_to_normal_mode",
        "Return to the normal command editing mode",
        |app, _key| {
            app.content_mode = ContentMode::Normal;
        },
    ),
];

use clap::builder::PossibleValuesParser;

pub fn possible_action_names() -> PossibleValuesParser {
    let values = POSSIBLE_ACTIONS.iter().map(|a| {
        let s = a.scoped_action_name();
        Box::leak(s.into_boxed_str()) as &'static str
    });

    PossibleValuesParser::new(values)
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
/// On iterm2, there is a seetitng in Porfiles->Keys->Left option key.
/// Choices are Normal or  (Set high bit (not recommended) or Esc+).
/// Set high bit gives you a warning: "You have chosen to have an option key as Meta. This is
/// useful for backward compatibility with old applications. The "Esc+" option is recommended for most users"
/// In text_buffer.rs, I check if either of them are set for maximal compatibility.
/// From highest priority to lowest
static DEFAULT_BINDINGS: LazyLock<[Binding; 60]> = LazyLock::new(|| {
    [
        Binding::try_new(&["Down"], Scope::AgentOutputSelection, "select_next").unwrap(),
        Binding::try_new(&["Up"], Scope::AgentOutputSelection, "select_prev").unwrap(),
        Binding::try_new(&["Up"], Scope::TabCompletion, "move_up").unwrap(),
        Binding::try_new(&["Down"], Scope::TabCompletion, "move_down").unwrap(),
        Binding::try_new(&["Left"], Scope::TabCompletion, "move_left").unwrap(),
        Binding::try_new(&["Right"], Scope::TabCompletion, "move_right").unwrap(),
        Binding::try_new(&["Up"], Scope::FuzzyHistorySearch, "select_prev").unwrap(),
        Binding::try_new(
            &["Down", "Ctrl+s"],
            Scope::FuzzyHistorySearch,
            "select_next",
        )
        .unwrap(),
        Binding::try_new(&["PageUp"], Scope::FuzzyHistorySearch, "scroll_page_up").unwrap(),
        Binding::try_new(&["PageDown"], Scope::FuzzyHistorySearch, "scroll_page_down").unwrap(),
        Binding::try_new(
            &["ctrl+r", "meta+r"],
            Scope::FuzzyHistorySearch,
            "escape_to_normal_mode", // Stop fuzzy history search if active, otherwise escape to normal mode
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Alt+Enter"],
            Scope::Any,
            "run_agent_mode",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Enter"],
            Scope::FuzzyHistorySearch,
            "accept_entry",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Enter"],
            Scope::TabCompletion,
            "accept_entry",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Enter"],
            Scope::AgentError,
            "run_help_command",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Enter"],
            Scope::AgentOutputSelection,
            "accept_entry",
        )
        .unwrap(),
        // PromptCwdEdit Enter must appear before the Normal Enter binding.
        Binding::try_new(
            &expand_variations!["Enter"],
            Scope::PromptDirSelect,
            "accept_entry",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Enter"],
            Scope::Any,
            "submit_or_newline",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Shift+Tab"], // Shift+Tab and Backtab are equivalent
            Scope::TabCompletion,
            "prev_suggestion",
        )
        .unwrap(),
        // Scoped Esc bindings must appear before the Normal Esc binding.
        Binding::try_new(&["Tab"], Scope::FuzzyHistorySearch, "accept_and_edit").unwrap(),
        Binding::try_new(
            &expand_variations!["Shift+Tab"],
            Scope::AgentOutputSelection,
            "select_prev",
        )
        .unwrap(),
        Binding::try_new(&["Tab"], Scope::AgentOutputSelection, "next_suggestion").unwrap(),
        Binding::try_new(&["Tab"], Scope::TabCompletion, "next_suggestion").unwrap(),
        Binding::try_new(&["Tab"], Scope::Any, "run_tab_completion").unwrap(),
        // PromptCwdEdit Esc must appear before the Normal Esc binding.
        Binding::try_new(&["Esc"], Scope::PromptDirSelect, "cancel").unwrap(),
        Binding::try_new(&["Esc"], Scope::Any, "escape_to_normal_mode").unwrap(),
        Binding::try_new(&["Esc"], Scope::Any, "toggle_mouse").unwrap(),
        Binding::try_new(&["Ctrl+d"], Scope::Any, "exit").unwrap(),
        Binding::try_new(&["Ctrl+c", "Meta+c"], Scope::Any, "cancel").unwrap(),
        Binding::try_new(
            // Ctrl+/ (shows as Ctrl+7) - comment out and execute
            &["Ctrl+/", "Meta+/", "Super+/", "Ctrl+7"],
            Scope::Any,
            "comment_line_submit",
        )
        .unwrap(),
        Binding::try_new(
            &["ctrl+r", "meta+r"],
            Scope::Any,
            "run_fuzzy_history_search",
        )
        .unwrap(),
        Binding::try_new(&["Ctrl+l"], Scope::Any, "clear_screen").unwrap(),
        Binding::try_new(
            &["Super+Backspace", "Ctrl+u", "Ctrl+Shift+Backspace"],
            Scope::Any,
            "delete_left_until_start_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Alt+Backspace"],
            Scope::Any,
            "delete_left_one_word_fine_grained",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Ctrl+Backspace", "Ctrl+H", "Alt+W", "Ctrl+w"],
            Scope::Any,
            "delete_left_one_word_whitespace",
        )
        .unwrap(),
        Binding::try_new(&["Backspace"], Scope::Any, "delete_left").unwrap(),
        Binding::try_new(
            &["Super+Delete", "Ctrl+Shift+Delete", "Ctrl+k"],
            Scope::Any,
            "delete_right_until_end_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Alt+Delete"],
            Scope::Any,
            "delete_right_one_word_fine_grained",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Ctrl+Delete", "Alt+D"],
            Scope::Any,
            "delete_right_one_word_whitespace",
        )
        .unwrap(),
        Binding::try_new(&["Delete"], Scope::Any, "delete_right").unwrap(),
        // PromptCwdEdit Home/End/Alt+Left/Ctrl+Left/Alt+Right/Ctrl+Right must appear before
        // the corresponding Any/InlineHistoryAcceptable bindings.
        Binding::try_new(
            &expand_variations!["Home"],
            Scope::PromptDirSelect,
            "move_to_start",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["End"],
            Scope::PromptDirSelect,
            "move_to_end",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Ctrl+Left", "Alt+Left"],
            Scope::PromptDirSelect,
            "move_left",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Ctrl+Right", "Alt+Right"],
            Scope::PromptDirSelect,
            "move_right",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Home", "Super+Left", "Ctrl+A", "Super+A"],
            Scope::Any,
            "move_left_start_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Left"], // Emacs-style whitespace word-left
            Scope::Any,
            "move_left_one_word_whitespace",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Alt+Left"], // Fine-grained word-left (stops at punctuation / path boundaries)
            Scope::Any,
            "move_left_one_word_fine_grained",
        )
        .unwrap(),
        // PromptCwdEdit Left must appear before the Normal Left binding.
        Binding::try_new(&["Left"], Scope::PromptDirSelect, "move_left").unwrap(),
        Binding::try_new(&["Left"], Scope::Any, "move_left").unwrap(),
        Binding::try_new(
            &expand_variations!["Right", "End"],
            Scope::InlineHistoryAcceptable,
            "accept_suggestion",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["End", "Super+Right", "Ctrl+E", "Super+E"],
            Scope::Any,
            "move_right_end_of_line",
        )
        .unwrap(),
        Binding::try_new(
            &["Ctrl+Right"], // Emacs-style whitespace word-right
            Scope::Any,
            "move_right_one_word_whitespace",
        )
        .unwrap(),
        Binding::try_new(
            &expand_variations!["Alt+Right"], // Fine-grained word-right (stops at punctuation / path boundaries)
            Scope::Any,
            "move_right_one_word_fine_grained",
        )
        .unwrap(),
        // PromptCwdEdit Right must appear before the Normal Right binding.
        Binding::try_new(&["Right"], Scope::PromptDirSelect, "move_right").unwrap(),
        Binding::try_new(&["Right"], Scope::Any, "move_right").unwrap(),
        Binding::try_new(&["Up"], Scope::Any, "move_line_up_or_history_up").unwrap(),
        Binding::try_new(&["Down"], Scope::Any, "move_line_down_or_history_down").unwrap(),
        Binding::try_new(&["Ctrl+z", "Super+Shift+Z"], Scope::Any, "undo").unwrap(),
        Binding::try_new(&["Ctrl+y", "Super+Shift+Z"], Scope::Any, "redo").unwrap(),
        Binding::try_new(&["AnyChar", "Shift+AnyChar"], Scope::Any, "insert_char").unwrap(),
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

/// Return the display name for a single modifier bit.
fn display_modifier_bit(bit: KeyModifiers) -> &'static str {
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
            KeyEventMatch::AnyCharAndMods(mods) => mods
                .iter()
                .map(|m| {
                    let mut parts = display_modifiers(*m);
                    parts.push("AnyChar".to_string());
                    parts.join("+")
                })
                .collect::<Vec<_>>()
                .join(" / "),
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
                let mut parts: Vec<String> = Vec::new();
                push_modifiers(ke.modifiers, &mut parts);
                match inverse_keycode_display(ke.code, remappings) {
                    Ok(name) => parts.push(name),
                    Err(name) => parts.push(format!("[INACCESSIBLE: {}]", name)),
                }
                parts.join("+")
            }
            // AnyChar bindings: apply inverse modifier display per modifier set.
            KeyEventMatch::AnyCharAndMods(mods) => mods
                .iter()
                .map(|m| {
                    let mut parts: Vec<String> = Vec::new();
                    push_modifiers(*m, &mut parts);
                    parts.push("AnyChar".to_string());
                    parts.join("+")
                })
                .collect::<Vec<_>>()
                .join(" / "),
        }
    }
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
        scoped_action: String,
        description: String,
    }

    let binding_to_row = |binding: &Binding, is_user: bool| -> Row {
        let mut keys = binding
            .key_events
            .iter()
            .map(|k| k.display_with_remapping(remappings))
            .collect::<Vec<_>>()
            .join(", ");
        if is_user {
            keys = format!("User keybinding: {}", keys);
        }
        Row {
            keys: keys.clone(),
            scoped_action: binding.action.scoped_action_name(),
            description: binding.action.description.to_string(),
        }
    };

    // Collect rows lowest-to-highest priority:
    //   1. DEFAULT_BINDINGS in reverse (last entry = lowest default priority)
    //   2. user_bindings in reverse (last entry = lowest user priority; all user
    //      bindings have higher priority than all defaults)
    let mut rows: Vec<Row> = Vec::new();
    for binding in DEFAULT_BINDINGS.iter().rev() {
        if filter_event.is_none_or(|ev| binding.matches(ev)) {
            rows.push(binding_to_row(binding, false));
        }
    }
    for binding in user_bindings.iter() {
        if filter_event.is_none_or(|ev| binding.matches(ev)) {
            rows.push(binding_to_row(binding, true));
        }
    }

    // Retrieve the terminal width; fall back to 120 columns if unavailable.
    let term_width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(120);

    let constraints = [
        Constraint::Fill(1), // Key(s)
        Constraint::Fill(2), // Action
        Constraint::Fill(2), // Description
    ];

    // Build the TableAccum for the bindings.
    let mut accum = TableAccum::default();
    accum.header_cells = vec![
        "Key(s)".to_string(),
        "Action".to_string(),
        "Description".to_string(),
    ];
    for row in &rows {
        accum.body_rows.push(vec![
            row.keys.clone(),
            row.scoped_action.clone(),
            row.description.clone(),
        ]);
    }

    // Render and print the table, converting each ratatui Line to plain text.
    let options = TableOptions { row_dividers: true };
    for line in render_table_constrained(&accum, &constraints, term_width, &options) {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        println!("{}", text);
    }

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
            }
        }
    }
}

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

        let key = apply_remappings(key, &self.settings.key_remappings);
        log::trace!("Key event after remapping: {:?}", key);

        for binding in self
            .settings
            .keybindings
            .iter()
            .rev()
            .chain(DEFAULT_BINDINGS.iter())
        {
            if binding.action.scope.is_active(self) && binding.matches(key) {
                log::trace!("Matched binding: {}", binding.action.name);
                (binding.action.action)(self, key);
                break;
            }
        }

        self.on_possible_buffer_change();
    }
}

#[cfg(test)]
mod tests {
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
}
