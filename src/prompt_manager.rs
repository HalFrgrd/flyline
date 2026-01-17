use ansi_to_tui::IntoText;
use ratatui::prelude::Line;
use ratatui::text::Span;

pub struct PromptManager {
    ps1: Vec<Line<'static>>,
}

impl PromptManager {
    pub fn new(ps1: String) -> Self {
        // Strip literal "\[" and "\]" markers from PS1 (they wrap non-printing sequences)
        let ps1 = ps1.replace("\\[", "").replace("\\]", "");
        const PS1_DEFAULT: &str = "bad ps1> ";

        // Convert from ansi-to-tui types (old ratatui 0.29) to new ratatui types
        let ps1: Vec<Line<'static>> = match ps1.into_text() {
            Ok(text) => {
                text.lines
                    .into_iter()
                    .map(|old_line| {
                        // Convert each span from old to new ratatui types
                        let new_spans: Vec<Span<'static>> = old_line
                            .spans
                            .into_iter()
                            .map(|old_span| {
                                // Just convert content to owned string, ignore styling for now
                                // TODO: convert color/modifier types properly
                                Span::raw(old_span.content.to_string())
                            })
                            .collect();
                        Line::from(new_spans)
                    })
                    .collect()
            }
            Err(_) => {
                log::warn!("Failed to parse PS1, defaulting to '{}'", PS1_DEFAULT);
                vec![Line::from(PS1_DEFAULT)]
            }
        };

        PromptManager { ps1 }
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

        self.ps1
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
