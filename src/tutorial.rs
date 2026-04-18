use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use std::sync::LazyLock;
use unicode_width::UnicodeWidthChar;

use crate::bash_funcs;
use crate::palette::Palette;
use crate::shell_integration;

/// A sample of symbols from the Unicode legacy computing supplement range (U+1FB00–U+1FB3B).
const LEGACY_COMPUTING_SYMBOLS_SAMPLE: &str = "🬀 🬁 🬂 🬃 🬄 🬅 🬆 🬇 🬈 🬉 🬊 🬋 🬌 🬍 🬎 🬏 🬐 🬑 🬒 🬓 🬔 🬕 🬖 🬗 🬘 🬙 🬚 🬛 🬜 🬝 🬞 🬟 🬠 🬡 🬢 🬣 🬤 🬥 🬦 🬧 🬨 🬩 🬪 🬫 🬬 🬭 🬮 🬯 🬰 🬱 🬲 🬳 🬴 🬵 🬶 🬷 🬸 🬹 🬺 🬻";

/// Large block-art logo displayed on the welcome screen.
const LOGO_LINES: &[&str] = &[
    "",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}             \u{2588}\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}\u{2588}                     ",
    "\u{2591}\u{2591}\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2591}\u{2591}\u{2588}\u{2588}\u{2588}            \u{2591}\u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2591}\u{2591}                      ",
    " \u{2591}\u{2588}\u{2588}\u{2588}   \u{2588} \u{2591}  \u{2591}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    " \u{2591}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}    \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2591}\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2588}\u{2588}\u{2588}",
    " \u{2591}\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2591}\u{2588}    \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    " \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}     \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588}  \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2591}  ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}       \u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2591}\u{2591}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}       \u{2591}\u{2591}\u{2591}\u{2591}\u{2591}   \u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2588}\u{2588}\u{2588} \u{2591}\u{2591}\u{2591}\u{2591}\u{2591}  \u{2591}\u{2591}\u{2591}\u{2591}\u{2591} \u{2591}\u{2591}\u{2591}\u{2591} \u{2591}\u{2591}\u{2591}\u{2591}\u{2591}  \u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}  ",
    "                    \u{2588}\u{2588}\u{2588} \u{2591}\u{2588}\u{2588}\u{2588}                                 ",
    "                   \u{2591}\u{2591}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}                                  ",
    "                    \u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}",
];

/// Truncates a `&str` to at most `max_width` display columns.
/// Returns an owned `String` (which is always `'static`-compatible).
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut cols = 0usize;
    let mut byte_end = s.len();
    for (byte_idx, ch) in s.char_indices() {
        let ch_w = ch.width().unwrap_or(0);
        if cols + ch_w > max_width {
            byte_end = byte_idx;
            break;
        }
        cols += ch_w;
    }
    s[..byte_end].to_string()
}

/// Returns the logo lines for the welcome screen, each truncated to `max_width` display columns.
pub fn generate_welcome_logo_lines(max_width: u16) -> Vec<Line<'static>> {
    LOGO_LINES
        .iter()
        .map(|&line| Line::from(truncate_to_width(line, max_width as usize)))
        .collect()
}

/// Returns a [`Line`] whose characters each carry their own span.  The foreground
/// brightness of every span follows a Gaussian wave that travels left-to-right
/// at 15 columns per second and loops after 50 virtual positions.  Because the
/// text is only 33 characters wide the wave peak is sometimes outside the
/// visible text, giving periods where the whole line appears dim.
pub fn generate_welcome_action_line() -> Line<'static> {
    const TEXT: &str = "Press enter to start the tutorial";

    // Wave peak: travels 30 cols every 2 s → 15 cols/s; loops every 50 virtual cols.
    // The text is only 33 chars wide, so the peak spends some of its loop period
    // outside the visible text — giving intervals where the whole line is dim.
    // Non-circular distance is intentional: no wrap-around continuity at the boundary.
    let elapsed_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f32())
        .unwrap_or(0.0);
    // Virtual loop length is 50 columns; peak wraps to 0 after travelling 50 cols.
    let peak_pos = (elapsed_secs * 15.0) % 50.0;

    let spans: Vec<Span<'static>> = TEXT
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            // Gaussian falloff: sigma ≈ 4  →  2σ² = 32
            let dist = (i as f32 - peak_pos).abs();
            let intensity = (-dist * dist / 32.0_f32).exp();
            // Brightness range: 80 (dim) … 255 (peak)
            let brightness = (80.0 + 175.0 * intensity) as u8;
            let style = Style::default().fg(Color::Rgb(brightness, brightness, brightness));
            Span::styled(ch.to_string(), style)
        })
        .collect();

    Line::from(spans)
}

/// Tracks progress through the interactive tutorial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TutorialStep {
    /// Tutorial is not active.
    #[default]
    NotRunning,
    Welcome,
    RecommendedSettings,
    MouseMode,
    ThemeColours,
    FuzzyHistorySearch,
    Autocompletions,
    AutoClosing,
    FineGrainDeletion,
    FontDetection,
    End,
}

impl TutorialStep {
    const STEPS_IN_ORDER: [TutorialStep; 11] = [
        TutorialStep::Welcome,
        TutorialStep::RecommendedSettings,
        TutorialStep::MouseMode,
        TutorialStep::ThemeColours,
        TutorialStep::FuzzyHistorySearch,
        TutorialStep::Autocompletions,
        TutorialStep::AutoClosing,
        TutorialStep::FineGrainDeletion,
        TutorialStep::FontDetection,
        TutorialStep::End,
        TutorialStep::NotRunning,
    ];

    pub fn next(&mut self) {
        if self == &TutorialStep::NotRunning {
            return;
        }

        let self_idx = Self::STEPS_IN_ORDER
            .iter()
            .position(|s| s == self)
            .unwrap_or(0);
        let next_idx = (self_idx + 1) % Self::STEPS_IN_ORDER.len();
        *self = Self::STEPS_IN_ORDER[next_idx];
    }

    pub fn prev(&mut self) {
        let self_idx = Self::STEPS_IN_ORDER
            .iter()
            .position(|s| s == self)
            .unwrap_or(0);

        *self = Self::STEPS_IN_ORDER[self_idx.saturating_sub(1)];
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
        || lower_program.contains("vscode")
}

fn is_vscode() -> bool {
    shell_integration::is_vscode()
}

/// Path to the user's Zsh history file (`$HOME/.zsh_history`), if `$HOME` is
/// set. Returns `None` when no home directory can be determined.
fn zsh_history_path() -> Option<std::path::PathBuf> {
    bash_funcs::get_envvar_value("HOME").map(|h| std::path::PathBuf::from(h).join(".zsh_history"))
}

/// Returns true when the user's default shell (`$SHELL`) ends with `zsh`.
fn default_shell_is_zsh() -> bool {
    bash_funcs::get_envvar_value("SHELL")
        .map(|s| {
            std::path::PathBuf::from(&s)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == "zsh")
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Returns true when `$HOME/.zsh_history` exists and was modified within the
/// last 24 hours.
fn zsh_history_recently_modified() -> bool {
    let Some(path) = zsh_history_path() else {
        return false;
    };
    let Ok(meta) = std::fs::metadata(&path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(elapsed) = std::time::SystemTime::now().duration_since(modified) else {
        return false;
    };
    elapsed < std::time::Duration::from_secs(24 * 60 * 60)
}

/// Cached result of [`should_recommend_zsh_history`]. Evaluated lazily on
/// first access; the underlying environment / filesystem state is not
/// expected to change over the lifetime of the process.
static SHOULD_RECOMMEND_ZSH_HISTORY: LazyLock<bool> =
    LazyLock::new(|| default_shell_is_zsh() || zsh_history_recently_modified());

/// Returns true when flyline should recommend that the user enables Zsh
/// history loading: the user's default shell is `zsh`, or there is a
/// `$HOME/.zsh_history` file that was modified in the last 24 hours.
fn should_recommend_zsh_history() -> bool {
    *SHOULD_RECOMMEND_ZSH_HISTORY
}

/// Generate recommended settings text for the first tutorial step.
pub fn generate_recommended_settings(palette: &Palette) -> Text<'static> {
    let text_style = palette.normal_text();
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    if is_vscode() {
        lines.push(Line::from(Span::styled(
            "You are running in VS Code. For the best experience, set these in settings.json (try ctrl+clicking the links):",
            text_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • vscode://settings/terminal.integrated.minimumContrastRatio = 1",
            text_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • vscode://settings/terminal.integrated.enableKittyKeyboardProtocol = true",
            text_style,
        )));
        lines.push(Line::from(Span::styled(
            "  • vscode://settings/terminal.integrated.macOptionIsMeta (if on macOS)",
            text_style,
        )));
        lines.push(Line::from(""));
    }

    if detect_kitty_keyboard_support() {
        lines.push(Line::from(Span::styled(
            "✅ Your terminal supports the Kitty extended keyboard protocol.",
            text_style,
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "⚠ Your terminal may not support the Kitty extended keyboard protocol.",
            text_style,
        )));
        lines.push(Line::from(Span::styled(
            "  Consider using a terminal emulator that does (kitty, ghostty, wezterm, foot, rio).",
            text_style,
        )));
        lines.push(Line::from(Span::styled(
            "  This enables better key disambiguation for flyline.",
            text_style,
        )));
    }

    if should_recommend_zsh_history() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "💡 We detected that you use Zsh. Consider loading your Zsh history into flyline:",
            text_style,
        )));
        lines.push(Line::from(Span::styled(
            "    flyline --load-zsh-history",
            text_style,
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

    let text_style = palette.normal_text();
    let heading_style = palette.markdown_heading2();
    let mut lines: Vec<Line> = Vec::new();

    match step {
        TutorialStep::Welcome => {
            // Rendered separately as a logo screen; not handled by this function.
            return None;
        }
        TutorialStep::RecommendedSettings => {
            lines.push(Line::from(Span::styled(
                "Recommended Settings",
                heading_style,
            )));
            lines.push(Line::from(Span::styled(
                "Flyline will detect your terminal and suggest optimal settings for the best experience:",
                text_style,
            )));
            let settings_text = generate_recommended_settings(palette);
            for line in settings_text.lines {
                lines.push(line);
            }
        }
        TutorialStep::MouseMode => {
            lines.push(Line::from(Span::styled(
                "Mouse Interaction Modes",
                heading_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Flyline has three mouse interaction modes:",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "1. Smart: mouse interactions are enabled when they work well (recommended).",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "2. Simple: mouse interactions are enabled by default and toggled when Escape is pressed.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "3. Disabled: mouse interactions are disabled.",
                text_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Switch mouse interaction modes with `flyline --mouse-mode smart/simple/disabled`.",
                text_style,
            )));
        }
        TutorialStep::FuzzyHistorySearch => {
            lines.push(Line::from(Span::styled(
                "Fuzzy History Search",
                heading_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press Ctrl+R to open fuzzy history search.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Type to filter, use arrow keys / Page Up/Down to browse results.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Press Enter to run the selected command, or Tab to accept it for editing.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Press Escape to cancel.",
                text_style,
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
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Type to filter suggestions, use arrow keys or your mouse to navigate.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Press Enter or click a suggestion to accept it.",
                text_style,
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
                text_style,
            )));
            lines.push(Line::from(Span::styled("Examples:", text_style)));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --default-theme dark",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --default-theme light",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --matching-char \"bold green\"",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "  flyline set-color --recognised-command \"green\" --unrecognised-command \"bold red\"",
                text_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Run `flyline set-color --help` for all options.",
                text_style,
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
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Try typing: echo $(\" — watch how the closing \" ) are inserted for you.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "This works for parentheses (), square brackets [], curly braces {}, and quotes \" \".",
                text_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Toggle this feature with `flyline --auto-close-chars true/false`.",
                text_style,
            )));
        }
        TutorialStep::FineGrainDeletion => {
            lines.push(Line::from(Span::styled(
                "Fine-Grain Deletion",
                heading_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Flyline provides more granular deletion commands in addition to Backspace and Delete.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Ctrl+Backspace deletes one whitespace-delimited word to the left, and Alt+Backspace deletes left using finer punctuation or path-segment boundaries.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "Similarly, Ctrl+Delete deletes one whitespace-delimited word to the right, and Alt+Delete deletes right using finer punctuation or path-segment boundaries.",
                text_style,
            )));
        }
        TutorialStep::FontDetection => {
            lines.push(Line::from(Span::styled("Font Detection", heading_style)));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Flyline uses symbols from the Unicode legacy computing supplement range. Here are some examples:",
                text_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                LEGACY_COMPUTING_SYMBOLS_SAMPLE,
                text_style,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "If the symbols above are not rendering correctly, install a font that supports this range,",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "such as Iosevka Term Sans Serif (https://github.com/be5invis/Iosevka).",
                text_style,
            )));
        }
        TutorialStep::End => {
            lines.push(Line::from(Span::styled(
                "You've reached the end of the tutorial!",
                text_style.add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Feel free to explore and experiment with flyline's features.",
                text_style,
            )));
            lines.push(Line::from(Span::styled(
                "For more information, check out the documentation and GitHub repo.",
                text_style,
            )));
        }
        TutorialStep::NotRunning => unreachable!(),
    }

    Some(lines)
}
