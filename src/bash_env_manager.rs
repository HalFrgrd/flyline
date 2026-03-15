use crate::bash_funcs;
use lscolors::LsColors;
use ratatui::style::{Color, Modifier, Style};
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::fuzzy_matcher::arinae::ArinaeMatcher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Manages bash environment state including caches for command types, aliases, and other bash constructs
pub struct BashEnvManager {
    call_type_cache: HashMap<String, (bash_funcs::CommandType, String)>,
    defined_aliases: Vec<String>,
    defined_reserved_words: Vec<String>,
    defined_shell_functions: Vec<String>,
    defined_builtins: Vec<String>,
    defined_executables: Vec<(PathBuf, String)>,
    ls_colors: Option<LsColors>,
}

impl BashEnvManager {
    pub fn new() -> Self {
        let executables = if let Some(path_str) = bash_funcs::get_env_variable("PATH") {
            Self::get_executables_from_path(&path_str)
        } else {
            Vec::new()
        };

        let ls_colors =
            bash_funcs::get_env_variable("LS_COLORS").map(|s| LsColors::from_string(&s));

        Self {
            call_type_cache: HashMap::new(),
            defined_aliases: bash_funcs::get_all_aliases(),
            defined_reserved_words: bash_funcs::get_all_reserved_words(),
            defined_shell_functions: bash_funcs::get_all_shell_functions(),
            defined_builtins: bash_funcs::get_all_shell_builtins(),
            defined_executables: executables,
            ls_colors,
        }
    }

    pub fn get_command_info(&mut self, cmd: &str) -> (bash_funcs::CommandType, String) {
        if let Some(res) = self.call_type_cache.get(cmd) {
            res.clone()
        } else {
            let result = bash_funcs::call_type(cmd);
            self.call_type_cache.insert(cmd.to_string(), result.clone());
            result
        }
    }

    /// Return a ratatui `Style` for the given path based on the `LS_COLORS` environment variable.
    /// Returns `None` if `LS_COLORS` was not set or the path has no matching entry.
    pub fn style_for_path(&self, path: &Path) -> Option<Style> {
        let lscolors_style = self.ls_colors.as_ref()?.style_for_path(path)?;
        Some(lscolors_style_to_ratatui(lscolors_style))
    }

    /// Get all potential first word completions (aliases, reserved words, functions, builtins, executables)
    pub fn get_first_word_completions(&self, command: &str) -> Vec<String> {
        let mut res = Vec::new();
        let mut seen = HashSet::new();

        if command.is_empty() {
            return res;
        }

        for poss_completion in self
            .defined_aliases
            .iter()
            .chain(self.defined_reserved_words.iter())
            .chain(self.defined_shell_functions.iter())
            .chain(self.defined_builtins.iter())
            .chain(self.defined_executables.iter().map(|(_, name)| name))
        {
            if poss_completion.starts_with(command) && seen.insert(poss_completion.as_str()) {
                res.push(poss_completion.to_string());
            }
        }

        res
    }

    /// Get fuzzy first word completions using SkimMatcherV2 for when no exact prefix match is found
    pub fn get_fuzzy_first_word_completions(&self, command: &str) -> Vec<String> {
        if command.is_empty() {
            return vec![];
        }

        let matcher = ArinaeMatcher::new(skim::CaseMatching::Smart, true);
        let mut scored: Vec<(i64, String)> = self
            .defined_aliases
            .iter()
            .chain(self.defined_reserved_words.iter())
            .chain(self.defined_shell_functions.iter())
            .chain(self.defined_builtins.iter())
            .chain(self.defined_executables.iter().map(|(_, name)| name))
            .filter_map(|poss_completion| {
                matcher
                    .fuzzy_match(poss_completion, command)
                    .map(|score| (score, poss_completion.to_string()))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, s)| s).collect()
    }

    /// Get executables from PATH environment variable
    fn get_executables_from_path(path_str: &str) -> Vec<(PathBuf, String)> {
        let mut executables = Vec::new();
        for path_dir in path_str.split(':') {
            if let Ok(entries) = std::fs::read_dir(path_dir) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            let permissions = metadata.permissions();
                            if permissions.mode() & 0o111 != 0 {
                                // File is executable
                                if let Some(file_name) = entry.file_name().to_str() {
                                    executables.push((entry.path(), file_name.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }
        executables
    }
}

/// Convert an `lscolors::Color` to a `ratatui::style::Color`.
fn lscolors_color_to_ratatui(color: lscolors::Color) -> Color {
    match color {
        lscolors::Color::Black => Color::Black,
        lscolors::Color::Red => Color::Red,
        lscolors::Color::Green => Color::Green,
        lscolors::Color::Yellow => Color::Yellow,
        lscolors::Color::Blue => Color::Blue,
        lscolors::Color::Magenta => Color::Magenta,
        lscolors::Color::Cyan => Color::Cyan,
        lscolors::Color::White => Color::White,
        lscolors::Color::BrightBlack => Color::DarkGray,
        lscolors::Color::BrightRed => Color::LightRed,
        lscolors::Color::BrightGreen => Color::LightGreen,
        lscolors::Color::BrightYellow => Color::LightYellow,
        lscolors::Color::BrightBlue => Color::LightBlue,
        lscolors::Color::BrightMagenta => Color::LightMagenta,
        lscolors::Color::BrightCyan => Color::LightCyan,
        lscolors::Color::BrightWhite => Color::Gray,
        lscolors::Color::Fixed(n) => Color::Indexed(n),
        lscolors::Color::RGB(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Convert an `lscolors::Style` to a `ratatui::style::Style`.
fn lscolors_style_to_ratatui(style: &lscolors::Style) -> Style {
    let mut ratatui_style = Style::default();

    if let Some(fg) = style.foreground {
        ratatui_style = ratatui_style.fg(lscolors_color_to_ratatui(fg));
    }
    if let Some(bg) = style.background {
        ratatui_style = ratatui_style.bg(lscolors_color_to_ratatui(bg));
    }

    let fs = &style.font_style;
    if fs.bold {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if fs.dimmed {
        ratatui_style = ratatui_style.add_modifier(Modifier::DIM);
    }
    if fs.italic {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if fs.underline {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }
    if fs.slow_blink {
        ratatui_style = ratatui_style.add_modifier(Modifier::SLOW_BLINK);
    }
    if fs.rapid_blink {
        ratatui_style = ratatui_style.add_modifier(Modifier::RAPID_BLINK);
    }
    if fs.reverse {
        ratatui_style = ratatui_style.add_modifier(Modifier::REVERSED);
    }
    if fs.hidden {
        ratatui_style = ratatui_style.add_modifier(Modifier::HIDDEN);
    }
    if fs.strikethrough {
        ratatui_style = ratatui_style.add_modifier(Modifier::CROSSED_OUT);
    }

    ratatui_style
}
