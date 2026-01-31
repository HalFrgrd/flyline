use crate::bash_symbols;
use ansi_to_tui::IntoText;
use ratatui::text::{Line, Span, Text};

pub struct PromptManager {
    // TODO think of lifetimes
    prompt: Vec<Line<'static>>,
}

fn get_current_readline_prompt() -> Option<String> {
    unsafe {
        let bash_prompt_cstr = bash_symbols::current_readline_prompt;
        if !bash_prompt_cstr.is_null() {
            let c_str = std::ffi::CStr::from_ptr(bash_prompt_cstr);
            if let Ok(prompt_str) = c_str.to_str() {
                log::debug!("Fetched current_readline_prompt: {}", prompt_str);
                Some(prompt_str.to_string())
            } else {
                log::debug!("current_readline_prompt is not valid UTF-8");
                None
            }
        } else {
            log::debug!("current_readline_prompt is null");
            None
        }
    }
}

impl PromptManager {
    pub fn new(unfinished_from_prev_command: bool) -> Self {
        // let ps1 = bash_builtins::variables::find_as_string("PS1")
        //     .as_ref()
        //     .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        //     .unwrap_or("default> ".into());

        let ps1 = get_current_readline_prompt().unwrap_or_else(|| "default> ".into());

        if unfinished_from_prev_command {
            // If the previous command was unfinished, use a simple prompt to avoid confusion

            let style = ratatui::style::Style::default()
                .bg(ratatui::style::Color::Red)
                .fg(ratatui::style::Color::Black);

            PromptManager {
                prompt: vec![
                    Line::from(vec![
                        Span::styled(
                            "Bash is waiting for more input to finish the previous command .",
                            style.clone(),
                        ),
                        Span::styled(
                            "Flyline thought the previous command was complete. ",
                            style.clone(),
                        ),
                        Span::styled(
                            "Please open an issue on GitHub with the previous command that caused this message ",
                            style.clone(),
                        ),
                    ]),
                    Line::from("> "),
                ],
            }
        } else {
            // Strip literal "\[" and "\]" markers from PS1 (they wrap non-printing sequences)
            let ps1 = ps1.replace("\\[", "").replace("\\]", "");
            const PS1_DEFAULT: &str = "bad ps1> ";

            let ps1: Vec<Line<'static>> =
                match ps1.into_text().unwrap_or(Text::from(PS1_DEFAULT)).lines {
                    lines if lines.is_empty() => {
                        log::warn!("Failed to parse PS1, defaulting to '>'");
                        vec![Line::from(PS1_DEFAULT)]
                    }
                    lines => lines,
                };

            PromptManager { prompt: ps1 }
        }
    }

    pub fn get_ps1_lines(&self) -> Vec<Line<'static>> {
        const FLYLINE_TIME_STR: &str = "FLYLINE_TIME";
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        let time_str = format!(
            "{:02}:{:02}:{:02}.{:03}",
            (now.as_secs() / 3600) % 24, // hours
            (now.as_secs() / 60) % 60,   // minutes
            now.as_secs() % 60,          // seconds
            now.subsec_millis()          // milliseconds
        );

        self.prompt
            .clone()
            .into_iter()
            .map(|line| {
                let spans: Vec<Span> = line
                    .spans
                    .into_iter()
                    .map(|span| {
                        Span::styled(
                            span.content.replace(FLYLINE_TIME_STR, &time_str),
                            span.style,
                        )
                    })
                    .collect();
                Line::from(spans)
            })
            .collect()
    }
}
