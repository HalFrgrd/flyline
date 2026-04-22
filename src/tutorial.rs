use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::sync::LazyLock;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::content_builder::{ClipboardTypes, Tag, TaggedLine, TaggedSpan};
use crate::palette::Palette;
use crate::shell_integration;
use crate::{bash_funcs, settings};

/// A sample of symbols from the Unicode legacy computing supplement range (U+1FB00–U+1FB3B).
const LEGACY_COMPUTING_SYMBOLS_SAMPLE: &str = "🯁🯂🯃 🬛 🮐 🮑 🮔 🮖 🮘";

/// Large block-art logo displayed on the welcome screen.
const LOGO_LINES: &[&str] = &[
    "",
    " ███████████ ████             ████   ███",
    "░░███░░░░░░█░░███            ░░███  ░░░",
    " ░███   █ ░  ░███  █████ ████ ░███  ████  ████████    ██████",
    " ░███████    ░███ ░░███ ░███  ░███ ░░███ ░░███░░███  ███░░███",
    " ░███░░░█    ░███  ░███ ░███  ░███  ░███  ░███ ░███ ░███████",
    " ░███  ░     ░███  ░███ ░███  ░███  ░███  ░███ ░███ ░███░░░",
    " █████       █████ ░░███████  █████ █████ ████ █████░░██████",
    "░░░░░       ░░░░░   ░░░░░███ ░░░░░ ░░░░░ ░░░░ ░░░░░  ░░░░░░",
    "                    ███ ░███",
    "                   ░░██████",
    "                    ░░░░░░",
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

pub fn generate_welcome_logo_lines(term_width: u16) -> Vec<Line<'static>> {
    let log_width = LOGO_LINES
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0) as u16;
    let left_padding_width = (term_width).saturating_sub(log_width) / 2;
    let right_padding_width = (term_width)
        .saturating_sub(log_width)
        .saturating_sub(left_padding_width);
    let left_padding = " ".repeat(left_padding_width as usize);
    let right_padding = " ".repeat(right_padding_width as usize);
    LOGO_LINES
        .iter()
        .map(|&line| {
            // log::info!("line len: {}, width: {}", line.len(), line.width());
            let truncated = Span::from(truncate_to_width(line, term_width as usize));
            let padded = if truncated.width() < term_width as usize {
                vec![
                    Span::from(left_padding.to_owned()),
                    truncated,
                    Span::from(right_padding.to_owned()),
                ]
            } else {
                vec![truncated]
            };
            Line::from(padded)
        })
        .collect()
}

/// Returns a [`Line`] whose characters each carry their own span.  The foreground
/// brightness of every span follows a Gaussian wave that travels left-to-right
/// at 15 columns per second and loops after 50 virtual positions.  Because the
/// text is only 33 characters wide the wave peak is sometimes outside the
/// visible text, giving periods where the whole line appears dim.
pub fn generate_welcome_action_line(now: std::time::Instant, width: u16) -> (u16, Line<'static>) {
    const TEXT: &str = "Press Enter to start the tutorial";
    static START_TIME: LazyLock<std::time::Instant> = LazyLock::new(std::time::Instant::now);

    let elapsed_secs = now.duration_since(*START_TIME).as_secs_f32();
    let peak_pos = (elapsed_secs * 25.0) % 45.0 - 5.0;

    let spans: Vec<Span<'static>> = TEXT
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            // Gaussian falloff: sigma ≈ 4  →  2σ² = 32
            let dist = (i as f32 - peak_pos).abs();
            let intensity = (-dist * dist / 16.0_f32).exp();
            let brightness = (100.0 + 175.0 * intensity) as u8;
            let style = Style::default().fg(Color::Rgb(brightness, brightness, brightness));
            Span::styled(ch.to_string(), style)
        })
        .collect();

    let offset = (width + 32).saturating_sub(TEXT.len() as u16) / 2;

    (offset, Line::from(spans))
}

/// Tracks progress through the interactive tutorial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TutorialStep {
    /// Tutorial is not active.
    #[default]
    NotRunning,
    Welcome,
    TutorialsTutorial,
    RecommendedSettings,
    MouseMode,
    ThemeColours,
    FuzzyHistorySearch,
    Keybindings,
    Autocompletions,
    AutoClosing,
    FineGrainDeletion,
    AgentMode,
    FontDetection,
    End,
}

impl TutorialStep {
    const STEPS_IN_ORDER: [TutorialStep; 14] = [
        TutorialStep::Welcome,
        TutorialStep::TutorialsTutorial,
        TutorialStep::RecommendedSettings,
        TutorialStep::MouseMode,
        TutorialStep::ThemeColours,
        TutorialStep::FuzzyHistorySearch,
        TutorialStep::Keybindings,
        TutorialStep::Autocompletions,
        TutorialStep::AutoClosing,
        TutorialStep::FineGrainDeletion,
        TutorialStep::AgentMode,
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

static SHOULD_RECOMMEND_ZSH_HISTORY: LazyLock<bool> =
    LazyLock::new(|| default_shell_is_zsh() || zsh_history_recently_modified());

/// Generate the tutorial text for the current step.
/// Returns `None` if the tutorial is not active.
pub fn generate_tutorial_text(
    settings: &settings::Settings,
    step: TutorialStep,
    palette: &Palette,
) -> Option<Vec<TaggedLine<'static>>> {
    if !step.is_active() {
        return None;
    }

    let text_style = palette.normal_text();
    let copiable_style = text_style.add_modifier(Modifier::UNDERLINED);
    let heading_style = palette.markdown_heading2();
    let key_seq_style = palette.key_sequence_style();
    let mut lines: Vec<TaggedLine> = Vec::new();

    let empty = || -> TaggedLine<'static> { TaggedLine::from_line(Line::from(""), Tag::Tutorial) };
    let tl = |span: Span<'static>| -> TaggedLine<'static> {
        TaggedLine::from_line(Line::from(span), Tag::Tutorial)
    };
    let ts_copiable = |s: String, clip_type: ClipboardTypes| -> TaggedSpan<'static> {
        TaggedSpan::new(Span::styled(s, copiable_style), Tag::Clipboard(clip_type))
    };
    let ts_key = |s: &'static str| -> TaggedSpan<'static> {
        TaggedSpan::new(Span::styled(s, key_seq_style), Tag::Tutorial)
    };
    let ts_text = |s: &'static str| -> TaggedSpan<'static> {
        TaggedSpan::new(Span::styled(s, text_style), Tag::Tutorial)
    };

    match step {
        TutorialStep::Welcome => {
            // Rendered separately as a logo screen; not handled by this function.
            return None;
        }
        TutorialStep::TutorialsTutorial => {
            lines.push(tl(Span::styled("How to use this tutorial", heading_style)));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "• Click the prev and next buttons to navigate.",
                text_style,
            )));
            lines.push(TaggedLine::from(vec![
                ts_text("• Press "),
                ts_key("Enter"),
                ts_text(" with an empty command buffer to move to the next tutorial screen."),
            ]));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(
                    Span::styled(
                        "• Click on underlined text to copy it to your clipboard and command buffer: ",
                        text_style,
                    ),
                    Tag::Tutorial,
                ),
                ts_copiable(
                    "flyline --version".to_string(),
                    ClipboardTypes::TutorialClickExample,
                ),
            ]));
            lines.push(tl(Span::styled(
                "• Exit the tutorial at any time with `flyline --run-tutorial false`.",
                text_style,
            )));
            lines.push(tl(Span::styled(
                "• Remember to append settings to your `~/.bashrc` so they persist!",
                text_style,
            )));
        }
        TutorialStep::RecommendedSettings => {
            lines.push(tl(Span::styled("Recommended Settings", heading_style)));
            lines.push(tl(Span::styled(
                "Flyline will detect your terminal and suggest optimal settings for the best experience:",
                text_style,
            )));
            lines.push(TaggedLine::from_line(Line::from(""), Tag::Tutorial));

            if is_vscode() {
                lines.push(tl(Span::styled(
                    "You are running in VS Code. For the best experience, set these in settings.json (try ctrl+clicking the links):",
                    text_style,
                )));
                lines.push(tl(Span::styled(
                    "  • vscode://settings/terminal.integrated.minimumContrastRatio = 1",
                    text_style,
                )));
                lines.push(tl(Span::styled(
                    "  • vscode://settings/terminal.integrated.enableKittyKeyboardProtocol = true",
                    text_style,
                )));
                lines.push(tl(Span::styled(
                    "  • vscode://settings/terminal.integrated.macOptionIsMeta (if on macOS)",
                    text_style,
                )));
                lines.push(TaggedLine::from_line(Line::from(""), Tag::Tutorial));
            }

            if detect_kitty_keyboard_support() {
                lines.push(tl(Span::styled(
                    "✅ Your terminal supports the Kitty extended keyboard protocol.",
                    text_style,
                )));
            } else {
                lines.push(tl(Span::styled(
                    "⚠ Your terminal may not support the Kitty extended keyboard protocol.",
                    text_style,
                )));
                lines.push(tl(Span::styled(
                    "  Consider using a terminal emulator that does (kitty, ghostty, wezterm, foot, rio).",
                    text_style,
                )));
                lines.push(tl(Span::styled(
                    "  This enables better key disambiguation for flyline.",
                    text_style,
                )));
            }

            if *SHOULD_RECOMMEND_ZSH_HISTORY {
                lines.push(TaggedLine::from_line(Line::from(""), Tag::Tutorial));
                lines.push(tl(Span::styled(
                    "💡 We detected that you use Zsh. Consider loading your Zsh history into flyline:",
                    text_style,
                )));
                lines.push(TaggedLine::from(vec![
                    TaggedSpan::new(Span::styled("    ", text_style), Tag::Tutorial),
                    ts_copiable(
                        "flyline --load-zsh-history".to_string(),
                        ClipboardTypes::TutorialRecommendedSettings,
                    ),
                ]));
            }

            let rps1_set = bash_funcs::get_envvar_value("RPS1").is_some_and(|v| !v.is_empty())
                || bash_funcs::get_envvar_value("RPROMPT").is_some_and(|v| !v.is_empty());

            if !rps1_set {
                lines.push(TaggedLine::from_line(Line::from(""), Tag::Tutorial));
                lines.push(tl(Span::styled(
                    "💡 How about showing the time in your right prompt:",
                    text_style,
                )));
                lines.push(TaggedLine::from(vec![
                    TaggedSpan::new(Span::styled("    ", text_style), Tag::Tutorial),
                    ts_copiable("RPS1='\\t'".to_string(), ClipboardTypes::TutorialRP1),
                ]));
            }
        }
        TutorialStep::MouseMode => {
            lines.push(tl(Span::styled("Mouse Interaction Modes", heading_style)));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Flyline has three mouse interaction modes:",
                text_style,
            )));
            lines.push(tl(Span::styled(
                "1. Smart: mouse interactions are enabled when they work well (recommended).",
                text_style,
            )));
            lines.push(TaggedLine::from(vec![
                ts_text("2. Simple: mouse interactions are enabled by default and toggled when "),
                ts_key("Escape"),
                ts_text(" is pressed."),
            ]));
            lines.push(tl(Span::styled(
                "3. Disabled: mouse interactions are disabled.",
                text_style,
            )));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Switch mouse interaction modes with `flyline --mouse-mode smart/simple/disabled`.",
                text_style,
            )));

            if settings
                .custom_prompt_widgets
                .values()
                .all(|w| !matches!(w, settings::PromptWidget::MouseMode(_)))
            {
                lines.push(TaggedLine::from_line(Line::from(""), Tag::Tutorial));
                lines.push(tl(Span::styled(
                    "💡 Consider display the mouse capture mode in your right prompt:",
                    text_style,
                )));
                lines.push(TaggedLine::from(vec![
                    TaggedSpan::new(Span::styled("    ", text_style), Tag::Tutorial),
                    ts_copiable(
                        "flyline create-prompt-widget mouse-mode --name MOUSE_MODE 'ON ' 'OFF' && RPS1=\" MOUSE_MODE $RPS1\"".to_string(),
                        ClipboardTypes::TutorialMouseMode,
                    ),
                ]));
            }
        }
        TutorialStep::FuzzyHistorySearch => {
            lines.push(tl(Span::styled("Fuzzy History Search", heading_style)));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                ts_text("Press "),
                ts_key("Ctrl+R"),
                ts_text(" to open fuzzy history search."),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_text("Type to filter, use "),
                ts_key("arrow keys"),
                ts_text(" / "),
                ts_key("Page Up/Down"),
                ts_text(" to browse results."),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_text("Press "),
                ts_key("Enter"),
                ts_text(" to accept the selected command for editing."),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_text("Press "),
                ts_key("Escape"),
                ts_text(" to cancel."),
            ]));
        }
        TutorialStep::Keybindings => {
            lines.push(tl(Span::styled("Keybindings", heading_style)));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                ts_text("Run "),
                ts_copiable(
                    "flyline key list".to_string(),
                    ClipboardTypes::TutorialKeybindingsList,
                ),
                ts_text(" to see all current keybindings."),
            ]));
            lines.push(empty());
            lines.push(tl(Span::styled("Common custom keybindings:", text_style)));
            lines.push(TaggedLine::from(vec![
                ts_text("• Accept and immediately run the selected fuzzy history entry (instead of accepting for editing):"),
            ]));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled("    ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline key bind Enter fuzzy_history_search::accept_and_run".to_string(),
                    ClipboardTypes::TutorialKeybindingsBind1,
                ),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_text("• Temporarily dismiss an inline history suggestion with "),
                ts_key("Escape"),
                ts_text(":"),
            ]));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled("    ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline key bind Escape inline_history_acceptable::dismiss_suggestion"
                        .to_string(),
                    ClipboardTypes::TutorialKeybindingsBind2,
                ),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_text("• Accept an inline history suggestion with "),
                ts_key("Tab"),
                ts_text(":"),
            ]));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled("    ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline key bind Tab inline_history_acceptable::accept_suggestion".to_string(),
                    ClipboardTypes::TutorialKeybindingsBind3,
                ),
            ]));
        }
        TutorialStep::Autocompletions => {
            lines.push(tl(Span::styled("Fuzzy Autocompletions", heading_style)));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled("Type ", text_style), Tag::Tutorial),
                ts_copiable(
                    "grep --".to_string(),
                    ClipboardTypes::TutorialGrep,
                ),
                ts_text(" and press "),
                ts_key("Tab"),
                ts_text(" to trigger autocompletions. If nothing comes up, first set normal Bash completions ("),
                ts_copiable(
                    "https://github.com/scop/bash-completion".to_string(),
                    ClipboardTypes::TutorialBashCompletion,
                ),
                TaggedSpan::new(Span::styled(")", text_style), Tag::Tutorial),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_text("Type to filter suggestions, use "),
                ts_key("arrow keys"),
                ts_text(" or your mouse to navigate."),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_text("Press "),
                ts_key("Enter"),
                ts_text(" or click a suggestion to accept it."),
            ]));
        }
        TutorialStep::ThemeColours => {
            lines.push(tl(Span::styled("Setting Theme Colours", heading_style)));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Customise your colour theme with the `flyline set-color` command.",
                text_style,
            )));
            lines.push(tl(Span::styled("Examples:", text_style)));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled(" ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline set-color --default-theme dark".to_string(),
                    ClipboardTypes::TutorialSetColor1,
                ),
            ]));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled(" ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline set-color --default-theme light".to_string(),
                    ClipboardTypes::TutorialSetColor2,
                ),
            ]));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled(" ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline set-color --matching-char \"bold green\"".to_string(),
                    ClipboardTypes::TutorialSetColor3,
                ),
            ]));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled(" ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline set-color --recognised-command \"green\" --unrecognised-command \"bold red\"".to_string(),
                    ClipboardTypes::TutorialSetColor4,
                ),
            ]));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled("Run ", text_style), Tag::Tutorial),
                ts_copiable(
                    "flyline set-color --help".to_string(),
                    ClipboardTypes::TutorialSetColor5,
                ),
                TaggedSpan::new(
                    Span::styled(" to see all options.", text_style),
                    Tag::Tutorial,
                ),
            ]));
        }
        TutorialStep::AutoClosing => {
            lines.push(tl(Span::styled(
                "Auto-Closing Quotes & Brackets",
                heading_style,
            )));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Flyline automatically inserts closing characters when you type an opening one.",
                text_style,
            )));
            lines.push(tl(Span::styled(
                "Try typing `echo \"$(` and watch Flyline insert the closing `)\"` for you.",
                text_style,
            )));
            lines.push(tl(Span::styled(
                "This works for parentheses (), square brackets [], curly braces {}, and quotes \" \".",
                text_style,
            )));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(
                    Span::styled("Toggle this feature with ", text_style),
                    Tag::Tutorial,
                ),
                ts_copiable(
                    "flyline --auto-close-chars true/false".to_string(),
                    ClipboardTypes::TutorialAutoClose,
                ),
                TaggedSpan::new(Span::styled(".", text_style), Tag::Tutorial),
            ]));
        }
        TutorialStep::FineGrainDeletion => {
            lines.push(tl(Span::styled("Fine-Grained Deletion", heading_style)));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                ts_key("Ctrl+Backspace"),
                ts_text(" deletes one whitespace-delimited word to the left."),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_key("Alt+Backspace"),
                ts_text(
                    " deletes one chunk to the left using finer punctuation or path-segment boundaries.",
                ),
            ]));
            lines.push(TaggedLine::from(vec![
                ts_key("Ctrl+Delete"),
                ts_text(" and "),
                ts_key("Alt+Delete"),
                ts_text(" work similarly."),
            ]));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Try it out on this example command:",
                text_style,
            )));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled("  ", text_style), Tag::Tutorial),
                ts_copiable(
                    "ls foo/bar_abc/qwe.txt oiu.txt".to_string(),
                    ClipboardTypes::TutorialFineGrainDeletion,
                ),
            ]));
        }
        TutorialStep::AgentMode => {
            lines.push(tl(Span::styled("Agent Mode", heading_style)));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Flyline can interface with your AI agent to help you write commands.",
                text_style,
            )));
            lines.push(tl(Span::styled(
                "Try activating agent mode and get help setting it up:",
                text_style,
            )));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled("Type ", text_style), Tag::Tutorial),
                ts_copiable(
                    "list files older than three days".to_string(),
                    ClipboardTypes::TutorialAgentMode,
                ),
                ts_text(" and press "),
                ts_key("Alt+Enter"),
                ts_text("."),
            ]));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                ts_text("When setting it up, you can specify a `--trigger-prefix`. If the buffer starts with this prefix, flyline will activate agent mode when you press "),
                ts_key("Enter"),
                ts_text("."),
            ]));
            lines.push(empty());
        }
        TutorialStep::FontDetection => {
            lines.push(tl(Span::styled("Font Detection", heading_style)));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Optional: For the best terminal experience, use a font that supports the Unicode legacy computing symbols (U+1FB00-U+1FB3B).",
                text_style,
            )));
            lines.push(empty());
            lines.push(tl(Span::styled(
                LEGACY_COMPUTING_SYMBOLS_SAMPLE,
                text_style,
            )));
            lines.push(empty());
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(Span::styled(
                    "If the symbols above are not rendering correctly, install a font that supports this range, such as Iosevka Term Sans Serif (",
                    text_style,
                ), Tag::Tutorial),
                ts_copiable(
                    "https://github.com/be5invis/Iosevka".to_string(),
                    ClipboardTypes::TutorialIosevka,
                ),
                TaggedSpan::new(Span::styled(").", text_style), Tag::Tutorial),
            ]));
        }
        TutorialStep::End => {
            lines.push(tl(Span::styled(
                "You've reached the end of the tutorial!",
                text_style.add_modifier(Modifier::BOLD),
            )));
            lines.push(empty());
            lines.push(tl(Span::styled(
                "Feel free to explore and experiment with flyline's features.",
                text_style,
            )));
            lines.push(TaggedLine::from(vec![
                TaggedSpan::new(
                    Span::styled("For more information, check out ", text_style),
                    Tag::Tutorial,
                ),
                ts_copiable(
                    "flyline --help".to_string(),
                    ClipboardTypes::TutorialRunHelp,
                ),
                TaggedSpan::new(
                    Span::styled(" and https://github.com/HalFrgrd/flyline.", text_style),
                    Tag::Tutorial,
                ),
            ]));
        }
        TutorialStep::NotRunning => unreachable!(),
    }

    Some(lines)
}
