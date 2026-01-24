use ansi_to_tui::IntoText;
use ratatui::text::{Line, Span, Text};

pub struct PromptManager {
    // TODO think of lifetimes
    prompt: Vec<Line<'static>>,
}

impl PromptManager {
    pub fn new(ps1: String, unfinished_from_prev_command: bool) -> Self {
        if unfinished_from_prev_command {
            // If the previous command was unfinished, use a simple prompt to avoid confusion

            let style = ratatui::style::Style::default()
                .bg(ratatui::style::Color::Red)
                .fg(ratatui::style::Color::Black);

            return PromptManager {
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
            };
        }

        // Strip literal "\[" and "\]" markers from PS1 (they wrap non-printing sequences)
        let ps1 = ps1.replace("\\[", "").replace("\\]", "");
        const PS1_DEFAULT: &str = "bad ps1> ";

        let ps1: Vec<Line<'static>> = match ps1.into_text().unwrap_or(Text::from(PS1_DEFAULT)).lines
        {
            lines if lines.is_empty() => {
                log::warn!("Failed to parse PS1, defaulting to '>'");
                vec![Line::from(PS1_DEFAULT)]
            }
            lines => lines,
        };

        PromptManager { prompt: ps1 }
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
