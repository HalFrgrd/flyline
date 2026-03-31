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
    FirstStep,
    /// Fuzzy history search (Ctrl+R).
    SecondStep,
    /// Fuzzy autocompletions (Tab).
    ThirdStep,
    /// Setting theme colours.
    FourthStep,
}

impl TutorialStep {
    /// Advance to the next step. Wraps around to `FirstStep` after the last step.
    pub fn next(&mut self) {
        *self = match self {
            TutorialStep::NotRunning => TutorialStep::NotRunning,
            TutorialStep::FirstStep => TutorialStep::SecondStep,
            TutorialStep::SecondStep => TutorialStep::ThirdStep,
            TutorialStep::ThirdStep => TutorialStep::FourthStep,
            TutorialStep::FourthStep => TutorialStep::NotRunning,
        };
    }

    /// Go back to the previous step. Stops at `FirstStep`.
    pub fn prev(&mut self) {
        *self = match self {
            TutorialStep::NotRunning => TutorialStep::NotRunning,
            TutorialStep::FirstStep => TutorialStep::FirstStep,
            TutorialStep::SecondStep => TutorialStep::FirstStep,
            TutorialStep::ThirdStep => TutorialStep::SecondStep,
            TutorialStep::FourthStep => TutorialStep::ThirdStep,
        };
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
    let hint_style = palette.tutorial_hint;
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        "Recommended settings:",
        hint_style,
    )));
    lines.push(Line::from(""));

    if is_vscode() {
        lines.push(Line::from(Span::styled(
            "You are running in VS Code. For the best experience, set these in settings.json:",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • terminal.integrated.minimumContrastRatio = 1",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • terminal.integrated.enableKittyKeyboardProtocol = true",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • workbench.settings.alwaysShowAdvancedSettings = 1",
            hint_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • terminal.integrated.macOptionIsMeta (if on macOS)",
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
pub fn generate_tutorial_text(
    step: TutorialStep,
    palette: &Palette,
    width: u16,
) -> Option<Vec<Line<'static>>> {
    if !step.is_active() {
        return None;
    }

    let hint_style = palette.tutorial_hint;
    let mut lines: Vec<Line> = Vec::new();

    // Navigation bar with prev/next boxes
    let nav_line = build_nav_line(step, palette, width);
    lines.push(nav_line);
    lines.push(Line::from(""));

    match step {
        TutorialStep::FirstStep => {
            lines.push(Line::from(Span::styled(
                "Welcome to flyline!",
                hint_style.add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            let settings_text = generate_recommended_settings(palette);
            for line in settings_text.lines {
                lines.push(line);
            }
        }
        TutorialStep::SecondStep => {
            lines.push(Line::from(Span::styled(
                "Fuzzy History Search",
                hint_style.add_modifier(Modifier::BOLD),
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
        TutorialStep::ThirdStep => {
            lines.push(Line::from(Span::styled(
                "Fuzzy Autocompletions",
                hint_style.add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press Tab to trigger autocompletions.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Type to filter suggestions, use arrow keys to navigate.",
                hint_style,
            )));
            lines.push(Line::from(Span::styled(
                "Press Enter or click a suggestion to accept it.",
                hint_style,
            )));
        }
        TutorialStep::FourthStep => {
            lines.push(Line::from(Span::styled(
                "Setting Theme Colors",
                hint_style.add_modifier(Modifier::BOLD),
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
        TutorialStep::NotRunning => unreachable!(),
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "💡 Run `flyline --run-tutorial false` to disable the tutorial.",
        hint_style,
    )));
    lines.push(Line::from(""));

    Some(lines)
}

/// Build the navigation line with [prev] and [next] boxes.
fn build_nav_line(step: TutorialStep, palette: &Palette, width: u16) -> Line<'static> {
    let hint_style = palette.tutorial_hint;
    let step_label = match step {
        TutorialStep::FirstStep => "Step 1/4",
        TutorialStep::SecondStep => "Step 2/4",
        TutorialStep::ThirdStep => "Step 3/4",
        TutorialStep::FourthStep => "Step 4/4",
        TutorialStep::NotRunning => "",
    };

    let prev_text = " ◀ prev ";
    let next_text = " next ▶ ";
    let step_text = format!(" {} ", step_label);

    // Total width of nav content: prev + step + next + spaces
    let content_width = prev_text.len() + step_text.len() + next_text.len() + 2; // 2 spaces between parts
    let padding = if (width as usize) > content_width {
        " ".repeat(width as usize - content_width)
    } else {
        String::new()
    };

    Line::from(vec![
        Span::styled(prev_text, hint_style.add_modifier(Modifier::REVERSED)),
        Span::styled(" ", hint_style),
        Span::styled(step_text, hint_style),
        Span::styled(" ", hint_style),
        Span::styled(next_text, hint_style.add_modifier(Modifier::REVERSED)),
        Span::styled(padding, hint_style),
    ])
}
