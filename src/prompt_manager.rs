use crate::bash_funcs;
use crate::bash_symbols;
use crate::content_builder::{Tag, TaggedLine, TaggedSpan};
use crate::settings::{Placeholder, PromptAnimation, PromptWidget, PromptWidgetCustom};
#[cfg(not(test))]
use ansi_to_tui::IntoText;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

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

impl ProcessedAnimation {
    pub fn patch_style(mut self, style: Style) -> Self {
        for frame in &mut self.frames {
            for span in frame {
                span.style = span.style.patch(style);
            }
        }
        self
    }
}

/// State of a running custom widget command.
///
/// Held directly inside `PromptSegment::WidgetCustom`.  Each segment is fully
/// independent: the same widget name appearing in PS1 and RPS1 produces two
/// independent segments that each run the command separately.
enum WidgetCustomState {
    /// Command is still running (or has not yet been polled).
    Pending {
        placeholder: Vec<TaggedSpan<'static>>,
        child: std::process::Child,
        /// The command that was spawned, retained for log messages.
        command: Vec<String>,
        /// Shared storage to write the output into when the command finishes,
        /// so that `Placeholder::Prev` can use it on the next render cycle.
        prev_output_cell: Arc<Mutex<Vec<TaggedSpan<'static>>>>,
    },
    /// Command finished successfully; pre-tagged output spans are stored here
    /// so that the tag is applied only once rather than on every render.
    Done(Vec<TaggedSpan<'static>>),
    /// Command exited with a non-zero exit code or could not be spawned.
    Failed(WidgetFailure),
}

impl std::fmt::Debug for WidgetCustomState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WidgetCustomState::Pending { .. } => f.write_str("WidgetCustomState::Pending"),
            WidgetCustomState::Done(_) => f.write_str("WidgetCustomState::Done"),
            WidgetCustomState::Failed(_) => f.write_str("WidgetCustomState::Failed"),
        }
    }
}

impl Drop for WidgetCustomState {
    /// Kill and reap a still-running child process when the state is dropped.
    fn drop(&mut self) {
        if let WidgetCustomState::Pending { child, .. } = self {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Failure details returned by a custom widget command.
struct WidgetFailure {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl WidgetFailure {
    /// Build a set of error [`TaggedSpan`]s that visualise this failure in the
    /// prompt.  The label is `"command failed (exit N)"` when an exit code is
    /// available, or `"command failed"` for spawn errors.  The full stdout and
    /// stderr are emitted at `debug` level.
    fn to_error_spans(&self) -> Vec<TaggedSpan<'static>> {
        let style = Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK);
        let label = match self.exit_code {
            Some(code) => format!("command failed (exit {})", code),
            None => "command failed".to_string(),
        };
        log::debug!(
            "Custom widget failed — stdout: {:?}  stderr: {:?}",
            self.stdout,
            self.stderr
        );
        vec![TaggedSpan::new(Span::styled(label, style), Tag::Ps1Prompt)]
    }
}

/// A segment of a rendered prompt line.
///
/// Prompt strings are parsed into sequences of `PromptSegment`s at
/// construction time.  At render time each segment is cheaply converted to a
/// ratatui [`Span`]: static segments are used as-is, dynamic-time segments are
/// formatted with the current wall-clock time, and animation segments render
/// the appropriate frame for the current time.
#[derive(Debug)]
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
    /// Holds one span per path segment, where each segment owns the slash
    /// to its left.  E.g. `~/foo/bar` is stored as `["~", "/foo", "/bar"]`.
    /// At render time the spans are emitted in order with per-segment tags.
    Cwd(Vec<Span<'static>>),
    /// A mouse-mode widget.  Rendered as `enabled_text` when mouse capture is
    /// active, otherwise `disabled_text`.
    ///
    /// Both texts are stored as pre-tagged [`TaggedSpan`]s so that the tag is
    /// applied once at construction rather than on every render.
    WidgetMouseMode {
        enabled_text: Vec<TaggedSpan<'static>>,
        disabled_text: Vec<TaggedSpan<'static>>,
    },
    /// A custom-command widget.  On each render the child process is polled
    /// with `try_wait`; once it exits the output (processed through
    /// `expand_prompt_through_bash`) is shown.  While still pending the
    /// placeholder string is shown.
    ///
    /// Each segment is fully independent: two occurrences of the same widget
    /// name in a prompt string each run their own process.
    WidgetCustom(WidgetCustomState),
}

pub struct PromptManager {
    prompt: Vec<Vec<PromptSegment>>,
    rprompt: Vec<Vec<PromptSegment>>,
    fill_span: Vec<PromptSegment>,
    /// Time captured at construction; used when animations are disabled so
    /// that time-based prompt fields show the session-start time rather than
    /// updating on every render.
    construction_time: chrono::DateTime<chrono::Local>,
    /// The current working directory at the time the prompt was constructed.
    /// Used by `PromptCwdEdit` mode to compute the path for a given CWD segment index.
    cwd: String,
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
#[cfg(not(test))]
fn expand_prompt_through_bash(raw: String) -> Option<Vec<Line<'static>>> {
    if raw.is_empty() {
        return Some(vec![]);
    }

    // Strip literal `\[` / `\]` non-printing-sequence markers before handing
    // the string to `decode_prompt_string`.
    let raw = raw.replace("\\[", "").replace("\\]", "");

    let c_prompt = std::ffi::CString::new(raw).ok()?;

    let decoded = unsafe {
        #[cfg(not(feature = "pre_bash_4_4"))]
        let decoded_prompt_cstr = bash_symbols::decode_prompt_string(c_prompt.as_ptr(), 1);
        #[cfg(feature = "pre_bash_4_4")]
        let decoded_prompt_cstr = bash_symbols::decode_prompt_string(c_prompt.as_ptr());
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

/// In test builds the bash FFI symbols are not linked; this function returns
/// the raw string unchanged (wrapped in a single [`Line`]) so that unit tests
/// can exercise the prompt-rendering logic without requiring a live bash
/// process.
#[cfg(test)]
fn expand_prompt_through_bash(raw: String) -> Option<Vec<Line<'static>>> {
    if raw.is_empty() {
        return Some(vec![]);
    }
    Some(vec![Line::raw(raw)])
}

/// Builds expanded prompt segment lines from raw bash prompt strings while
/// accumulating a shared map of time-placeholder identifiers to chrono format
/// strings and holding pre-processed animation data.
///
/// A single `PromptStringBuilder` should be used for all prompt variables
/// (PS1, RPS1 / RPROMPT, PS1_FILL) so that placeholder identifiers are unique
/// across all of them.
struct PromptStringBuilder<'a> {
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
    /// Raw prompt widget definitions.  Used by [`expand_span_to_segments`] to
    /// recognise widget-name placeholders and produce the appropriate
    /// `PromptSegment::Widget*` segments.  Each occurrence of a widget name in
    /// the prompt text produces a fully independent segment (and, for custom
    /// widgets, a separate process invocation).
    widgets: &'a [PromptWidget],
    /// Current working directory, used to detect CWD substrings in prompt spans.
    cwd: Option<String>,
    /// Home directory, used to recognise `~`-prefixed path representations.
    home: Option<String>,
}

impl<'a> PromptStringBuilder<'a> {
    fn new(animations: Vec<ProcessedAnimation>, widgets: &'a [PromptWidget]) -> Self {
        Self {
            counter: 0,
            time_map: HashMap::new(),
            animations,
            widgets,
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
                let mut segs: Vec<PromptSegment> = line
                    .spans
                    .into_iter()
                    .flat_map(|span| self.expand_span_to_segments(span))
                    .collect();
                // If multiple Cwd segments are present (because the same path
                // substring appeared in more than one prompt span), keep only
                // the longest one and convert the rest back to Static segments.
                dedup_cwd_segments(&mut segs);
                segs
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

        // Pass 3: split any remaining Static segments on widget names.
        let widget_segs: Vec<PromptSegment> = if self.widgets.is_empty() {
            animation_segs
        } else {
            animation_segs
                .into_iter()
                .flat_map(|seg| match seg {
                    PromptSegment::Static(s) => self.split_static_span_by_widgets(s),
                    other => vec![other],
                })
                .collect()
        };

        // Pass 4: split any remaining Static segments on CWD.
        if let Some(cwd) = self.cwd.as_deref() {
            widget_segs
                .into_iter()
                .flat_map(|seg| match seg {
                    PromptSegment::Static(s) => split_static_span_by_cwd(s, cwd),
                    other => vec![other],
                })
                .collect()
        } else {
            widget_segs
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

            let anim = self.animations[anim_idx].clone().patch_style(style);

            result.push(PromptSegment::Animation(Box::new(anim)));

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

    /// Split a static [`Span`] at widget-name boundaries, producing
    /// `Static` and `Widget*` segments.
    ///
    /// Runs a single greedy loop that always picks the earliest-occurring
    /// widget name in the remaining text.  Returns at least one segment;
    /// if no widget names are found the original span is returned unchanged
    /// as a `Static` segment.
    ///
    /// For each occurrence of a custom-widget name the widget command is
    /// either run synchronously (blocking) or launched in a background thread
    /// (non-blocking).  Multiple occurrences of the same name each produce an
    /// independent `WidgetCustom` segment with their own process.
    fn split_static_span_by_widgets(&self, span: Span<'static>) -> Vec<PromptSegment> {
        let style = span.style;
        let mut result: Vec<PromptSegment> = Vec::new();
        let mut remaining: String = span.content.into_owned();

        loop {
            // Find the widget whose name appears earliest in `remaining`.
            // Custom widgets with an empty command are excluded (they have no
            // valid name to match) so their placeholder text stays literal.
            let next = self
                .widgets
                .iter()
                .enumerate()
                .filter_map(|(i, widget)| {
                    let name = match widget {
                        PromptWidget::MouseMode(w) => w.name.as_str(),
                        PromptWidget::Custom(w) if !w.command.is_empty() => w.name.as_str(),
                        PromptWidget::Custom(_) => return None,
                    };
                    remaining.find(name).map(|pos| (pos, i, name.len()))
                })
                .min_by_key(|(pos, _, _)| *pos);

            let (pos, widget_idx, name_len) = match next {
                None => break,
                Some(m) => m,
            };

            if pos > 0 {
                result.push(PromptSegment::Static(Span::styled(
                    remaining[..pos].to_owned(),
                    style,
                )));
            }

            // TODO: patch the widget style here
            result.push(make_widget_segment(&self.widgets[widget_idx]));

            remaining = remaining[pos + name_len..].to_owned();
        }

        if !remaining.is_empty() {
            result.push(PromptSegment::Static(Span::styled(remaining, style)));
        }

        // Ensure at least one segment is always returned.
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

/// Build a [`PromptSegment`] for the given [`PromptWidget`].
///
/// For mouse-mode widgets the enabled/disabled texts are expanded through
/// bash and stored as pre-tagged [`TaggedSpan`]s.
///
/// For custom widgets the command is spawned directly as a child process.
/// A `block` timeout of 0 (the default when `block` is not specified) means
/// we do a single non-blocking `try_wait` and immediately put the child in
/// `Pending` if it hasn't finished yet.  Any positive value (including
/// `i32::MAX` ≈ 24.8 days, which is effectively indefinite) polls up to that
/// many milliseconds before moving on.
fn make_widget_segment(widget: &PromptWidget) -> PromptSegment {
    match widget {
        PromptWidget::MouseMode(w) => PromptSegment::WidgetMouseMode {
            enabled_text: stdout_to_tagged_spans(w.enabled_text.clone()),
            disabled_text: stdout_to_tagged_spans(w.disabled_text.clone()),
        },
        PromptWidget::Custom(w) => {
            let state = match spawn_widget_child(&w.command) {
                Err(failure) => WidgetCustomState::Failed(failure),
                Ok(mut child) => {
                    // Default to 0 ms when `block` is not specified; a 0 ms
                    // timeout performs a single non-blocking try_wait and moves
                    // on immediately.  i32::MAX polls for ~24.8 days, which is
                    // effectively indefinite.
                    let timeout_ms = w.block.unwrap_or(0);
                    let timeout = std::time::Duration::from_millis(timeout_ms as u64);
                    let start = std::time::Instant::now();
                    let done_status = loop {
                        match child.try_wait() {
                            Ok(Some(status)) => break Some(Ok(status)),
                            Ok(None) => {
                                if start.elapsed() >= timeout {
                                    break None;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(5));
                            }
                            Err(e) => break Some(Err(e)),
                        }
                    };
                    match done_status {
                        Some(Ok(status)) => {
                            collect_and_finalize(&mut child, status, &w.command, &w.prev_output)
                        }
                        Some(Err(e)) => {
                            log::error!("Custom prompt widget: try_wait error: {}", e);
                            WidgetCustomState::Failed(WidgetFailure {
                                exit_code: None,
                                stdout: String::new(),
                                stderr: e.to_string(),
                            })
                        }
                        None => {
                            // Timed out: keep the child running in the background.
                            let placeholder = resolve_placeholder(w);
                            WidgetCustomState::Pending {
                                placeholder,
                                child,
                                command: w.command.clone(),
                                prev_output_cell: w.prev_output.clone(),
                            }
                        }
                    }
                }
            };
            PromptSegment::WidgetCustom(state)
        }
    }
}

/// If a prompt line has more than one [`PromptSegment::Cwd`] segment
/// (because the same path substring appeared in several prompt spans),
/// keep only the longest one and convert the shorter duplicates back to
/// plain [`PromptSegment::Static`] segments.
fn dedup_cwd_segments(segs: &mut Vec<PromptSegment>) {
    // Collect indices and text lengths of every Cwd segment.
    let cwd_indices: Vec<(usize, usize)> = segs
        .iter()
        .enumerate()
        .filter_map(|(i, seg)| {
            if let PromptSegment::Cwd(spans) = seg {
                let len: usize = spans.iter().map(|s| s.content.len()).sum();
                Some((i, len))
            } else {
                None
            }
        })
        .collect();

    if cwd_indices.len() <= 1 {
        return;
    }

    // Index of the Cwd segment with the most text content.
    let longest_idx = cwd_indices
        .iter()
        .max_by_key(|(_, len)| *len)
        .map(|(i, _)| *i)
        .unwrap();

    // Convert all shorter Cwd segments back to Static.
    for (i, _) in &cwd_indices {
        if *i != longest_idx {
            if let PromptSegment::Cwd(spans) = &segs[*i] {
                let content: String = spans.iter().map(|s| s.content.as_ref()).collect();
                let style = spans.first().map(|s| s.style).unwrap_or_default();
                segs[*i] = PromptSegment::Static(Span::styled(content, style));
            }
        }
    }
}

/// Resolve the placeholder string for a custom widget that is not yet done.
fn resolve_placeholder(w: &PromptWidgetCustom) -> Vec<TaggedSpan<'static>> {
    match &w.placeholder {
        Placeholder::Spaces(n) => vec![TaggedSpan::new(Span::raw(" ".repeat(*n)), Tag::Ps1Prompt)],
        Placeholder::Prev => w.prev_output.lock().unwrap().clone(),
    }
}

/// Find the longest substring of `text` that looks like the current working
/// directory or any contiguous sub-path of it.
fn find_cwd_in_span(text: &str, cwd: &str) -> Option<Range<usize>> {
    // Early exit: a CWD path must contain '/' or start with '~'.
    // Spans with neither cannot match any CWD sub-path.
    if !text.contains('/') && !text.contains('~') {
        return None;
    }

    let cwd_trimmed = cwd.trim_end_matches('/');

    // Collect non-empty path segments.
    let segments: Vec<&str> = cwd_trimmed.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return None;
    }

    let mut best: Option<Range<usize>> = None;

    // Update `best` if `candidate` is found in `text` and is longer than the
    // current best match.
    let try_candidate = |candidate: &str, best: &mut Option<Range<usize>>| {
        if candidate.is_empty() {
            return;
        }
        if let Some(pos) = text.find(candidate) {
            let end = pos + candidate.len();
            let possible_best = pos..end;
            if best
                .clone()
                .map_or(true, |range| possible_best.len() > range.len())
            {
                *best = Some(possible_best);
            }
        }
    };
    try_candidate("~", &mut best);
    try_candidate("~/", &mut best);

    for (idx, _seg) in segments.iter().rev().enumerate() {
        let cwd_suffix = segments[segments.len() - 1 - idx..].join("/");
        try_candidate(&cwd_suffix, &mut best);

        let cwd_suffix_with_slash = format!("/{}", cwd_suffix);
        try_candidate(&cwd_suffix_with_slash, &mut best);

        let cwd_suffix_with_trailing_slash = format!("{}/", cwd_suffix);
        try_candidate(&cwd_suffix_with_trailing_slash, &mut best);

        let cwd_suffix_with_both_slashes = format!("/{}/", cwd_suffix);
        try_candidate(&cwd_suffix_with_both_slashes, &mut best);

        let cwd_suffix_with_tilde = format!("~{}", cwd_suffix);
        try_candidate(&cwd_suffix_with_tilde, &mut best);

        let cwd_suffix_with_tilde_and_slash = format!("~/{}", cwd_suffix);
        try_candidate(&cwd_suffix_with_tilde_and_slash, &mut best);
    }

    best
}

/// Split a static [`Span`] at the first (longest) detected CWD substring,
/// producing up to three segments: an optional `Static` prefix, a `Cwd`
/// segment for the matched path text, and an optional `Static` suffix.
///
/// If no CWD-like substring is found the original span is returned unchanged
/// as a single `Static` segment.
fn split_static_span_by_cwd(span: Span<'static>, cwd: &str) -> Vec<PromptSegment> {
    let text = span.content.as_ref().to_owned();
    let style = span.style;

    let Some(cwd_range) = find_cwd_in_span(&text, cwd) else {
        return vec![PromptSegment::Static(span)];
    };

    let mut result = Vec::new();
    if cwd_range.start > 0 {
        result.push(PromptSegment::Static(Span::styled(
            text[..cwd_range.start].to_owned(),
            style,
        )));
    }
    result.push(PromptSegment::Cwd(split_cwd_text_into_spans(
        &text[cwd_range.clone()],
        style,
    )));
    if cwd_range.end < text.len() {
        result.push(PromptSegment::Static(Span::styled(
            text[cwd_range.end..].to_owned(),
            style,
        )));
    }
    result
}

/// Split a CWD text string into individual [`Span`]s, with each `/` separator
/// as its own span distinct from the path segments on either side.
///
/// The leftmost span is the tilde or root `/`; each subsequent alternating pair
/// is a `"/"` separator followed by a directory name.
///
/// Examples:
/// - `"~/foo/bar"` → `["~", "/", "foo", "/", "bar"]`
/// - `"/home/foo/bar"` → `["/", "home", "/", "foo", "/", "bar"]`
/// - `"~"` → `["~"]`
/// - `"qwe/try/ooh"` → `["qwe", "/", "try", "/", "ooh"]`
fn split_cwd_text_into_spans(text: &str, style: ratatui::style::Style) -> Vec<Span<'static>> {
    if text.is_empty() {
        return vec![];
    }

    let mut result: Vec<Span<'static>> = Vec::new();

    if text.starts_with('~') {
        // The tilde segment never has a leading slash.
        // Find the first slash after '~', if any.
        if let Some(rel_slash) = text[1..].find('/') {
            let slash_pos = 1 + rel_slash;
            result.push(Span::styled("~".to_owned(), style));
            split_into_spans(&text[slash_pos..], style, &mut result);
        } else {
            // Bare "~" or "~something" with no slash — emit as a single segment.
            result.push(Span::styled(text.to_owned(), style));
        }
    } else {
        split_into_spans(text, style, &mut result);
    }

    result
}

/// Append spans from `path` to `result`, emitting each `/` separator and each
/// directory name as a separate span.  Handles absolute paths (starting with
/// `/`) as well as relative paths.
fn split_into_spans(path: &str, style: ratatui::style::Style, result: &mut Vec<Span<'static>>) {
    let mut remaining = path;
    while !remaining.is_empty() {
        if remaining.starts_with('/') {
            result.push(Span::styled("/".to_owned(), style));
            remaining = &remaining[1..];
        } else if let Some(slash_pos) = remaining.find('/') {
            result.push(Span::styled(remaining[..slash_pos].to_owned(), style));
            remaining = &remaining[slash_pos..];
        } else {
            result.push(Span::styled(remaining.to_owned(), style));
            break;
        }
    }
}

/// Expand `stdout` through bash prompt processing and return pre-tagged spans.
///
/// This is the common conversion used whenever a custom widget command
/// completes successfully: the raw stdout string is passed through
/// [`expand_prompt_through_bash`] and each resulting span is wrapped in a
/// [`TaggedSpan`] with [`Tag::Ps1Prompt`].
fn stdout_to_tagged_spans(stdout: String) -> Vec<TaggedSpan<'static>> {
    expand_prompt_through_bash(stdout)
        .unwrap_or_default()
        .into_iter()
        .flat_map(|line| {
            line.spans
                .into_iter()
                .map(|span| TaggedSpan::new(span, Tag::Ps1Prompt))
        })
        .collect()
}

/// Convert a slice of [`PromptSegment`]s to a [`TaggedLine`] by resolving each
/// segment against `now` and attaching an appropriate [`Tag`] to each span.
///
/// `mouse_enabled` is used by [`PromptSegment::WidgetMouseMode`] to choose
/// between the enabled and disabled text.
///
/// Segments are passed as a mutable slice so that `WidgetCustom` segments can
/// advance their internal state (polling the child process) without
/// requiring a `RefCell`.
fn format_prompt_line(
    segments: &mut [PromptSegment],
    now: &chrono::DateTime<chrono::Local>,
    mouse_enabled: bool,
) -> TaggedLine<'static> {
    // First pass: advance any Pending WidgetCustom segments whose child process
    // has exited.  We need a mutable pass before the immutable render pass.
    for segment in segments.iter_mut() {
        if let PromptSegment::WidgetCustom(state) = segment {
            let new_state: Option<WidgetCustomState> = match state {
                WidgetCustomState::Pending {
                    child,
                    command,
                    prev_output_cell,
                    ..
                } => match child.try_wait() {
                    Ok(Some(status)) => Some(collect_and_finalize(
                        child,
                        status,
                        command,
                        prev_output_cell,
                    )),
                    Ok(None) => None,
                    Err(e) => {
                        log::error!("Custom prompt widget: try_wait error: {}", e);
                        Some(WidgetCustomState::Failed(WidgetFailure {
                            exit_code: None,
                            stdout: String::new(),
                            stderr: e.to_string(),
                        }))
                    }
                },
                _ => None,
            };
            if let Some(s) = new_state {
                *state = s;
            }
        }
    }

    let tagged_spans: Vec<TaggedSpan<'static>> = segments
        .iter()
        .flat_map(|segment| -> Vec<TaggedSpan<'static>> {
            match segment {
                PromptSegment::Static(span) => {
                    vec![TaggedSpan::new(span.clone(), Tag::Ps1Prompt)]
                }
                PromptSegment::Cwd(spans) => {
                    // Only selectable spans get a Ps1PromptCwd(n) tag.
                    // A span is selectable when it is not a "/" separator, or
                    // when it is the very first span (the leading "/" of an
                    // absolute path).  Internal "/" separators get Ps1Prompt so
                    // they are rendered normally and never highlighted.
                    let selectable_count = spans
                        .iter()
                        .enumerate()
                        .filter(|(i, s)| s.content.as_ref() != "/" || *i == 0)
                        .count();
                    let mut sel_idx = 0usize;
                    let mut tagged: Vec<TaggedSpan<'static>> = Vec::with_capacity(spans.len());
                    for (i, span) in spans.iter().enumerate() {
                        let is_selectable = span.content.as_ref() != "/" || i == 0;
                        let tag = if is_selectable {
                            let t = Tag::Ps1PromptCwd(selectable_count - 1 - sel_idx);
                            sel_idx += 1;
                            t
                        } else {
                            Tag::Ps1Prompt
                        };
                        tagged.push(TaggedSpan::new(span.clone(), tag));
                    }
                    tagged
                }
                PromptSegment::DynamicTime { strftime, style } => {
                    vec![TaggedSpan::new(
                        Span::styled(now.format(strftime).to_string(), *style),
                        Tag::Ps1PromptDynamicTime,
                    )]
                }
                PromptSegment::Animation(anim) => get_frame_spans(anim, now)
                    .iter()
                    .map(|span| TaggedSpan::new(span.clone(), Tag::Ps1PromptAnimation))
                    .collect(),
                PromptSegment::WidgetMouseMode {
                    enabled_text,
                    disabled_text,
                } => {
                    let tagged: &Vec<TaggedSpan<'static>> = if mouse_enabled {
                        enabled_text
                    } else {
                        disabled_text
                    };
                    tagged.clone()
                }
                PromptSegment::WidgetCustom(state) => match state {
                    WidgetCustomState::Pending { placeholder, .. } => placeholder.clone(),
                    WidgetCustomState::Done(tagged_spans) => tagged_spans.clone(),
                    WidgetCustomState::Failed(failure) => failure.to_error_spans(),
                },
            }
        })
        .collect();
    TaggedLine::from(tagged_spans)
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

/// Spawn a widget command as a child process with captured stdout and stderr.
///
/// Returns the [`std::process::Child`] on success, or a [`WidgetFailure`] if
/// the command list is empty or the process cannot be spawned.
///
/// `SIGCHLD` is expected to have been set to `SIG_DFL` by the caller before
/// `app::get_command` was invoked; this function does not touch signal
/// dispositions.
fn spawn_widget_child(command: &[String]) -> Result<std::process::Child, WidgetFailure> {
    use std::process::Stdio;
    let (prog, args) = match command.split_first() {
        Some(parts) => parts,
        None => {
            let msg = "spawn_widget_child: empty command".to_string();
            log::warn!("{}", msg);
            return Err(WidgetFailure {
                exit_code: None,
                stdout: String::new(),
                stderr: msg,
            });
        }
    };
    std::process::Command::new(prog)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            log::error!(
                "Custom prompt widget: failed to spawn command {:?}: {}",
                command,
                e
            );
            WidgetFailure {
                exit_code: None,
                stdout: String::new(),
                stderr: e.to_string(),
            }
        })
}

/// Read remaining output from a child that has already exited (after
/// [`try_wait`] returned `Some(status)`) and build the final [`WidgetCustomState`].
///
/// The exit `status` must come from the `try_wait` call so that we do not call
/// `wait` a second time (which would fail because the zombie has already been
/// reaped by `try_wait` on Unix).
fn collect_and_finalize(
    child: &mut std::process::Child,
    status: std::process::ExitStatus,
    command: &[String],
    prev_output: &Arc<Mutex<Vec<TaggedSpan<'static>>>>,
) -> WidgetCustomState {
    use std::io::Read;
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        if let Err(e) = out.read_to_end(&mut stdout_buf) {
            log::warn!(
                "Custom prompt widget {:?}: error reading stdout: {}",
                command,
                e
            );
        }
    }
    if let Some(mut err) = child.stderr.take() {
        if let Err(e) = err.read_to_end(&mut stderr_buf) {
            log::warn!(
                "Custom prompt widget {:?}: error reading stderr: {}",
                command,
                e
            );
        }
    }
    let stdout = String::from_utf8_lossy(&stdout_buf).trim().to_string();
    let stderr = String::from_utf8_lossy(&stderr_buf).trim().to_string();
    log::info!("Custom prompt widget {:?} exited with {}", command, status);
    log::debug!("Custom prompt widget stdout: {}", stdout);
    log::debug!("Custom prompt widget stderr: {}", stderr);
    if status.success() {
        let process_output = stdout_to_tagged_spans(stdout);
        *prev_output.lock().unwrap() = process_output.clone();
        WidgetCustomState::Done(process_output)
    } else {
        log::warn!(
            "Custom prompt widget {:?} failed with {}; stderr: {}",
            command,
            status,
            stderr
        );
        WidgetCustomState::Failed(WidgetFailure {
            exit_code: status.code(),
            stdout,
            stderr,
        })
    }
}

impl PromptManager {
    pub fn new(
        unfinished_from_prev_command: bool,
        animations: &[PromptAnimation],
        widgets: &[PromptWidget],
    ) -> Self {
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
                cwd: String::new(),
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
            // placeholder IDs are unique.  Animations and widgets are passed in
            // so that expand_span_to_segments can produce the right segments.
            // Widget segments (including process spawning) are created lazily
            // inside split_static_span_by_widgets as each widget name is found.
            log::debug!("Widget count: {}", widgets.len());
            let cwd = bash_funcs::get_cwd();
            let home = bash_funcs::get_envvar_value("HOME");
            log::debug!("CWD for prompt detection: {:?}, HOME: {:?}", cwd, home);
            let mut builder =
                PromptStringBuilder::new(processed_animations, widgets).with_cwd(cwd.clone(), home);

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
                cwd,
            }
        }
    }

    pub fn get_ps1_lines(
        &mut self,
        show_animations: bool,
        mouse_enabled: bool,
    ) -> (
        Vec<TaggedLine<'static>>,
        Vec<TaggedLine<'static>>,
        TaggedLine<'static>,
    ) {
        use chrono::Local;
        let now = if show_animations {
            Local::now()
        } else {
            self.construction_time
        };

        let formatted_prompt: Vec<TaggedLine<'static>> = self
            .prompt
            .iter_mut()
            .map(|line| format_prompt_line(line, &now, mouse_enabled))
            .collect();

        let formatted_rprompt: Vec<TaggedLine<'static>> = self
            .rprompt
            .iter_mut()
            .map(|line| format_prompt_line(line, &now, mouse_enabled))
            .collect();

        let formatted_fill = format_prompt_line(&mut self.fill_span, &now, mouse_enabled);

        (formatted_prompt, formatted_rprompt, formatted_fill)
    }

    /// Return the number of CWD display segments in the left prompt.
    ///
    /// This is the count of *selectable* path spans tagged with
    /// [`Tag::Ps1PromptCwd`]: every non-`"/"` span, plus the leading `"/"` of
    /// an absolute path (index 0).  Internal `"/"` separator spans are not
    /// counted.  Returns 0 when no CWD segments are present.
    pub fn cwd_display_segment_count(&self) -> usize {
        self.prompt
            .iter()
            .flat_map(|line| line.iter())
            .find_map(|seg| {
                if let PromptSegment::Cwd(spans) = seg {
                    let count = spans
                        .iter()
                        .enumerate()
                        .filter(|(i, s)| s.content.as_ref() != "/" || *i == 0)
                        .count();
                    Some(count)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    /// Return the filesystem path corresponding to the CWD segment at `index`.
    ///
    /// Index 0 is the rightmost (current) directory; each increment steps one
    /// level toward the root.  Returns `None` when no CWD is available.
    pub fn cwd_path_for_index(&self, index: usize) -> Option<String> {
        if self.cwd.is_empty() {
            return None;
        }
        let components: Vec<_> = std::path::Path::new(&self.cwd).components().collect();
        if components.is_empty() {
            return None;
        }
        let keep = components.len().saturating_sub(index);
        if keep == 0 {
            // Clamp to root.
            let root: std::path::PathBuf = components[..1].iter().collect();
            return Some(root.to_string_lossy().into_owned());
        }
        let result: std::path::PathBuf = components[..keep].iter().collect();
        Some(result.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{PromptWidgetCustom, PromptWidgetMouseMode};
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
        let builder = PromptStringBuilder::new(vec![anim], &[]);
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
        let builder = PromptStringBuilder::new(vec![anim], &[]);
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
        let builder = PromptStringBuilder::new(vec![anim], &[]);
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
        let mut segments = vec![
            PromptSegment::Static(Span::raw("before ")),
            PromptSegment::Animation(Box::new(anim)),
            PromptSegment::Static(Span::raw(" after")),
        ];
        let now = fixed_time(0);
        let line = format_prompt_line(&mut segments, &now, false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "before f0 after");
    }

    #[test]
    fn test_format_prompt_line_animation_frame_advances() {
        // At t=100 ms, fps=10 → frame 1 ("f1").
        let anim = make_processed_anim("SPIN", 10.0, &["f0", "f1"]);
        let mut segments = vec![PromptSegment::Animation(Box::new(anim))];
        let line = format_prompt_line(&mut segments, &fixed_time(100), false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "f1");
    }

    #[test]
    fn test_format_prompt_line_animation_tag() {
        let anim = make_processed_anim("SPIN", 10.0, &["f0"]);
        let mut segments = vec![PromptSegment::Animation(Box::new(anim))];
        let line = format_prompt_line(&mut segments, &fixed_time(0), false);
        assert!(
            line.spans.iter().all(
                |s| s.tag == crate::content_builder::SpanTag::Constant(Tag::Ps1PromptAnimation)
            )
        );
    }

    // --- format_prompt_line (DynamicTime rendering) --------------------------

    #[test]
    fn test_format_prompt_line_dynamic_time() {
        // Use a fixed time to produce a predictable formatted string.  The actual
        // HH:MM:SS value is timezone-dependent, so we compute the expected value
        // with the same `now` and format string rather than hard-coding a literal.
        let now = fixed_time(0);
        let formatted_time = now.format("%H:%M:%S").to_string();

        let mut segments = vec![
            PromptSegment::Static(Span::raw("[")),
            PromptSegment::DynamicTime {
                strftime: "%H:%M:%S".to_string(),
                style: ratatui::style::Style::default(),
            },
            PromptSegment::Static(Span::raw("]")),
        ];
        let line = format_prompt_line(&mut segments, &now, false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, format!("[{}]", formatted_time));
    }

    // --- expand_span_to_segments (time-code splitting) -----------------------

    #[test]
    fn test_expand_span_to_segments_no_placeholders() {
        let builder = PromptStringBuilder::new(vec![], &[]);
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
        let mut builder = PromptStringBuilder::new(vec![], &[]);
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
        let mut builder = PromptStringBuilder::new(vec![], &[]);
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

    #[test]
    fn test_find_cwd_tilde_only() {
        // "~" alone should be detected as the home dir representation
        let cwd = "/home";
        let text = "otherpartofheprompt~:$";
        let result = find_cwd_in_span(text, cwd);
        let cwd_range = result.expect("should find a match");
        assert_eq!(&text[cwd_range], "~");
    }

    #[test]
    fn test_find_cwd_tilde_slash() {
        // "~/" should be preferred over bare "~"
        let text = "otherpartofheprompt~/:$";
        let cwd = "/home";
        let result = find_cwd_in_span(text, cwd);
        let cwd_range = result.expect("should find a match");
        assert_eq!(&text[cwd_range], "~/");
    }

    #[test]
    fn test_find_cwd_tilde_one_segment() {
        let text = "otherpartofheprompt~/qwe:$";
        let cwd = "/home/qwe";
        let result = find_cwd_in_span(text, cwd);
        let cwd_range = result.expect("should find a match");
        assert_eq!(&text[cwd_range], "~/qwe");
    }

    #[test]
    fn test_find_cwd_tilde_two_segments() {
        let text = "otherpartofheprompt~/qwe/try:$";
        let cwd = "/home/qwe/try";
        let result = find_cwd_in_span(text, cwd);
        let cwd_range = result.expect("should find a match");
        assert_eq!(&text[cwd_range], "~/qwe/try");
    }

    #[test]
    fn test_find_cwd_partial_path_no_tilde() {
        // "qwe/try/ooh" is a contiguous sub-path; no tilde in the text
        let text = "otherpartofhepromptqwe/try/ooh:$";
        let cwd = "/home/qwe/try/ooh";
        let result = find_cwd_in_span(text, cwd);
        let cwd_range = result.expect("should find a match");
        assert_eq!(&text[cwd_range], "qwe/try/ooh");
    }

    #[test]
    fn test_find_cwd_partial_path_trailing_slash() {
        // trailing slash is part of the detected match
        let text = "otherpartofheprompt try/ooh/lkj/:$";
        let cwd = "/home/qwe/try/ooh/lkj";
        let result = find_cwd_in_span(text, cwd);
        let cwd_range = result.expect("should find a match");
        assert_eq!(&text[cwd_range], "try/ooh/lkj/");
    }

    #[test]
    fn test_find_cwd_full() {
        // trailing slash is part of the detected match
        let text = "otherpartofheprompt /home/qwe/try/ooh/lkj/:$";
        let cwd = "/home/qwe/try/ooh/lkj";
        let result = find_cwd_in_span(text, cwd);
        let cwd_range = result.expect("should find a match");
        assert_eq!(&text[cwd_range], "/home/qwe/try/ooh/lkj/");
    }
    // --- expand_span_to_segments with CWD detection --------------------------

    #[test]
    fn test_expand_span_cwd_tilde_only() {
        let builder = PromptStringBuilder::new(vec![], &[]).with_cwd(
            "/home/foo/qwe/try/ooh/lkj".to_string(),
            Some("/home/foo".to_string()),
        );
        let span = Span::raw("otherpartofheprompt~:$");
        let segs = builder.expand_span_to_segments(span);
        let cwd_seg = segs.iter().find(|s| matches!(s, PromptSegment::Cwd(_)));
        match cwd_seg.expect("expected a Cwd segment") {
            PromptSegment::Cwd(spans) => {
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].content, "~");
            }
            _ => unreachable!(),
        }
    }

    // #[test]
    // fn test_expand_span_cwd_partial_no_tilde() {
    //     let builder = PromptStringBuilder::new(vec![], &[]).with_cwd(
    //         "/home/foo/qwe/try/ooh/lkj".to_string(),
    //         Some("/home/foo".to_string()),
    //     );
    //     let span = Span::raw("otherpartofhepromptqwe/try/ooh:$");
    //     let segs = builder.expand_span_to_segments(span);
    //     let cwd_seg = segs.iter().find(|s| matches!(s, PromptSegment::Cwd(_)));
    //     match cwd_seg.expect("expected a Cwd segment") {
    //         PromptSegment::Cwd(spans) => {
    //             let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    //             assert_eq!(text, "qwe/try/ooh");
    //         }
    //         _ => unreachable!(),
    //     }
    // }

    // --- split_cwd_text_into_spans -------------------------------------------

    fn span_contents<'a>(spans: &'a [Span<'static>]) -> Vec<&'a str> {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn test_split_cwd_tilde_only() {
        let style = ratatui::style::Style::default();
        let spans = split_cwd_text_into_spans("~", style);
        assert_eq!(span_contents(&spans), vec!["~"]);
    }

    #[test]
    fn test_split_cwd_tilde_with_segments() {
        let style = ratatui::style::Style::default();
        let spans = split_cwd_text_into_spans("~/foo/bar/baz", style);
        assert_eq!(
            span_contents(&spans),
            vec!["~", "/", "foo", "/", "bar", "/", "baz"]
        );
    }

    #[test]
    fn test_split_cwd_absolute_path() {
        let style = ratatui::style::Style::default();
        let spans = split_cwd_text_into_spans("/home/foo/bar", style);
        assert_eq!(
            span_contents(&spans),
            vec!["/", "home", "/", "foo", "/", "bar"]
        );
    }

    #[test]
    fn test_split_cwd_relative_path() {
        let style = ratatui::style::Style::default();
        let spans = split_cwd_text_into_spans("qwe/try/ooh", style);
        assert_eq!(span_contents(&spans), vec!["qwe", "/", "try", "/", "ooh"]);
    }

    #[test]
    fn test_split_cwd_single_segment() {
        let style = ratatui::style::Style::default();
        let spans = split_cwd_text_into_spans("/home", style);
        assert_eq!(span_contents(&spans), vec!["/", "home"]);
    }

    #[test]
    fn test_split_cwd_trailing_slash() {
        let style = ratatui::style::Style::default();
        let spans = split_cwd_text_into_spans("~/foo/", style);
        // trailing slash becomes its own "/" span
        assert_eq!(span_contents(&spans), vec!["~", "/", "foo", "/"]);
    }

    // --- format_prompt_line (CWD tagging) ------------------------------------

    #[test]
    fn test_format_prompt_line_cwd_tags() {
        // "~/foo/bar" → 5 spans ["~", "/", "foo", "/", "bar"];
        // selectable spans are "~", "foo", "bar" (3 total).
        // "~" is leftmost selectable → index 2, "foo" → index 1, "bar" → index 0.
        // "/" separators get Ps1Prompt (not selectable).
        let spans = split_cwd_text_into_spans("~/foo/bar", ratatui::style::Style::default());
        let mut segments = vec![PromptSegment::Cwd(spans)];
        let line = format_prompt_line(&mut segments, &fixed_time(0), false);
        assert_eq!(line.spans.len(), 5);
        // "~" → index 2
        assert_eq!(
            line.spans[0].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1PromptCwd(2))
        );
        // "/" separator → Ps1Prompt
        assert_eq!(
            line.spans[1].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1Prompt)
        );
        // "foo" → index 1
        assert_eq!(
            line.spans[2].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1PromptCwd(1))
        );
        // "/" separator → Ps1Prompt
        assert_eq!(
            line.spans[3].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1Prompt)
        );
        // "bar" is rightmost → index 0
        assert_eq!(
            line.spans[4].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1PromptCwd(0))
        );
    }

    #[test]
    fn test_format_prompt_line_cwd_single_segment_tag() {
        let spans = split_cwd_text_into_spans("~", ratatui::style::Style::default());
        let mut segments = vec![PromptSegment::Cwd(spans)];
        let line = format_prompt_line(&mut segments, &fixed_time(0), false);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(
            line.spans[0].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1PromptCwd(0))
        );
    }

    #[test]
    fn test_format_prompt_line_dynamic_time_tag() {
        let mut segments = vec![PromptSegment::DynamicTime {
            strftime: "%H:%M".to_string(),
            style: ratatui::style::Style::default(),
        }];
        let line = format_prompt_line(&mut segments, &fixed_time(0), false);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(
            line.spans[0].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1PromptDynamicTime)
        );
    }

    #[test]
    fn test_format_prompt_line_static_tag() {
        let mut segments = vec![PromptSegment::Static(Span::raw("$ "))];
        let line = format_prompt_line(&mut segments, &fixed_time(0), false);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(
            line.spans[0].tag,
            crate::content_builder::SpanTag::Constant(Tag::Ps1Prompt)
        );
    }

    // --- cwd_path_for_index / cwd_display_segment_count -------------------

    /// Build a minimal `PromptManager` for testing the CWD helper methods.
    fn make_pm_with_cwd(cwd: &str, cwd_text: &str) -> PromptManager {
        let spans = split_cwd_text_into_spans(cwd_text, ratatui::style::Style::default());
        PromptManager {
            prompt: vec![vec![PromptSegment::Cwd(spans)]],
            rprompt: vec![],
            fill_span: vec![],
            construction_time: chrono::Local::now(),
            cwd: cwd.to_string(),
        }
    }

    #[test]
    fn test_cwd_display_segment_count_no_cwd() {
        let pm = PromptManager {
            prompt: vec![vec![PromptSegment::Static(Span::raw("$ "))]],
            rprompt: vec![],
            fill_span: vec![],
            construction_time: chrono::Local::now(),
            cwd: String::new(),
        };
        assert_eq!(pm.cwd_display_segment_count(), 0);
    }

    #[test]
    fn test_cwd_display_segment_count_three_segments() {
        let pm = make_pm_with_cwd("/home/foo/bar", "~/bar");
        // "~/bar" splits into ["~", "/bar"] → 2 display segments
        assert_eq!(pm.cwd_display_segment_count(), 2);
    }

    #[test]
    fn test_cwd_display_segment_count_absolute_three_segments() {
        let pm = make_pm_with_cwd("/home/foo/bar", "/home/foo/bar");
        // "/home/foo/bar" → ["/", "home", "/", "foo", "/", "bar"]
        // selectable: "/", "home", "foo", "bar" → 4 display segments
        assert_eq!(pm.cwd_display_segment_count(), 4);
    }

    #[test]
    fn test_cwd_path_for_index_no_cwd() {
        let pm = make_pm_with_cwd("", "");
        assert_eq!(pm.cwd_path_for_index(0), None);
    }

    #[test]
    fn test_cwd_path_for_index_zero() {
        let pm = make_pm_with_cwd("/home/foo/bar", "~/bar");
        // index 0 → current dir
        assert_eq!(pm.cwd_path_for_index(0), Some("/home/foo/bar".to_string()));
    }

    #[test]
    fn test_cwd_path_for_index_one() {
        let pm = make_pm_with_cwd("/home/foo/bar", "~/bar");
        // index 1 → parent dir
        assert_eq!(pm.cwd_path_for_index(1), Some("/home/foo".to_string()));
    }

    #[test]
    fn test_cwd_path_for_index_root() {
        let pm = make_pm_with_cwd("/home/foo/bar", "/home/foo/bar");
        // index 3 → root "/"
        assert_eq!(pm.cwd_path_for_index(3), Some("/".to_string()));
    }

    #[test]
    fn test_cwd_path_for_index_clamped_to_root() {
        let pm = make_pm_with_cwd("/home/foo/bar", "/home/foo/bar");
        // index beyond path depth → root
        assert_eq!(pm.cwd_path_for_index(100), Some("/".to_string()));
    }

    // --- WidgetMouseMode rendering -------------------------------------------

    #[test]
    fn test_format_prompt_line_widget_mouse_mode_enabled() {
        let mut segments = vec![PromptSegment::WidgetMouseMode {
            enabled_text: vec![TaggedSpan::new(Span::raw("mouse on"), Tag::Ps1Prompt)],
            disabled_text: vec![TaggedSpan::new(Span::raw("mouse off"), Tag::Ps1Prompt)],
        }];
        let line = format_prompt_line(&mut segments, &fixed_time(0), true);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "mouse on");
    }

    #[test]
    fn test_format_prompt_line_widget_mouse_mode_disabled() {
        let mut segments = vec![PromptSegment::WidgetMouseMode {
            enabled_text: vec![TaggedSpan::new(Span::raw("mouse on"), Tag::Ps1Prompt)],
            disabled_text: vec![TaggedSpan::new(Span::raw("mouse off"), Tag::Ps1Prompt)],
        }];
        let line = format_prompt_line(&mut segments, &fixed_time(0), false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "mouse off");
    }

    #[test]
    fn test_expand_span_widget_mouse_mode_name_only() {
        let widget = PromptWidget::MouseMode(PromptWidgetMouseMode {
            name: "MOUSE_WIDGET".to_string(),
            enabled_text: "on".to_string(),
            disabled_text: "off".to_string(),
        });
        let widgets = [widget];
        let builder = PromptStringBuilder::new(vec![], &widgets);
        let span = Span::raw("MOUSE_WIDGET");
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 1);
        match &segs[0] {
            PromptSegment::WidgetMouseMode {
                enabled_text,
                disabled_text,
            } => {
                // In test builds expand_prompt_through_bash returns the text unchanged.
                assert_eq!(enabled_text[0].span.content, "on");
                assert_eq!(disabled_text[0].span.content, "off");
            }
            _ => panic!("expected WidgetMouseMode"),
        }
    }

    #[test]
    fn test_expand_span_widget_mouse_mode_surrounded_by_text() {
        let widget = PromptWidget::MouseMode(PromptWidgetMouseMode {
            name: "MMODE".to_string(),
            enabled_text: "on".to_string(),
            disabled_text: "off".to_string(),
        });
        let widgets = [widget];
        let builder = PromptStringBuilder::new(vec![], &widgets);
        let span = Span::raw("before MMODE after");
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 3);
        match &segs[0] {
            PromptSegment::Static(s) => assert_eq!(s.content, "before "),
            _ => panic!("expected Static at index 0"),
        }
        match &segs[1] {
            PromptSegment::WidgetMouseMode { .. } => {}
            _ => panic!("expected WidgetMouseMode at index 1"),
        }
        match &segs[2] {
            PromptSegment::Static(s) => assert_eq!(s.content, " after"),
            _ => panic!("expected Static at index 2"),
        }
    }

    // --- WidgetCustom rendering ---------------------------------------------

    #[test]
    fn test_format_prompt_line_widget_custom_pending() {
        // A pending custom widget should render its placeholder.
        // Spawn a long-running process so that try_wait returns None (still running).
        let child = std::process::Command::new("sleep")
            .arg("100")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to spawn sleep for test");
        let mut segs = vec![PromptSegment::WidgetCustom(WidgetCustomState::Pending {
            placeholder: vec![TaggedSpan::new(Span::raw("   "), Tag::Ps1Prompt)],
            child,
            command: vec!["sleep".to_string(), "100".to_string()],
            prev_output_cell: Arc::new(Mutex::new(Vec::new())),
        })];
        let line = format_prompt_line(&mut segs, &fixed_time(0), false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "   ");
        // Drop segs here; the Drop impl on WidgetCustomState will kill sleep.
    }

    #[test]
    fn test_format_prompt_line_widget_custom_done() {
        // A done custom widget renders the pre-tagged result spans.
        let result_spans = vec![TaggedSpan::new(Span::raw("output"), Tag::Ps1Prompt)];
        let mut segs = vec![PromptSegment::WidgetCustom(WidgetCustomState::Done(
            result_spans,
        ))];
        let line = format_prompt_line(&mut segs, &fixed_time(0), false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "output");
    }

    #[test]
    fn test_format_prompt_line_widget_custom_failed() {
        // A failed custom widget renders the "command failed (exit N)" error text.
        let mut segs = vec![PromptSegment::WidgetCustom(WidgetCustomState::Failed(
            WidgetFailure {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: String::new(),
            },
        ))];
        let line = format_prompt_line(&mut segs, &fixed_time(0), false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "command failed (exit 1)");
    }

    #[test]
    fn test_format_prompt_line_widget_custom_failed_no_exit_code() {
        // When there's no exit code (spawn failure), renders "command failed".
        let mut segs = vec![PromptSegment::WidgetCustom(WidgetCustomState::Failed(
            WidgetFailure {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            },
        ))];
        let line = format_prompt_line(&mut segs, &fixed_time(0), false);
        let content: String = line.spans.iter().map(|s| s.span.content.as_ref()).collect();
        assert_eq!(content, "command failed");
    }

    #[test]
    fn test_expand_span_widget_custom_name() {
        // When a custom widget name appears in a span it should produce a
        // WidgetCustom segment. The spawn will fail immediately (ENOENT) since
        // the command doesn't exist, producing a Failed state, which still
        // satisfies the WidgetCustom(_) pattern match below.
        let widget = PromptWidget::Custom(PromptWidgetCustom {
            name: "MY_WIDGET".to_string(),
            command: vec!["nonexistent_test_command".to_string()],
            block: None,
            placeholder: crate::settings::Placeholder::Spaces(0),
            prev_output: Arc::new(Mutex::new(Vec::new())),
        });
        let widgets = [widget];
        let builder = PromptStringBuilder::new(vec![], &widgets);
        let span = Span::raw("before MY_WIDGET after");
        let segs = builder.expand_span_to_segments(span);
        assert_eq!(segs.len(), 3);
        match &segs[0] {
            PromptSegment::Static(s) => assert_eq!(s.content, "before "),
            _ => panic!("expected Static at 0"),
        }
        match &segs[1] {
            PromptSegment::WidgetCustom(_) => {}
            _ => panic!("expected WidgetCustom at 1"),
        }
        match &segs[2] {
            PromptSegment::Static(s) => assert_eq!(s.content, " after"),
            _ => panic!("expected Static at 2"),
        }
    }
}
