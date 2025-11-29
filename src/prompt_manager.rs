use ansi_to_tui::IntoText;
use ratatui::text::{Line, Span, Text};

pub struct PromptManager {
    // TODO think of lifetimes
    ps1: Vec<Line<'static>>,
}

impl PromptManager {
    pub fn new(ps1: String) -> Self {
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

        PromptManager { ps1 }
    }

    pub fn get_ps1_lines(&self) -> Vec<Line<'static>> {
        const JOBU_TIME_STR: &str = "JOBU_TIME_XXX";
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

        self.ps1
            .clone()
            .into_iter()
            .map(|line| {
                let spans: Vec<Span> = line
                    .spans
                    .into_iter()
                    .map(|span| {
                        if span.content.contains(JOBU_TIME_STR) {
                            Span::styled(span.content.replace(JOBU_TIME_STR, &time_str), span.style)
                        } else {
                            span
                        }
                    })
                    .collect();
                Line::from(spans)
            })
            .collect()
    }
}
