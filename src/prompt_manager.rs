use crate::bash_funcs;
use crate::bash_symbols;
use crate::settings::PromptAnimation;
use ansi_to_tui::IntoText;
use ratatui::text::{Line, Span};
use std::collections::HashMap;

/// An animation whose frames have been processed through
/// [`expand_prompt_through_bash`].  Embedded directly inside
/// [`PromptSegment::Animation`] so that each animation carries its own render
/// logic without requiring a separate lookup table on [`PromptManager`].
#[derive(Debug, Clone)]
struct ProcessedAnimation {
    /// The animation name as it appears literally in the raw PS1 string
    /// (e.g. `COOL_SPINNER`).  Retained for debugging.
    name: String,
    /// Playback speed in frames per second.
    fps: f64,
    /// Pre-processed frames.  Each frame has been run through
    /// [`expand_prompt_through_bash`] so bash prompt escapes (e.g. `\u`, `\w`)
    /// and ANSI colour codes are already resolved into [`Span`]s.
    frames: Vec<Vec<Span<'static>>>,
    /// When true the animation reverses direction at each end instead of
    /// wrapping around (ping-pong / bounce mode).
    ping_pong: bool,
}

/// A segment of a rendered prompt line.
///
/// Prompt strings are parsed into sequences of `PromptSegment`s at
/// construction time.  At render time each segment is cheaply converted to a
/// ratatui [`Span`]: static segments are used as-is, dynamic-time segments are
/// formatted with the current wall-clock time, and animation segments render
/// the appropriate frame for the current time.
#[derive(Debug, Clone)]
enum PromptSegment {
    /// A fully-resolved span (text + style) with no further substitution needed.
    Static(Span<'static>),
    /// A bash time escape sequence (`\t`, `\T`, `\@`, `\A`, `\D{…}`).
    /// Rendered by formatting the current time with the stored chrono
    /// format string and applying the span's style.
    DynamicTime {
        strftime: String,
        style: ratatui::style::Style,
    },
    /// A custom animation.  Rendered by selecting the appropriate frame for the
    /// current time and emitting its pre-resolved [`Span`]s directly.
    ///
    /// Boxed to keep the `PromptSegment` enum size small.
    Animation(Box<ProcessedAnimation>),
    /// The detected current-working-directory substring in the prompt.
    /// Holds the original span from the decoded prompt; at render time it is
    /// emitted as-is so it can be styled or replaced in the future.
    Cwd(Span<'static>),
}

pub struct PromptManager {
    prompt: Vec<Vec<PromptSegment>>,
    rprompt: Vec<Vec<PromptSegment>>,
    fill_span: Vec<PromptSegment>,
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

/// Pass a raw bash prompt string (with any time-code placeholders already
/// substituted) through bash's `decode_prompt_string`, then convert the
/// decoded output to a `Vec<Line<'static>>` via [`IntoText`].
///
/// `\[` / `\]` non-printing-sequence markers are stripped before the string is
/// handed to `decode_prompt_string` because they are Bash-specific and not
/// meaningful to ANSI parsers.  Trailing newlines and carriage returns are
/// stripped from each span.
///
/// Returns `None` when the string cannot be processed (e.g. contains interior
/// NUL bytes or bash returns a null pointer).
fn expand_prompt_through_bash(raw: String) -> Option<Vec<Line<'static>>> {
    if raw.is_empty() {
        return Some(vec![]);
    }

    // Strip literal `\[` / `\]` non-printing-sequence markers before handing
    // the string to `decode_prompt_string`.
    let raw = raw.replace("\\[", "").replace("\\]", "");

    let c_prompt = std::ffi::CString::new(raw).ok()?;

    let decoded = unsafe {
        let decoded_prompt_cstr = bash_symbols::decode_prompt_string(c_prompt.as_ptr(), 1);
        if decoded_prompt_cstr.is_null() {
            log::warn!("decode_prompt_string returned null");
            return None;
        }

        let decoded = std::ffi::CStr::from_ptr(decoded_prompt_cstr)
            .to_str()
            .ok()?
            .to_string();

        // `decode_prompt_string` returns an allocated buffer.
        bash_symbols::xfree(decoded_prompt_cstr as *mut std::ffi::c_void);

        decoded
    };

    let mut lines = decoded.into_text().ok()?.lines;
    for line in &mut lines {
        for span in &mut line.spans {
            let raw = span.content.as_ref();
            let stripped = raw.trim_end_matches(&['\n', '\r'][..]);
            if stripped.len() != raw.len() {
                log::debug!("Stripping trailing newline/carriage return from prompt line span");
                span.content = stripped.to_owned().into();
            }
        }
    }

    Some(lines)
}

/// Builds expanded prompt segment lines from raw bash prompt strings while
/// accumulating a shared map of time-placeholder identifiers to chrono format
/// strings and holding pre-processed animation data.
///
/// A single `PromptStringBuilder` should be used for all prompt variables
/// (PS1, RPS1 / RPROMPT, PS1_FILL) so that placeholder identifiers are unique
/// across all of them.
struct PromptStringBuilder {
    /// Monotonically increasing counter used to generate unique placeholder IDs.
    counter: u32,
    /// Accumulated map of placeholder → chrono format string.
    /// Used during `expand_prompt_string` to recognise which spans contain
    /// time placeholders and convert them into [`PromptSegment::DynamicTime`].
    time_map: HashMap<String, String>,
    /// Pre-processed animations.  Used by [`expand_span_to_segments`] to
    /// recognise animation-name placeholders and produce
    /// [`PromptSegment::Animation`] segments in the same pass.
    animations: Vec<ProcessedAnimation>,
    /// Current working directory, used to detect CWD substrings in prompt spans.
    cwd: Option<String>,
    /// Home directory, used to recognise `~`-prefixed path representations.
    home: Option<String>,
}

impl PromptStringBuilder {
    fn new(animations: Vec<ProcessedAnimation>) -> Self {
        Self {
            counter: 0,
            time_map: HashMap::new(),
            animations,
            cwd: None,
            home: None,
        }
    }

    /// Set the current working directory and home directory for CWD detection.
    fn with_cwd(mut self, cwd: String, home: Option<String>) -> Self {
        self.cwd = Some(cwd);
        self.home = home;
        self
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

    /// Expand a raw prompt string (e.g. from `PS1`, `RPS1`, or `PS1_FILL`)
    /// into a sequence of lines, each line being a sequence of
    /// [`PromptSegment`]s.
    ///
    /// The pipeline is:
    /// 1. [`extract_time_codes`] — replace bash time escape sequences with
    ///    unique placeholders, recording the mapping in `self.time_map`.
    /// 2. [`expand_prompt_through_bash`] — run the modified string through
    ///    bash's `decode_prompt_string` and parse ANSI colour codes into
    ///    `Line<'static>` values.
    /// 3. [`expand_span_to_segments`] — split each decoded span at
    ///    time-placeholder boundaries, producing `Static` or `DynamicTime`
    ///    segments.
    ///
    /// Returns `None` when the string cannot be processed.
    fn expand_prompt_string(&mut self, raw: String) -> Option<Vec<Vec<PromptSegment>>> {
        let modified = self.extract_time_codes(&raw);
        let lines = expand_prompt_through_bash(modified)?;
        let result = lines
            .into_iter()
            .map(|line| {
                line.spans
                    .into_iter()
                    .flat_map(|span| self.expand_span_to_segments(span))
                    .collect()
            })
            .collect();
        Some(result)
    }

    /// Split a single decoded [`Span`] into a sequence of [`PromptSegment`]s.
    ///
    /// Two passes are performed:
    /// 1. Split on time-placeholder strings: portions matching a placeholder
    ///    become [`PromptSegment::DynamicTime`]; the rest stay
    ///    [`PromptSegment::Static`].
    /// 2. Split each remaining `Static` segment on animation names: each
    ///    occurrence of an animation name becomes a
    ///    [`PromptSegment::Animation`] carrying a clone of the matching
    ///    [`ProcessedAnimation`].
    fn expand_span_to_segments(&self, span: Span<'static>) -> Vec<PromptSegment> {
        let raw = span.content.as_ref();

        let has_placeholder =
            !self.time_map.is_empty() && self.time_map.keys().any(|id| raw.contains(id.as_str()));

        let time_segs: Vec<PromptSegment> = if has_placeholder {
            let style = span.style;
            let mut result: Vec<PromptSegment> = Vec::new();
            let mut remaining = raw.to_owned();

            loop {
                // Find the placeholder that appears earliest in `remaining`.
                let next = self
                    .time_map
                    .iter()
                    .filter_map(|(id, fmt)| {
                        remaining
                            .find(id.as_str())
                            .map(|pos| (pos, id.len(), fmt.clone()))
                    })
                    .min_by_key(|(pos, _, _)| *pos);

                let (pos, id_len, fmt) = match next {
                    None => break,
                    Some(t) => t,
                };

                if pos > 0 {
                    result.push(PromptSegment::Static(Span::styled(
                        remaining[..pos].to_owned(),
                        style,
                    )));
                }

                result.push(PromptSegment::DynamicTime {
                    strftime: fmt,
                    style,
                });

                remaining = remaining[pos + id_len..].to_owned();
            }

            if !remaining.is_empty() {
                result.push(PromptSegment::Static(Span::styled(remaining, style)));
            }

            if result.is_empty() {
                result.push(PromptSegment::Static(Span::styled(String::new(), style)));
            }

            result
        } else {
            vec![PromptSegment::Static(span)]
        };

        // Pass 2: split any remaining Static segments on animation names.
        let animation_segs: Vec<PromptSegment> = if self.animations.is_empty() {
            time_segs
        } else {
            time_segs
                .into_iter()
                .flat_map(|seg| match seg {
                    PromptSegment::Static(s) => self.split_static_span_by_animations(s),
                    other => vec![other],
                })
                .collect()
        };

        // Pass 3: split any remaining Static segments on CWD.
        if let Some(cwd) = self.cwd.as_deref() {
            let home = self.home.as_deref();
            animation_segs
                .into_iter()
                .flat_map(|seg| match seg {
                    PromptSegment::Static(s) => split_static_span_by_cwd(s, cwd, home),
                    other => vec![other],
                })
                .collect()
        } else {
            animation_segs
        }
    }

    /// Split a static [`Span`] at animation-name boundaries, producing
    /// `Static` and `Animation` segments.
    ///
    /// Runs a single greedy loop that always picks the earliest-occurring
    /// animation name in the remaining text.  Returns at least one segment;
    /// if no animation names are found the original span is returned unchanged
    /// as a `Static` segment.
    fn split_static_span_by_animations(&self, span: Span<'static>) -> Vec<PromptSegment> {
        let style = span.style;
        let mut result: Vec<PromptSegment> = Vec::new();
        let mut remaining: String = span.content.into_owned();

        loop {
            // Find the animation whose name appears earliest in `remaining`.
            let next = self
                .animations
                .iter()
                .enumerate()
                .filter_map(|(i, anim)| {
                    remaining
                        .find(anim.name.as_str())
                        .map(|pos| (pos, i, anim.name.len()))
                })
                .min_by_key(|(pos, _, _)| *pos);

            let (pos, anim_idx, name_len) = match next {
                None => break,
                Some(m) => m,
            };

            if pos > 0 {
                result.push(PromptSegment::Static(Span::styled(
                    remaining[..pos].to_owned(),
                    style,
                )));
            }

            result.push(PromptSegment::Animation(Box::new(
                self.animations[anim_idx].clone(),
            )));

            remaining = remaining[pos + name_len..].to_owned();
        }

        if !remaining.is_empty() {
            result.push(PromptSegment::Static(Span::styled(remaining, style)));
        }

        // Ensure at least one segment is always returned (e.g. for an empty
        // span with no matching animation name).
        if result.is_empty() {
            result.push(PromptSegment::Static(Span::styled(String::new(), style)));
        }

        result
    }

    /// Allocate the next placeholder identifier and advance the counter.
    fn next_id(&mut self) -> String {
        let id = format!("FLYT{:04X}", self.counter);
        self.counter += 1;
        id
    }
}

/// Find the longest substring of `text` that looks like the current working
/// directory or any contiguous sub-path of it.
///
/// * `cwd` – the current working directory (absolute or relative path string).
/// * `home` – optionally the user's home directory; when provided the function
///   also searches for `~`-prefixed representations such as `~`, `~/`, or
///   `~/subdir/…`.
///
/// The function generates all contiguous subsequences of cwd path segments
/// (e.g. for `/home/foo/bar` it tries `home`, `foo`, `bar`, `home/foo`,
/// `foo/bar`, `home/foo/bar`, plus tilde variants when `home` is supplied)
/// and returns the byte range of the longest one found in `text`.
///
/// Returns `Some((start, end))` so that `text[start..end]` is the matched
/// substring, or `None` if no path-like substring is found.
fn find_cwd_in_span(text: &str, cwd: &str, home: Option<&str>) -> Option<(usize, usize)> {
    let cwd_trimmed = cwd.trim_end_matches('/');

    // Collect non-empty path segments.
    let segments: Vec<&str> = cwd_trimmed.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return None;
    }

    let mut best: Option<(usize, usize)> = None;

    // Update `best` if `candidate` is found in `text` and is longer than the
    // current best match.
    let mut try_candidate = |candidate: &str| {
        if candidate.is_empty() {
            return;
        }
        if let Some(pos) = text.find(candidate) {
            let end = pos + candidate.len();
            if best.map_or(true, |(bs, be)| end - pos > be - bs) {
                best = Some((pos, end));
            }
        }
    };

    // --- Tilde representations ---
    if let Some(home_dir) = home {
        let home_trimmed = home_dir.trim_end_matches('/');
        if !home_trimmed.is_empty() && cwd_trimmed.starts_with(home_trimmed) {
            let after_home = &cwd_trimmed[home_trimmed.len()..];
            // after_home is "" or "/seg1/seg2/…"
            let rest_segments: Vec<&str> = after_home
                .trim_start_matches('/')
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();

            // "~" alone (home dir exact match) and "~/" (with trailing slash)
            try_candidate("~");
            try_candidate("~/");

            // Build "~/seg1", "~/seg1/seg2", … trying both with and without
            // a trailing slash at each level.
            let mut tilde_path = String::from("~");
            for seg in &rest_segments {
                tilde_path.push('/');
                tilde_path.push_str(seg);
                let with_slash = tilde_path.clone() + "/";
                try_candidate(&with_slash);
                try_candidate(&tilde_path);
            }
        }
    }

    // --- All contiguous subsequences of cwd segments ---
    // For each starting segment index, build paths of increasing length.
    for start_idx in 0..segments.len() {
        let mut path = String::new();
        for (j, seg) in segments[start_idx..].iter().enumerate() {
            if j > 0 {
                path.push('/');
            }
            path.push_str(seg);
            let with_slash = path.clone() + "/";
            try_candidate(&with_slash);
            try_candidate(&path);
        }
    }

    // Also try the full absolute path (with leading "/").
    if !segments.is_empty() {
        let full = "/".to_string() + &segments.join("/");
        let with_slash = full.clone() + "/";
        try_candidate(&with_slash);
        try_candidate(&full);
    }

    best
}

/// Split a static [`Span`] at the first (longest) detected CWD substring,
/// producing up to three segments: an optional `Static` prefix, a `Cwd`
/// segment for the matched path text, and an optional `Static` suffix.
///
/// If no CWD-like substring is found the original span is returned unchanged
/// as a single `Static` segment.
fn split_static_span_by_cwd(
    span: Span<'static>,
    cwd: &str,
    home: Option<&str>,
) -> Vec<PromptSegment> {
    let text = span.content.as_ref().to_owned();
    let style = span.style;

    let Some((start, end)) = find_cwd_in_span(&text, cwd, home) else {
        return vec![PromptSegment::Static(span)];
    };

    let mut result = Vec::new();
    if start > 0 {
        result.push(PromptSegment::Static(Span::styled(
            text[..start].to_owned(),
            style,
        )));
    }
    result.push(PromptSegment::Cwd(Span::styled(
        text[start..end].to_owned(),
        style,
    )));
    if end < text.len() {
        result.push(PromptSegment::Static(Span::styled(
            text[end..].to_owned(),
            style,
        )));
    }
    result
}

/// Convert a slice of [`PromptSegment`]s to a [`Line`] by resolving each
/// segment against `now`.
fn format_prompt_line(
    segments: Vec<PromptSegment>,
    now: &chrono::DateTime<chrono::Local>,
) -> Line<'static> {
    let spans: Vec<Span<'static>> = segments
        .into_iter()
        .flat_map(|segment| match segment {
            PromptSegment::Static(span) => vec![span],
            PromptSegment::Cwd(span) => vec![span],
            PromptSegment::DynamicTime { strftime, style } => {
                vec![Span::styled(now.format(&strftime).to_string(), style)]
            }
            PromptSegment::Animation(anim) => get_frame_spans(&anim, now).to_vec(),
        })
        .collect();
    Line::from(spans)
}

/// Return the pre-processed [`Span`]s for the current animation frame.
///
/// The frame index is derived from the wall-clock milliseconds in `now`.
/// When `ping_pong` is enabled the animation bounces: it plays forward to
/// the last frame and then reverses back to the first, rather than
/// wrapping around.
fn get_frame_spans<'a>(
    anim: &'a ProcessedAnimation,
    now: &chrono::DateTime<chrono::Local>,
) -> &'a [Span<'static>] {
    if anim.frames.is_empty() {
        return &[];
    }
    if anim.fps <= 0.0 {
        return &anim.frames[0];
    }
    let ms = now.timestamp_millis();
    let frame_duration_ms = (1000.0 / anim.fps) as i64;
    let tick = if frame_duration_ms > 0 {
        (ms / frame_duration_ms) as usize
    } else {
        0
    };
    let n = anim.frames.len();
    let frame_index = if anim.ping_pong && n > 1 {
        // Period: forward (n frames) + reverse (n-2 inner frames) = 2*(n-1)
        let period = 2 * (n - 1);
        let pos = tick % period;
        if pos < n { pos } else { period - pos }
    } else {
        tick % n
    };
    &anim.frames[frame_index]
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
                    vec![
                        PromptSegment::Static(Span::styled(
                            "Bash needs more input to finish the command. ",
                            style,
                        )),
                        PromptSegment::Static(Span::styled(
                            "Flyline thought the previous command was complete. ",
                            style,
                        )),
                        PromptSegment::Static(Span::styled(
                            "Please open an issue on GitHub with the command that caused this message. ",
                            style,
                        )),
                    ],
                    vec![PromptSegment::Static(Span::raw("> "))],
                ],
                rprompt: vec![],
                fill_span: vec![PromptSegment::Static(Span::raw(" "))],
                construction_time: chrono::Local::now(),
            }
        } else {
            const PS1_DEFAULT: &str = "bad ps1> ";

            // Process each animation frame through expand_prompt_through_bash
            // so frames are resolved to plain Spans only.
            let processed_animations: Vec<ProcessedAnimation> = animations
                .iter()
                .map(|anim| {
                    let frames: Vec<Vec<Span<'static>>> = anim
                        .frames
                        .iter()
                        .map(|raw_frame| {
                            expand_prompt_through_bash(raw_frame.clone())
                                .unwrap_or_default()
                                .into_iter()
                                .flat_map(|line| line.spans)
                                .collect()
                        })
                        .collect();
                    ProcessedAnimation {
                        name: anim.name.clone(),
                        fps: anim.fps,
                        frames,
                        ping_pong: anim.ping_pong,
                    }
                })
                .collect();

            log::debug!("Animation count: {}", processed_animations.len());

            // A single builder is shared across all prompt variables so that
            // placeholder IDs are unique.  Animations are passed in so that
            // expand_span_to_segments can produce Animation segments directly.
            let cwd = bash_funcs::get_cwd();
            let home = bash_funcs::get_envvar_value("HOME");
            log::debug!("CWD for prompt detection: {:?}, HOME: {:?}", cwd, home);
            let mut builder = PromptStringBuilder::new(processed_animations).with_cwd(cwd, home);

            // Read the raw PS1 env var so we can intercept time format codes
            // before handing the string to decode_prompt_string.  Fall back to
            // the already-expanded readline prompt when PS1 is not available.
            let ps1_raw = bash_funcs::get_envvar_value("PS1").or_else(get_current_readline_prompt);

            let ps1 = ps1_raw
                .and_then(|raw| builder.expand_prompt_string(raw))
                .unwrap_or_else(|| {
                    log::warn!("Failed to parse PS1, defaulting to '{}'", PS1_DEFAULT);
                    vec![vec![PromptSegment::Static(Span::raw(PS1_DEFAULT))]]
                });

            // Examples:
            // RPS1='\e[01;32m\t\e[0m'
            // export RPROMPT='\e[01;32m\D{%H:%M:%S}\e[0m'
            let rps1 = bash_funcs::get_envvar_value("RPS1")
                .or_else(|| bash_funcs::get_envvar_value("RPROMPT"))
                .and_then(|raw| builder.expand_prompt_string(raw))
                .unwrap_or_default();

            log::debug!("Parsed RPS1: {:?}", rps1);

            let fill_span = bash_funcs::get_envvar_value("PS1_FILL")
                .and_then(|raw| builder.expand_prompt_string(raw))
                .and_then(|lines| lines.into_iter().next())
                .unwrap_or_else(|| vec![PromptSegment::Static(Span::raw(" "))]);

            PromptManager {
                prompt: ps1,
                rprompt: rps1,
                fill_span,
                construction_time: chrono::Local::now(),
            }
        }
    }

    pub fn get_ps1_lines(
        &mut self,
        show_animations: bool,
    ) -> (Vec<Line<'static>>, Vec<Line<'static>>, Line<'static>) {
        use chrono::Local;
        let now = if show_animations {
            Local::now()
        } else {
            self.construction_time
        };

        let formatted_prompt: Vec<Line<'static>> = self
            .prompt
            .clone()
            .into_iter()
            .map(|line| format_prompt_line(line, &now))
            .collect();

        let formatted_rprompt: Vec<Line<'static>> = self
            .rprompt
            .clone()
            .into_iter()
            .map(|line| format_prompt_line(line, &now))
            .collect();

        let formatted_fill = format_prompt_line(self.fill_span.clone(), &now);

        (formatted_prompt, formatted_rprompt, formatted_fill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time(ms: i64) -> chrono::DateTime<chrono::Local> {
        chrono::Local.timestamp_millis_opt(ms).unwrap()
    }

    /// Build a `ProcessedAnimation` where each frame is a single span,
    /// suitable for unit-testing without any bash FFI calls.
    fn make_processed_anim(name: &str, fps: f64, frames: &[&str]) -> ProcessedAnimation {
        ProcessedAnimation {
            name: name.to_string(),
            fps,
            frames: frames
                .iter()
                .map(|s| vec![Span::raw(s.to_string())])
                .collect(),
            ping_pong: false,
        }
    }

    /// Build a ping-pong `ProcessedAnimation` for unit-testing.
    fn make_ping_pong_anim(name: &str, fps: f64, frames: &[&str]) -> ProcessedAnimation {
        ProcessedAnimation {
            name: name.to_string(),
            fps,
            frames: frames
                .iter()
                .map(|s| vec![Span::raw(s.to_string())])
                .collect(),
            ping_pong: true,
        }
    }

    /// Extract the text content of the first span in a frame slice.
    fn first_span_content(spans: &[Span<'static>]) -> std::borrow::Cow<'static, str> {
        assert!(!spans.is_empty(), "expected at least one span");
        spans[0].content.clone()
    }

    // --- get_frame_spans (frame index selection) --------------------------

    #[test]
    fn test_get_frame_spans_empty_frames() {
        let anim = make_processed_anim("A", 10.0, &[]);
        assert!(get_frame_spans(&anim, &fixed_time(0)).is_empty());
    }

    #[test]
    fn test_get_frame_spans_single_frame() {
        let anim = make_processed_anim("A", 10.0, &["only"]);
        // Always returns the single frame regardless of time.
        let spans_at_0 = get_frame_spans(&anim, &fixed_time(0));
        assert_eq!(first_span_content(spans_at_0), "only");

        let spans_at_999 = get_frame_spans(&anim, &fixed_time(999));
        assert_eq!(first_span_content(spans_at_999), "only");
    }

    #[test]
    fn test_get_frame_spans_cycles() {
        // fps=10 → 100 ms per frame
        let anim = make_processed_anim("A", 10.0, &["f0", "f1", "f2"]);
        let frame_content = |ms| {
            let spans = get_frame_spans(&anim, &fixed_time(ms));
            first_span_content(spans)
        };
        assert_eq!(frame_content(0), "f0");
        assert_eq!(frame_content(100), "f1");
        assert_eq!(frame_content(200), "f2");
        assert_eq!(frame_content(300), "f0"); // wraps
    }

    #[test]
    fn test_get_frame_spans_ping_pong_three_frames() {
        // fps=10 → 100 ms per frame; frames: f0, f1, f2
        // ping-pong sequence: f0, f1, f2, f1, f0, f1, f2, ...  (period = 4)
        let anim = make_ping_pong_anim("A", 10.0, &["f0", "f1", "f2"]);
        let frame_content = |ms| {
            let spans = get_frame_spans(&anim, &fixed_time(ms));
            first_span_content(spans)
        };
        assert_eq!(frame_content(0), "f0"); // tick 0
        assert_eq!(frame_content(100), "f1"); // tick 1
        assert_eq!(frame_content(200), "f2"); // tick 2 – last frame
        assert_eq!(frame_content(300), "f1"); // tick 3 – reversed
        assert_eq!(frame_content(400), "f0"); // tick 4 – wraps back to start
        assert_eq!(frame_content(500), "f1"); // tick 5 – forward again
    }

    #[test]
    fn test_get_frame_spans_ping_pong_two_frames() {
        // fps=10 → 100 ms per frame; frames: f0, f1
        // period = 2*(2-1) = 2 → same as normal cycling for two frames
        let anim = make_ping_pong_anim("A", 10.0, &["f0", "f1"]);
        let frame_content = |ms| {
            let spans = get_frame_spans(&anim, &fixed_time(ms));
            first_span_content(spans)
        };
        assert_eq!(frame_content(0), "f0");
        assert_eq!(frame_content(100), "f1");
        assert_eq!(frame_content(200), "f0"); // wraps
    }

    #[test]
    fn test_get_frame_spans_ping_pong_single_frame() {
        // A single-frame ping-pong animation should always return that frame.
        let anim = make_ping_pong_anim("A", 10.0, &["only"]);
        for ms in [0, 100, 200, 999] {
            let spans = get_frame_spans(&anim, &fixed_time(ms));
            assert_eq!(first_span_content(spans), "only");
        }
    }

    #[test]
    fn test_get_frame_spans_frozen_when_disabled() {
        // When disable_animations is true the caller passes construction_time
        // for every render.  Verify that the same time always yields the same frame.
        let anim = make_processed_anim("A", 10.0, &["f0", "f1", "f2"]);
        let frozen = fixed_time(50); // 50 ms → frame 0 (0..100 ms range)
        assert_eq!(first_span_content(get_frame_spans(&anim, &frozen)), "f0");
        assert_eq!(first_span_content(get_frame_spans(&anim, &frozen)), "f0");
    }

    // --- split_span_by_animations (now tested via expand_span_to_segments) ---

    #[test]
    fn test_expand_span_animation_not_present() {
        let anim = make_processed_anim("SPIN", 10.0, &["f0"]);
        let builder = PromptStringBuilder::new(vec![anim]);
        let span = Span::raw("no spinner here");
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 1);
        match &segs[0] {
            PromptSegment::Static(s) => assert_eq!(s.content, "no spinner here"),
            _ => panic!("expected Static"),
        }
    }

    #[test]
    fn test_expand_span_animation_name_only() {
        let anim = make_processed_anim("SPIN", 10.0, &["f0", "f1"]);
        let builder = PromptStringBuilder::new(vec![anim]);
        let span = Span::raw("SPIN");
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 1);
        match &segs[0] {
            PromptSegment::Animation(a) => assert_eq!(a.name, "SPIN"),
            _ => panic!("expected Animation"),
        }
    }

    #[test]
    fn test_expand_span_animation_name_surrounded_by_text() {
        let anim = make_processed_anim("SPIN", 10.0, &["f0"]);
        let builder = PromptStringBuilder::new(vec![anim]);
        let span = Span::raw("before SPIN after");
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 3);
        match &segs[0] {
            PromptSegment::Static(s) => assert_eq!(s.content, "before "),
            _ => panic!("expected Static at index 0"),
        }
        match &segs[1] {
            PromptSegment::Animation(a) => assert_eq!(a.name, "SPIN"),
            _ => panic!("expected Animation at index 1"),
        }
        match &segs[2] {
            PromptSegment::Static(s) => assert_eq!(s.content, " after"),
            _ => panic!("expected Static at index 2"),
        }
    }

    // --- format_prompt_line (Animation rendering) ----------------------------

    #[test]
    fn test_format_prompt_line_animation_substitution() {
        // At t=0 ms, fps=10 → 100 ms/frame → frame 0 ("f0").
        let anim = make_processed_anim("SPIN", 10.0, &["f0", "f1"]);
        let segments = vec![
            PromptSegment::Static(Span::raw("before ")),
            PromptSegment::Animation(Box::new(anim)),
            PromptSegment::Static(Span::raw(" after")),
        ];
        let now = fixed_time(0);
        let line = format_prompt_line(segments, &now);
        let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(content, "before f0 after");
    }

    #[test]
    fn test_format_prompt_line_animation_frame_advances() {
        // At t=100 ms, fps=10 → frame 1 ("f1").
        let anim = make_processed_anim("SPIN", 10.0, &["f0", "f1"]);
        let segments = vec![PromptSegment::Animation(Box::new(anim))];
        let line = format_prompt_line(segments, &fixed_time(100));
        let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(content, "f1");
    }

    // --- format_prompt_line (DynamicTime rendering) --------------------------

    #[test]
    fn test_format_prompt_line_dynamic_time() {
        // Use a fixed time to produce a predictable formatted string.  The actual
        // HH:MM:SS value is timezone-dependent, so we compute the expected value
        // with the same `now` and format string rather than hard-coding a literal.
        let now = fixed_time(0);
        let formatted_time = now.format("%H:%M:%S").to_string();

        let segments = vec![
            PromptSegment::Static(Span::raw("[")),
            PromptSegment::DynamicTime {
                strftime: "%H:%M:%S".to_string(),
                style: ratatui::style::Style::default(),
            },
            PromptSegment::Static(Span::raw("]")),
        ];
        let line = format_prompt_line(segments, &now);
        let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(content, format!("[{}]", formatted_time));
    }

    // --- expand_span_to_segments (time-code splitting) -----------------------

    #[test]
    fn test_expand_span_to_segments_no_placeholders() {
        let builder = PromptStringBuilder::new(vec![]);
        let span = Span::raw("hello world");
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 1);
        match &segs[0] {
            PromptSegment::Static(s) => assert_eq!(s.content, "hello world"),
            _ => panic!("expected Static"),
        }
    }

    #[test]
    fn test_expand_span_to_segments_single_placeholder() {
        let mut builder = PromptStringBuilder::new(vec![]);
        let id = builder.next_id();
        builder.time_map.insert(id.clone(), "%H:%M:%S".to_string());

        let span = Span::raw(id.clone());
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 1);
        match &segs[0] {
            PromptSegment::DynamicTime { strftime, .. } => {
                assert_eq!(strftime, "%H:%M:%S");
            }
            _ => panic!("expected DynamicTime"),
        }
    }

    #[test]
    fn test_expand_span_to_segments_placeholder_surrounded_by_text() {
        let mut builder = PromptStringBuilder::new(vec![]);
        let id = builder.next_id();
        builder.time_map.insert(id.clone(), "%H:%M".to_string());

        let span = Span::raw(format!("prefix {} suffix", id));
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 3);
        match &segs[0] {
            PromptSegment::Static(s) => assert_eq!(s.content, "prefix "),
            _ => panic!("expected Static at index 0"),
        }
        match &segs[1] {
            PromptSegment::DynamicTime { strftime, .. } => assert_eq!(strftime, "%H:%M"),
            _ => panic!("expected DynamicTime at index 1"),
        }
        match &segs[2] {
            PromptSegment::Static(s) => assert_eq!(s.content, " suffix"),
            _ => panic!("expected Static at index 2"),
        }
    }

    // --- find_cwd_in_span ----------------------------------------------------

    /// Helper: cwd = /home/foo/qwe/try/ooh/lkj, home = /home/foo
    fn cwd() -> &'static str {
        "/home/foo/qwe/try/ooh/lkj"
    }
    fn home() -> &'static str {
        "/home/foo"
    }

    #[test]
    fn test_find_cwd_tilde_only() {
        // "~" alone should be detected as the home dir representation
        let text = "otherpartofheprompt~:$";
        let result = find_cwd_in_span(text, cwd(), Some(home()));
        let (start, end) = result.expect("should find a match");
        assert_eq!(&text[start..end], "~");
    }

    #[test]
    fn test_find_cwd_tilde_slash() {
        // "~/" should be preferred over bare "~"
        let text = "otherpartofheprompt~/:$";
        let result = find_cwd_in_span(text, cwd(), Some(home()));
        let (start, end) = result.expect("should find a match");
        assert_eq!(&text[start..end], "~/");
    }

    #[test]
    fn test_find_cwd_tilde_one_segment() {
        let text = "otherpartofheprompt~/qwe:$";
        let result = find_cwd_in_span(text, cwd(), Some(home()));
        let (start, end) = result.expect("should find a match");
        assert_eq!(&text[start..end], "~/qwe");
    }

    #[test]
    fn test_find_cwd_tilde_two_segments() {
        let text = "otherpartofheprompt~/qwe/try:$";
        let result = find_cwd_in_span(text, cwd(), Some(home()));
        let (start, end) = result.expect("should find a match");
        assert_eq!(&text[start..end], "~/qwe/try");
    }

    #[test]
    fn test_find_cwd_partial_path_no_tilde() {
        // "qwe/try/ooh" is a contiguous sub-path; no tilde in the text
        let text = "otherpartofhepromptqwe/try/ooh:$";
        let result = find_cwd_in_span(text, cwd(), Some(home()));
        let (start, end) = result.expect("should find a match");
        assert_eq!(&text[start..end], "qwe/try/ooh");
    }

    #[test]
    fn test_find_cwd_partial_path_trailing_slash() {
        // trailing slash is part of the detected match
        let text = "otherpartofheprompt try/ooh/lkj/:$";
        let result = find_cwd_in_span(text, cwd(), Some(home()));
        let (start, end) = result.expect("should find a match");
        assert_eq!(&text[start..end], "try/ooh/lkj/");
    }

    // --- expand_span_to_segments with CWD detection --------------------------

    #[test]
    fn test_expand_span_cwd_tilde_only() {
        let builder = PromptStringBuilder::new(vec![]).with_cwd(
            "/home/foo/qwe/try/ooh/lkj".to_string(),
            Some("/home/foo".to_string()),
        );
        let span = Span::raw("otherpartofheprompt~:$");
        let segs = builder.expand_span_to_segments(span);
        let cwd_seg = segs.iter().find(|s| matches!(s, PromptSegment::Cwd(_)));
        match cwd_seg.expect("expected a Cwd segment") {
            PromptSegment::Cwd(s) => assert_eq!(s.content, "~"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_expand_span_cwd_partial_no_tilde() {
        let builder = PromptStringBuilder::new(vec![]).with_cwd(
            "/home/foo/qwe/try/ooh/lkj".to_string(),
            Some("/home/foo".to_string()),
        );
        let span = Span::raw("otherpartofhepromptqwe/try/ooh:$");
        let segs = builder.expand_span_to_segments(span);
        let cwd_seg = segs.iter().find(|s| matches!(s, PromptSegment::Cwd(_)));
        match cwd_seg.expect("expected a Cwd segment") {
            PromptSegment::Cwd(s) => assert_eq!(s.content, "qwe/try/ooh"),
            _ => unreachable!(),
        }
    }
}
