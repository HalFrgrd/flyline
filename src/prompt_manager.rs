use crate::bash_funcs;
use crate::bash_symbols;
use ansi_to_tui::IntoText;
use ratatui::text::{Line, Span};
use std::collections::HashMap;

pub struct PromptManager {
    prompt: Vec<Line<'static>>,
    rprompt: Vec<Line<'static>>,
    fill_span: Line<'static>,
    /// Maps 8-character placeholder identifiers (e.g. `FLYT0000`) to the
    /// chrono format string they represent.  Populated from bash time escape
    /// sequences found in PS1 / RPS1 / PS1_FILL at construction time and
    /// applied on every render in `get_ps1_lines`.
    time_map: HashMap<String, String>,
    /// Time captured at construction; used when animations are disabled so
    /// that time-based prompt fields show the session-start time rather than
    /// updating on every render.
    construction_time: chrono::DateTime<chrono::Local>,
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

/// Builds expanded ratatui prompt lines from raw bash prompt strings while
/// accumulating a shared map of time-placeholder identifiers to chrono format
/// strings.
///
/// A single `PromptStringBuilder` should be used for all prompt variables
/// (PS1, RPS1 / RPROMPT, PS1_FILL) so that placeholder identifiers are unique
/// across all of them and can be safely merged into one `HashMap` stored in
/// [`PromptManager`].
struct PromptStringBuilder {
    /// Monotonically increasing counter used to generate unique placeholder IDs.
    counter: u32,
    /// Accumulated map of placeholder → chrono format string.
    time_map: HashMap<String, String>,
}

impl PromptStringBuilder {
    fn new() -> Self {
        Self {
            counter: 0,
            time_map: HashMap::new(),
        }
    }

    /// Scan a raw bash prompt string and replace every time format escape
    /// sequence with a unique 8-character placeholder, recording the mapping
    /// in `self.time_map`.  Returns the modified string.
    ///
    /// Recognised bash time escape sequences (see
    /// <https://www.gnu.org/software/bash/manual/html_node/Controlling-the-Prompt.html>):
    ///
    /// | Sequence     | Meaning                        | Chrono format |
    /// |--------------|--------------------------------|---------------|
    /// | `\t`         | 24-hour HH:MM:SS               | `%H:%M:%S`    |
    /// | `\T`         | 12-hour HH:MM:SS               | `%I:%M:%S`    |
    /// | `\@`         | 12-hour am/pm                  | `%I:%M %p`    |
    /// | `\A`         | 24-hour HH:MM                  | `%H:%M`       |
    /// | `\D{format}` | chrono format string (custom)  | `format`      |
    fn extract_time_codes(&mut self, s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c != '\\' {
                result.push(c);
                continue;
            }

            match chars.peek().copied() {
                Some('\\') => {
                    // Escaped backslash — pass both through so `decode_prompt_string`
                    // still sees `\\` as a literal `\`.
                    result.push('\\');
                    result.push('\\');
                    chars.next();
                }
                Some('t') => {
                    chars.next();
                    let id = self.next_id();
                    self.time_map.insert(id.clone(), "%H:%M:%S".to_string());
                    result.push_str(&id);
                }
                Some('T') => {
                    chars.next();
                    let id = self.next_id();
                    self.time_map.insert(id.clone(), "%I:%M:%S".to_string());
                    result.push_str(&id);
                }
                Some('@') => {
                    chars.next();
                    let id = self.next_id();
                    self.time_map.insert(id.clone(), "%I:%M %p".to_string());
                    result.push_str(&id);
                }
                Some('A') => {
                    chars.next();
                    let id = self.next_id();
                    self.time_map.insert(id.clone(), "%H:%M".to_string());
                    result.push_str(&id);
                }
                Some('D') => {
                    chars.next(); // consume 'D'
                    if chars.peek().copied() == Some('{') {
                        chars.next(); // consume '{'
                        let mut fmt = String::new();
                        for nc in chars.by_ref() {
                            if nc == '}' {
                                break;
                            }
                            fmt.push(nc);
                        }
                        // An empty \D{} falls back to 24-hour HH:MM:SS (%T).
                        // Bash would use strftime with the locale's time format here,
                        // but chrono does not expose a locale-aware equivalent, so %T
                        // is used as a reasonable default.
                        let chrono_fmt = if fmt.is_empty() {
                            "%T".to_string()
                        } else {
                            fmt
                        };
                        let id = self.next_id();
                        self.time_map.insert(id.clone(), chrono_fmt);
                        result.push_str(&id);
                    } else {
                        // Not \D{...} — pass through unchanged.
                        result.push('\\');
                        result.push('D');
                    }
                }
                _ => {
                    // Not a time code — pass the backslash through so
                    // `decode_prompt_string` can handle the sequence.
                    result.push('\\');
                }
            }
        }

        result
    }

    /// Expand a raw prompt string (e.g. from `PS1`, `RPS1`, `PS1_FILL`) through
    /// bash's `decode_prompt_string`, intercepting bash time escape sequences
    /// first so that the time can be substituted dynamically on every render.
    ///
    /// Returns `None` when the string cannot be processed (e.g. contains
    /// interior NUL bytes or bash returns a null pointer).
    fn expand_prompt_string(&mut self, raw: String) -> Option<Vec<Line<'static>>> {
        let modified = self.extract_time_codes(&raw);

        // Strip literal `\[` / `\]` non-printing-sequence markers before handing
        // the string to `decode_prompt_string`.
        let modified = modified.replace("\\[", "").replace("\\]", "");

        let c_prompt = std::ffi::CString::new(modified).ok()?;

        let decoded = unsafe {
            let decoded_prompt_cstr = bash_symbols::decode_prompt_string(c_prompt.as_ptr(), 1);
            if decoded_prompt_cstr.is_null() {
                return None;
            }

            let decoded = std::ffi::CStr::from_ptr(decoded_prompt_cstr)
                .to_str()
                .ok()?
                .to_string();

            // `decode_prompt_string` returns an allocated buffer.
            libc::free(decoded_prompt_cstr as *mut libc::c_void);

            decoded
        };

        let mut lines = decoded.into_text().ok()?.lines;
        for line in &mut lines {
            for span in &mut line.spans {
                let raw = span.content.as_ref();
                let stripped = raw.trim_end_matches(&['\n', '\r'][..]);
                if stripped.len() != raw.len() {
                    // `Span<'static>` can't hold a borrowed slice with a shorter lifetime, so
                    // store an owned String.
                    log::debug!("Stripping trailing newline/carriage return from prompt line span");
                    span.content = stripped.to_owned().into();
                }
            }
        }
        Some(lines)
    }

    /// Allocate the next placeholder identifier and advance the counter.
    fn next_id(&mut self) -> String {
        let id = format!("FLYT{:04X}", self.counter);
        self.counter += 1;
        id
    }

    /// Consume the builder and return the accumulated time map.
    fn into_time_map(self) -> HashMap<String, String> {
        self.time_map
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
                            "Bash needs more input to finish the command. ",
                            style.clone(),
                        ),
                        Span::styled(
                            "Flyline thought the previous command was complete. ",
                            style.clone(),
                        ),
                        Span::styled(
                            "Please open an issue on GitHub with the command that caused this message. ",
                            style.clone(),
                        ),
                    ]),
                    Line::from("> "),
                ],
                rprompt: vec![],
                fill_span: Line::from(" "),
                time_map: HashMap::new(),
                construction_time: chrono::Local::now(),
            }
        } else {
            const PS1_DEFAULT: &str = "bad ps1> ";

            // A single builder is shared across all prompt variables so that
            // placeholder IDs are unique and the resulting time_map can be
            // merged without collisions.
            let mut builder = PromptStringBuilder::new();

            // Read the raw PS1 env var so we can intercept time format codes
            // before handing the string to decode_prompt_string.  Fall back to
            // the already-expanded readline prompt when PS1 is not available.
            let ps1_raw = bash_funcs::get_env_variable("PS1").or_else(get_current_readline_prompt);

            let ps1 = ps1_raw
                .and_then(|raw| builder.expand_prompt_string(raw))
                .map(|lines| {
                    if lines.is_empty() {
                        log::warn!("Failed to parse PS1, defaulting to '{}'", PS1_DEFAULT);
                        vec![Line::from(PS1_DEFAULT)]
                    } else {
                        lines
                    }
                })
                .unwrap_or_else(|| {
                    log::warn!("Failed to parse PS1, defaulting to '{}'", PS1_DEFAULT);
                    vec![Line::from(PS1_DEFAULT)]
                });

            // Examples:
            // export RPS1='\[\e[01;32m\]\t\[\e[0m\]'
            // export RPROMPT='\[\e[01;32m\]\D{%H:%M:%S}\[\e[0m\]'
            let rps1 = bash_funcs::get_env_variable("RPS1")
                .or_else(|| bash_funcs::get_env_variable("RPROMPT"))
                .and_then(|raw| builder.expand_prompt_string(raw))
                .unwrap_or_default();

            log::debug!("Parsed RPS1: {:?}", rps1);

            let fill_lines = bash_funcs::get_env_variable("PS1_FILL")
                .and_then(|raw| builder.expand_prompt_string(raw))
                .unwrap_or_else(|| vec![Line::from(" ")]);

            let fill_span = fill_lines
                .into_iter()
                .next()
                .unwrap_or_else(|| Line::from(" "));

            let time_map = builder.into_time_map();
            log::debug!("Time map entries: {}", time_map.len());

            PromptManager {
                prompt: ps1,
                rprompt: rps1,
                fill_span,
                time_map,
                construction_time: chrono::Local::now(),
            }
        }
    }

    fn format_prompt_line(
        &self,
        line: Line<'static>,
        now: &chrono::DateTime<chrono::Local>,
    ) -> Line<'static> {
        if self.time_map.is_empty() {
            return line;
        }
        let spans: Vec<Span> = line
            .spans
            .into_iter()
            .map(|span| {
                // Only allocate a new String when at least one placeholder is present.
                let raw = span.content.as_ref();
                let needs_replacement = self.time_map.keys().any(|id| raw.contains(id.as_str()));
                if !needs_replacement {
                    return span;
                }
                let mut content = raw.to_owned();
                for (id, fmt) in &self.time_map {
                    if content.contains(id.as_str()) {
                        let time_str = now.format(fmt).to_string();
                        content = content.replace(id.as_str(), &time_str);
                    }
                }
                Span::styled(content, span.style)
            })
            .collect();
        Line::from(spans)
    }

    pub fn get_ps1_lines(
        &mut self,
        disable_animations: bool,
    ) -> (Vec<Line<'static>>, Vec<Line<'static>>, Line<'static>) {
        use chrono::Local;
        let now = if disable_animations {
            self.construction_time
        } else {
            Local::now()
        };

        let formatted_prompt: Vec<Line<'static>> = self
            .prompt
            .clone()
            .into_iter()
            .map(|line| self.format_prompt_line(line, &now))
            .collect();

        let formatted_rprompt: Vec<Line<'static>> = self
            .rprompt
            .clone()
            .into_iter()
            .map(|line| self.format_prompt_line(line, &now))
            .collect();

        let formatted_fill = self.format_prompt_line(self.fill_span.clone(), &now);

        (formatted_prompt, formatted_rprompt, formatted_fill)
    }
}
