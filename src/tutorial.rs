use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};

use crate::bash_funcs;
use crate::palette::Palette;

/// Tracks progress through the interactive tutorial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TutorialStep {
    /// Tutorial is not active.
    #[default]
    NotRunning,
    /// Welcome message and recommended settings.
    Welcome,
    RecommendedSettings,
    MouseMode,
    ThemeColours,
    FuzzyHistorySearch,
    Autocompletions,
    AutoClosing,
}

impl TutorialStep {
    const STEPS_IN_ORDER: [TutorialStep; 8] = [
        TutorialStep::Welcome,
        TutorialStep::RecommendedSettings,
        TutorialStep::MouseMode,
        TutorialStep::ThemeColours,
        TutorialStep::FuzzyHistorySearch,
        TutorialStep::Autocompletions,
        TutorialStep::AutoClosing,
        TutorialStep::NotRunning,
    ];

    pub fn next(&mut self) {
        if self == &TutorialStep::NotRunning {
            return;
        }

        let self_idx = self
            .STEPS_IN_ORDER
            .iter()
            .position(|s| s == self)
            .unwrap_or(0);
        let next_idx = (self_idx + 1) % self.steps_in_order.len();
        *self = self.steps_in_order[next_idx];
    }

    pub fn prev(&mut self) {
        if self == &TutorialStep::Welcome {
            return;
        }

        let self_idx = self
            .steps_in_order
            .iter()
            .position(|s| s == self)
            .unwrap_or(0);
        let prev_idx = if self_idx == 0 {
            0
        } else {
            self_idx - 1
        };
        *self = self.steps_in_order[prev_idx];
    }

    /// Whether the tutorial is currently active (any step other than `NotRunning`).
    pub fn is_active(&self) -> bool {
        !matches!(self, TutorialStep::NotRunning)
    }
}

/// Detect whether the terminal supports the Kitty extended keyboard protocol.
///
/// This checks the `TERM` and `TERM_PROGRAM` environment variables for terminals known to
/// support the protocol.
/// TODO: https://sw.kovidgoyal.net/kitty/keyboard-protocol/#detection-of-support-for-this-protocol
fn detect_kitty_keyboard_support() -> bool {
    let term = bash_funcs::get_envvar_value("TERM").unwrap_or_default();
    let term_program = bash_funcs::get_envvar_value("TERM_PROGRAM").unwrap_or_default();
    let lower_term = term.to_lowercase();
    let lower_program = term_program.to_lowercase();

    // Terminals known to support the Kitty keyboard protocol
    lower_term.contains("xterm-kitty")
        || lower_program.contains("kitty")
        || lower_program.contains("ghostty")
        || lower_program.contains("wezterm")
        || lower_program.contains("foot")
        || lower_program.contains("rio")
}

fn is_vscode() -> bool {
    bash_funcs::get_envvar_value("TERM_PROGRAM").as_deref() == Some("vscode")
}

/// Generate recommended settings text for the first tutorial step.
pub fn generate_recommended_settings(palette: &Palette) -> Text<'static> {
    let hint_style = palette.tutorial_hint();
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    if is_vscode() {
        lines.push(Line::from(Span::styled(
            "You are running in VS Code. For the best experience, set these in settings.json (try ctrl+clicking the links):",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • vscode://settings/terminal.integrated.minimumContrastRatio = 1",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • vscode://settings/terminal.integrated.enableKittyKeyboardProtocol = true",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • vscode://settings/terminal.integrated.macOptionIsMeta (if on macOS)",
            hint_style,
        )));
        lines.push(Line::from(""));
    }

    if detect_kitty_keyboard_support() {
        lines.push(Line::from(Span::styled(
            "✅ Your terminal supports the Kitty extended keyboard protocol.",
            hint_style,
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "⚠ Your terminal may not support the Kitty extended keyboard protocol.",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  Consider using a terminal emulator that does (kitty, ghostty, wezterm, foot, rio).",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  This enables better key disambiguation for flyline.",
            hint_style,
        )));
    }

    Text::from(lines)
}

/// Generate the tutorial text for the current step.
/// Returns `None` if the tutorial is not active.
pub fn generate_tutorial_text(step: TutorialStep, palette: &Palette) -> Option<Vec<Line<'static>>> {
    if !step.is_active() {
        return None;
    }

    let hint_style = palette.tutorial_hint();
    let heading_style = palette.markdown_heading2();
    let mut lines: Vec<Line> = Vec::new();

    match step {
        TutorialStep::Welcome => {
            lines.push(Line::from(Span::styled(
                "Welcome to flyline!",
                hint_style.add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Run `flyline --run-tutorial false` to disable the tutorial.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "To start the tutorial, press Enter. Navigate by clicking on the buttons.",
                hint_style,
            )));
        }
        TutorialStep::RecommendedSettings => {
            lines.push(Line::from(Span::styled(
                "Recommended settings:",
                heading_style,
            )));
            let settings_text = generate_recommended_settings(palette);
            for line in settings_text.lines {
                lines.push(line);
            }
        }
        TutorialStep::FuzzyHistorySearch => {
            lines.push(Line::from(Span::styled(
                "Fuzzy History Search",
                heading_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press Ctrl+R to open fuzzy history search.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Type to filter, use arrow keys / Page Up/Down to browse results.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Press Enter to run the selected command, Shift+Enter to accept it for editing.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Press Escape to cancel.",
                hint_style,
            )));
        }
        TutorialStep::Autocompletions => {
            lines.push(Line::from(Span::styled(
                "Fuzzy Autocompletions",
                heading_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Type `grep --` and press Tab to trigger autocompletions. If nothing comes up, first set normal Bash completions (https://github.com/scop/bash-completion).",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Type to filter suggestions, use arrow keys or your mouse to navigate.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Press Enter or click a suggestion to accept it.",
                hint_style,
            )));
        }
        TutorialStep::ThemeColours => {
            lines.push(Line::from(Span::styled(
                "Setting Theme Colors",
                heading_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Customise your colour theme with the `flyline set-color` command.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled("Examples:", hint_style)));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --default dark",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --default light",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --matching-char \"bold green\"",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --recognised-word \"green\" --unrecognised-word \"bold red\"",
                hint_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Run `flyline set-color --help` for all options.",
                hint_style,
            )));
        }
        TutorialStep::AutoClosing => {
            lines.push(Line::from(Span::styled(
                "Auto-Closing Quotes & Brackets",
                heading_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Flyline automatically inserts closing characters when you type an opening one.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Try typing: echo $(\" — watch how the closing \" ) are inserted for you.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "This works for parentheses (), square brackets [], curly braces {}, and quotes \" \".",
                hint_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Toggle this feature with `flyline --auto-close-chars true/false`.",
                hint_style,
            )));
        }
        TutorialStep::NotRunning => unreachable!(),
    }

    Some(lines)
}
