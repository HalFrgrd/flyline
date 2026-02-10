use crate::bash_funcs;
use crate::bash_symbols;
use ansi_to_tui::IntoText;
use ratatui::text::{Line, Span, Text};

pub struct PromptManager {
    prompt: Vec<Line<'static>>,
    rprompt: Vec<Line<'static>>,
    fill_char: char,
    last_time_str: String,
}

fn get_current_readline_prompt() -> Option<String> {
    unsafe {
        let bash_prompt_cstr = bash_symbols::current_readline_prompt;
        if !bash_prompt_cstr.is_null() {
            let c_str = std::ffi::CStr::from_ptr(bash_prompt_cstr);
            if let Ok(prompt_str) = c_str.to_str() {
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
                rprompt: vec![],
                fill_char: ' ',
                last_time_str: "".into(),
            }
        } else {
            let ps1 = get_current_readline_prompt().unwrap_or_else(|| "default> ".into());

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

            let rps1: Vec<Line<'static>> = bash_funcs::get_env_variable("RPS1")
                .and_then(|rps1| {
                    // Strip literal "\\[" and "\\]" markers (they wrap non-printing sequences)
                    let rps1 = rps1.replace("\\[", "").replace("\\]", "");
                    let c_prompt = std::ffi::CString::new(rps1).ok()?;

                    unsafe {
                        let decoded_prompt_cstr =
                            bash_symbols::decode_prompt_string(c_prompt.as_ptr(), 1);
                        if decoded_prompt_cstr.is_null() {
                            return None;
                        }

                        let decoded = std::ffi::CStr::from_ptr(decoded_prompt_cstr)
                            .to_str()
                            .ok()?
                            .to_string();

                        // `decode_prompt_string` returns an allocated buffer.
                        libc::free(decoded_prompt_cstr as *mut libc::c_void);

                        Some(decoded)
                    }
                })
                .and_then(|s| s.into_text().ok())
                .unwrap_or_else(|| Text::from(""))
                .lines;

            log::debug!("Parsed RPS1: {:?}", rps1);

            let ps1_fill = bash_funcs::get_env_variable("PS1_FILL")
                .and_then(|s| s.chars().next())
                .unwrap_or(' ');

            PromptManager {
                prompt: ps1,
                rprompt: rps1,
                fill_char: ps1_fill,
                last_time_str: "".into(),
            }
        }
    }

    fn format_prompt_line(&self, line: Line<'static>) -> Line<'static> {
        const FLYLINE_TIME_STR: &str = "FLYLINE_TIME";
        let spans: Vec<Span> = line
            .spans
            .into_iter()
            .map(|span| {
                Span::styled(
                    span.content.replace(FLYLINE_TIME_STR, &self.last_time_str),
                    span.style,
                )
            })
            .collect();
        Line::from(spans)
    }

    pub fn get_ps1_lines(&mut self) -> (Vec<Line<'static>>, Vec<Line<'static>>, char) {
        // Format the current time using the system locale
        use chrono::Local;
        let now = Local::now();
        // Use the system locale for formatting
        // This will use the default time format for the locale
        self.last_time_str = now.format("%X").to_string();

        let formatted_prompt: Vec<Line<'static>> = self
            .prompt
            .clone()
            .into_iter()
            .map(|line| self.format_prompt_line(line))
            .collect();

        let formatted_rprompt: Vec<Line<'static>> = self
            .rprompt
            .clone()
            .into_iter()
            .map(|line| self.format_prompt_line(line))
            .collect();

        (formatted_prompt, formatted_rprompt, self.fill_char)
    }
}
