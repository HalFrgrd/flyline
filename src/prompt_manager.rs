use crate::bash_funcs;
use crate::bash_symbols;
use crate::settings::PromptAnimation;
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
    /// Maps placeholder identifiers (e.g. `FLYT0001`) to the index of the
    /// corresponding animation in `animations`.
    anim_map: HashMap<String, usize>,
    /// Custom animations provided via `flyline create-anim`.
    animations: Vec<PromptAnimation>,
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
    /// Accumulated map of placeholder → animation index.
    anim_map: HashMap<String, usize>,
}

impl PromptStringBuilder {
    fn new() -> Self {
        Self {
            counter: 0,
            time_map: HashMap::new(),
            anim_map: HashMap::new(),
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

    /// Scan a raw bash prompt string and replace every occurrence of a registered
    /// animation name with a unique 8-character placeholder, recording the
    /// mapping in `self.anim_map`.  Returns the modified string.
    ///
    /// Animation names are matched in order of decreasing length so that a name
    /// that is a prefix of a longer name does not shadow it.
    fn extract_anim_codes(&mut self, s: &str, animations: &[PromptAnimation]) -> String {
        if animations.is_empty() {
            return s.to_string();
        }
        // Sort indices by name length (longest first) to avoid partial replacements.
        let mut sorted_indices: Vec<usize> = (0..animations.len()).collect();
        sorted_indices.sort_by(|&a, &b| animations[b].name.len().cmp(&animations[a].name.len()));

        let mut result = s.to_string();
        for idx in sorted_indices {
            let name = &animations[idx].name;
            if result.contains(name.as_str()) {
                let id = self.next_id();
                self.anim_map.insert(id.clone(), idx);
                result = result.replace(name.as_str(), &id);
            }
        }
        result
    }

    /// Expand a raw prompt string (e.g. from `PS1`, `RPS1`, `PS1_FILL`) through
    /// bash's `decode_prompt_string`, intercepting bash time escape sequences
    /// and custom animation names first so that their values can be substituted
    /// dynamically on every render.
    ///
    /// Returns `None` when the string cannot be processed (e.g. contains
    /// interior NUL bytes or bash returns a null pointer).
    fn expand_prompt_string(
        &mut self,
        raw: String,
        animations: &[PromptAnimation],
    ) -> Option<Vec<Line<'static>>> {
        // Animation names are replaced before time codes so that user-defined
        // names cannot accidentally shadow the generated `FLYT…` placeholders.
        let modified = self.extract_anim_codes(&raw, animations);
        let modified = self.extract_time_codes(&modified);

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

    /// Consume the builder and return the accumulated time map and animation map.
    fn into_maps(self) -> (HashMap<String, String>, HashMap<String, usize>) {
        (self.time_map, self.anim_map)
    }
}

impl PromptManager {
    pub fn new(unfinished_from_prev_command: bool, animations: &[PromptAnimation]) -> Self {
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
                anim_map: HashMap::new(),
                animations: vec![],
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
                .and_then(|raw| builder.expand_prompt_string(raw, animations))
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
            // export RPS1='\e[01;32m\t\e[0m'
            // export RPROMPT='\e[01;32m\D{%H:%M:%S}\e[0m'
            let rps1 = bash_funcs::get_env_variable("RPS1")
                .or_else(|| bash_funcs::get_env_variable("RPROMPT"))
                .and_then(|raw| builder.expand_prompt_string(raw, animations))
                .unwrap_or_default();

            log::debug!("Parsed RPS1: {:?}", rps1);

            let fill_lines = bash_funcs::get_env_variable("PS1_FILL")
                .and_then(|raw| builder.expand_prompt_string(raw, animations))
                .unwrap_or_else(|| vec![Line::from(" ")]);

            let fill_span = fill_lines
                .into_iter()
                .next()
                .unwrap_or_else(|| Line::from(" "));

            let (time_map, anim_map) = builder.into_maps();
            log::debug!(
                "Time map entries: {}, animation map entries: {}",
                time_map.len(),
                anim_map.len()
            );

            PromptManager {
                prompt: ps1,
                rprompt: rps1,
                fill_span,
                time_map,
                anim_map,
                animations: animations.to_vec(),
                construction_time: chrono::Local::now(),
            }
        }
    }

    fn format_prompt_line(
        &self,
        line: Line<'static>,
        now: &chrono::DateTime<chrono::Local>,
    ) -> Line<'static> {
        if self.time_map.is_empty() && self.anim_map.is_empty() {
            return line;
        }
        let spans: Vec<Span<'static>> = line
            .spans
            .into_iter()
            .flat_map(|span| self.expand_span(span, now))
            .collect();
        Line::from(spans)
    }

    /// Expand a single [`Span`] by substituting any time-code and animation
    /// placeholders it contains.
    ///
    /// Time-code substitution is a simple in-place string replacement that
    /// preserves the span's existing style.  Animation substitution may expand
    /// one span into several because a frame can contain its own ANSI colour
    /// sequences.  In that case the frame string is parsed through
    /// `ansi-to-tui` and the surrounding text retains the original span style.
    fn expand_span(
        &self,
        span: Span<'static>,
        now: &chrono::DateTime<chrono::Local>,
    ) -> Vec<Span<'static>> {
        let raw = span.content.as_ref();

        let needs_time =
            !self.time_map.is_empty() && self.time_map.keys().any(|id| raw.contains(id.as_str()));
        let needs_anim =
            !self.anim_map.is_empty() && self.anim_map.keys().any(|id| raw.contains(id.as_str()));

        if !needs_time && !needs_anim {
            return vec![span];
        }

        let style = span.style;

        // --- time-code replacement (plain string substitution) ---------------
        let mut content = raw.to_owned();
        if needs_time {
            for (id, fmt) in &self.time_map {
                if content.contains(id.as_str()) {
                    content = content.replace(id.as_str(), &now.format(fmt).to_string());
                }
            }
        }

        if !needs_anim {
            return vec![Span::styled(content, style)];
        }

        // --- animation replacement (may introduce ANSI-styled sub-spans) -----
        let mut result: Vec<Span<'static>> = vec![];
        let mut remaining = content;

        loop {
            // Find the animation placeholder that appears earliest in `remaining`.
            let next_opt = self
                .anim_map
                .iter()
                .filter_map(|(id, &anim_idx)| {
                    remaining
                        .find(id.as_str())
                        .map(|pos| (pos, id.clone(), anim_idx))
                })
                .min_by_key(|(pos, _, _)| *pos);

            let (pos, id, anim_idx) = match next_opt {
                None => break,
                Some(t) => t,
            };

            let anim = &self.animations[anim_idx];
            let frame = Self::compute_frame(anim, now);
            let id_len = id.len();

            if pos > 0 {
                result.push(Span::styled(remaining[..pos].to_owned(), style));
            }

            // Parse the frame through ansi-to-tui so that any ANSI colour
            // sequences it contains are converted to ratatui Styles rather
            // than left as raw bytes in span content.
            match frame.into_text() {
                Ok(text) => {
                    for frame_line in text.lines {
                        result.extend(frame_line.spans);
                    }
                }
                Err(e) => {
                    log::warn!("Failed to parse animation frame as ANSI text: {}", e);
                    result.push(Span::styled(String::new(), style));
                }
            }

            remaining = remaining[pos + id_len..].to_owned();
        }

        if !remaining.is_empty() {
            result.push(Span::styled(remaining, style));
        }

        if result.is_empty() {
            vec![Span::styled(String::new(), style)]
        } else {
            result
        }
    }

    /// Compute the current frame string for an animation given the current time.
    ///
    /// The frame index is derived from the wall-clock milliseconds stored in
    /// `now`, so it respects the `disable_animations` flag: when animations
    /// are disabled `now` is frozen at construction time and the frame index
    /// does not change.
    fn compute_frame(anim: &PromptAnimation, now: &chrono::DateTime<chrono::Local>) -> String {
        if anim.frames.is_empty() {
            return String::new();
        }
        if anim.fps <= 0.0 {
            return anim.frames[0].clone();
        }
        let ms = now.timestamp_millis();
        let frame_duration_ms = (1000.0 / anim.fps) as i64;
        let frame_index = if frame_duration_ms > 0 {
            (ms / frame_duration_ms) as usize % anim.frames.len()
        } else {
            0
        };
        anim.frames[frame_index].clone()
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_anim(name: &str, fps: f64, frames: &[&str]) -> PromptAnimation {
        PromptAnimation {
            name: name.to_string(),
            fps,
            frames: frames.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn fixed_time(ms: i64) -> chrono::DateTime<chrono::Local> {
        chrono::Local.timestamp_millis_opt(ms).unwrap()
    }

    // --- compute_frame -------------------------------------------------------

    #[test]
    fn test_compute_frame_empty_frames() {
        let anim = make_anim("A", 10.0, &[]);
        assert_eq!(PromptManager::compute_frame(&anim, &fixed_time(0)), "");
    }

    #[test]
    fn test_compute_frame_single_frame() {
        let anim = make_anim("A", 10.0, &["only"]);
        // Always returns the single frame regardless of time.
        assert_eq!(PromptManager::compute_frame(&anim, &fixed_time(0)), "only");
        assert_eq!(
            PromptManager::compute_frame(&anim, &fixed_time(999)),
            "only"
        );
    }

    #[test]
    fn test_compute_frame_cycles() {
        // fps=10 → 100 ms per frame
        let anim = make_anim("A", 10.0, &["f0", "f1", "f2"]);
        // t=0 ms   → frame 0
        assert_eq!(PromptManager::compute_frame(&anim, &fixed_time(0)), "f0");
        // t=100 ms → frame 1
        assert_eq!(PromptManager::compute_frame(&anim, &fixed_time(100)), "f1");
        // t=200 ms → frame 2
        assert_eq!(PromptManager::compute_frame(&anim, &fixed_time(200)), "f2");
        // t=300 ms → wraps back to frame 0
        assert_eq!(PromptManager::compute_frame(&anim, &fixed_time(300)), "f0");
    }

    #[test]
    fn test_compute_frame_frozen_when_disabled() {
        // When disable_animations is true the caller passes construction_time
        // for every render.  Verify that the same time always yields the same
        // frame, regardless of how many frames the animation has.
        let anim = make_anim("A", 10.0, &["f0", "f1", "f2"]);
        let frozen = fixed_time(50); // 50 ms → frame 0 (0..100 ms range)
        assert_eq!(PromptManager::compute_frame(&anim, &frozen), "f0");
        assert_eq!(PromptManager::compute_frame(&anim, &frozen), "f0");
    }

    // --- extract_anim_codes --------------------------------------------------

    #[test]
    fn test_extract_anim_codes_no_animations() {
        let mut builder = PromptStringBuilder::new();
        let result = builder.extract_anim_codes("hello WORLD", &[]);
        assert_eq!(result, "hello WORLD");
        assert!(builder.anim_map.is_empty());
    }

    #[test]
    fn test_extract_anim_codes_replaces_name() {
        let animations = vec![make_anim("SPINNER", 10.0, &["a", "b"])];
        let mut builder = PromptStringBuilder::new();
        let result = builder.extract_anim_codes("prefix SPINNER suffix", &animations);
        // The name should have been replaced by a placeholder.
        assert!(!result.contains("SPINNER"));
        assert_eq!(builder.anim_map.len(), 1);
        let placeholder = builder.anim_map.keys().next().unwrap();
        assert!(result.contains(placeholder.as_str()));
        assert_eq!(*builder.anim_map.get(placeholder).unwrap(), 0usize);
    }

    #[test]
    fn test_extract_anim_codes_longer_name_first() {
        // SPINNER_BIG should be replaced as a unit, not SPINNER inside it.
        let animations = vec![
            make_anim("SPINNER", 10.0, &["a"]),
            make_anim("SPINNER_BIG", 5.0, &["x"]),
        ];
        let mut builder = PromptStringBuilder::new();
        let result = builder.extract_anim_codes("SPINNER_BIG end", &animations);
        // Result must not contain the literal "SPINNER_BIG" or "SPINNER".
        assert!(!result.contains("SPINNER_BIG"));
        assert!(!result.contains("SPINNER"));
        // Exactly one animation placeholder should have been inserted.
        assert_eq!(builder.anim_map.len(), 1);
        // The index should point to the longer animation (index 1).
        let &anim_idx = builder.anim_map.values().next().unwrap();
        assert_eq!(anim_idx, 1usize);
    }

    // --- expand_span (animation substitution) --------------------------------

    #[test]
    fn test_expand_span_plain_frame_substitution() {
        // Build a minimal PromptManager with one animation and a known anim_map.
        let anim = make_anim("SPIN", 10.0, &["f0", "f1"]);
        let mut anim_map = HashMap::new();
        anim_map.insert("FLYT0000".to_string(), 0usize);
        let pm = PromptManager {
            prompt: vec![],
            rprompt: vec![],
            fill_span: Line::from(""),
            time_map: HashMap::new(),
            anim_map,
            animations: vec![anim],
            construction_time: fixed_time(0),
        };

        // At t=0 ms, fps=10 → 100 ms/frame → frame 0 ("f0").
        let span = Span::raw("before FLYT0000 after");
        let now = fixed_time(0);
        let spans = pm.expand_span(span, &now);
        let content: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(content, "before f0 after");
    }

    #[test]
    fn test_expand_span_no_placeholder() {
        let pm = PromptManager {
            prompt: vec![],
            rprompt: vec![],
            fill_span: Line::from(""),
            time_map: HashMap::new(),
            anim_map: HashMap::new(),
            animations: vec![],
            construction_time: fixed_time(0),
        };
        let span = Span::raw("no placeholders here");
        let now = fixed_time(0);
        let spans = pm.expand_span(span.clone(), &now);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, span.content);
    }
}
