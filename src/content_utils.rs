use crate::active_suggestions::ANIMATION_FRAME_FPS;
use crate::unicode_helpers::{OctantStyle, octant_from_grid};
use crate::{cursor::CursorEasing, palette::Palette};
use ansi_to_tui::IntoText;
use itertools::Itertools;
use ratatui::prelude::*;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Returns a [`Line`] whose characters each carry their own span styled with
/// the animated Gaussian wave effect used for the "Press Enter to start the
/// tutorial" prompt.
///
/// The foreground brightness of every span follows a Gaussian wave that
/// travels left-to-right at 25 columns per second and loops after 45 virtual
/// positions.  Because the wave peak is sometimes outside the visible text,
/// there are periods where the whole line appears dim.
///
/// # Parameters
/// * `text`  – the text to render.
/// * `now`   – the current instant; used together with `start_time` to compute
///             elapsed time.
/// * `start_time` – the instant when the animation began; used to maintain
///                  consistent phase across frames.
pub fn gaussian_wave_animated(
    text: &str,
    now: std::time::Instant,
    start_time: std::time::Instant,
) -> Line<'static> {
    let elapsed_secs = now.duration_since(start_time).as_secs_f32();
    let peak_pos = (elapsed_secs * 25.0) % 45.0 - 5.0;

    let spans: Vec<Span<'static>> = text
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            // Gaussian falloff: sigma ≈ 4  →  2σ² = 32
            let dist = (i as f32 - peak_pos).abs();
            let intensity = (-dist * dist / 16.0_f32).exp();
            let brightness = (100.0 + 175.0 * intensity) as u8;
            let style = Style::default().fg(Color::Rgb(brightness, brightness, brightness));
            Span::styled(ch.to_string(), style)
        })
        .collect();

    Line::from(spans)
}

pub fn vec_spans_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|s| s.width()).sum()
}

pub fn take_prefix_of_spans(spans: &[Span<'static>], mut n: usize) -> Vec<Span<'static>> {
    if n == 0 {
        return vec![];
    }

    let mut out: Vec<Span<'static>> = Vec::new();

    for span in spans {
        if n == 0 {
            break;
        }
        let span_width = span.width();
        if span_width <= n {
            out.push(span.clone());
            n -= span_width;
        } else {
            span.content
                .graphemes(true)
                .take_while(|g| {
                    let g_width = g.width();
                    if g_width <= n {
                        n -= g_width;
                        true
                    } else {
                        false
                    }
                })
                .for_each(|g| out.push(Span::styled(g.to_owned(), span.style)));

            break;
        }
    }
    out
}

pub fn take_suffix_of_spans(spans: &[Span<'static>], mut n: usize) -> Vec<Span<'static>> {
    if n == 0 {
        return vec![];
    }

    let mut out: Vec<Span<'static>> = Vec::new();

    for span in spans.iter().rev() {
        if n == 0 {
            break;
        }
        let span_width = span.width();
        if span_width <= n {
            out.push(span.clone());
            n -= span_width;
        } else {
            span.content
                .graphemes(true)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .take_while(|g| {
                    let g_width = g.width();
                    if g_width <= n {
                        n -= g_width;
                        true
                    } else {
                        false
                    }
                })
                .for_each(|g| out.push(Span::styled(g.to_owned(), span.style)));

            break;
        }
    }
    out.reverse();
    out
}

/// Truncate `spans` to at most `max_chars` Unicode characters using middle
/// ellipsis (e.g. `"very_long_name"` → `"very…ame"`), preserving span styles.
pub fn middle_truncate_spans(spans: &[Span<'static>], max_chars: usize) -> Vec<Span<'static>> {
    let total = vec_spans_width(spans);
    if total <= max_chars {
        return spans.to_vec();
    }
    if max_chars == 0 {
        return vec![];
    }
    if max_chars == 1 {
        let style = spans.first().map(|s| s.style).unwrap_or_default();
        return vec![Span::styled("…".to_string(), style)];
    }

    // Reserve 1 char for the ellipsis.
    let keep = max_chars - 1;
    let left = keep / 2;
    let right = keep - left;

    let mut out: Vec<Span<'static>> = Vec::new();
    let mut left_spans = take_prefix_of_spans(spans, left);
    let right_spans = take_suffix_of_spans(spans, right);

    let ellipsis_style = left_spans
        .last()
        .map(|s| s.style)
        .or_else(|| right_spans.first().map(|s| s.style))
        .unwrap_or_default();

    out.append(&mut left_spans);
    out.push(Span::styled("…".to_string(), ellipsis_style));
    out.extend(right_spans);
    out
}

/// Split a single logical line's spans into display rows, each fitting within `available_cols`
/// terminal columns. Returns at least one row (which may be empty if the input line is empty).
pub fn split_line_to_terminal_rows(
    line: &Line<'static>,
    available_cols: u16,
) -> Vec<Line<'static>> {
    if available_cols == 0 {
        return vec![Line::from(vec![])];
    }

    let mut rows: Vec<Line<'static>> = vec![];
    let mut current_spans: Vec<Span<'static>> = vec![];
    let mut current_col: u16 = 0;

    for span in &line.spans {
        let style = span.style;
        let mut current_text = String::new();

        for grapheme in span.content.graphemes(true) {
            let g_width = UnicodeWidthStr::width(grapheme) as u16;

            if g_width == 0 {
                current_text.push_str(grapheme);
                continue;
            }

            if current_col + g_width > available_cols {
                // Flush accumulated text into the current row
                if !current_text.is_empty() {
                    current_spans.push(Span::styled(current_text.clone(), style));
                    current_text.clear();
                }
                // Start a new terminal row
                rows.push(Line::from(std::mem::take(&mut current_spans)));
                current_col = 0;
            }

            current_text.push_str(grapheme);
            current_col += g_width;
        }

        if !current_text.is_empty() {
            current_spans.push(Span::styled(current_text, style));
        }
    }

    // Always push the final (possibly empty) row
    rows.push(Line::from(current_spans));

    rows
}

#[cfg(test)]
mod split_line_to_terminal_rows_tests {
    use super::*;
    use ratatui::text::{Line, Span};

    fn spans_text(rows: &[Line<'static>]) -> Vec<String> {
        rows.iter()
            .map(|row| row.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn test_split_line_fits_in_one_row() {
        let line = Line::from(vec![Span::raw("hello")]);
        let rows = split_line_to_terminal_rows(&line, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(spans_text(&rows), vec!["hello"]);
    }

    #[test]
    fn test_split_line_exact_width() {
        let line = Line::from(vec![Span::raw("hello")]);
        let rows = split_line_to_terminal_rows(&line, 5);
        assert_eq!(rows.len(), 1);
        assert_eq!(spans_text(&rows), vec!["hello"]);
    }

    #[test]
    fn test_split_line_wraps_single_span() {
        // "hello world" with available_cols=6: "hello " fits row 1, "world" fits row 2
        let line = Line::from(vec![Span::raw("hello world")]);
        let rows = split_line_to_terminal_rows(&line, 6);
        assert_eq!(rows.len(), 2);
        assert_eq!(spans_text(&rows), vec!["hello ", "world"]);
    }

    #[test]
    fn test_split_line_wraps_multiple_spans() {
        let line = Line::from(vec![Span::raw("abc"), Span::raw("de"), Span::raw("fg")]);
        // available_cols=4: "abcd" fits, then "efg" wraps to next row
        let rows = split_line_to_terminal_rows(&line, 4);
        assert_eq!(rows.len(), 2);
        // "abc" + "d" fit in row 0, "e" + "fg" in row 1
        assert_eq!(spans_text(&rows), vec!["abcd", "efg"]);
    }

    #[test]
    fn test_split_empty_line() {
        let line = Line::from(vec![]);
        let rows = split_line_to_terminal_rows(&line, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(spans_text(&rows), vec![""]);
    }

    #[test]
    fn test_split_line_zero_available_cols() {
        let line = Line::from(vec![Span::raw("hello")]);
        let rows = split_line_to_terminal_rows(&line, 0);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].spans.is_empty());
    }

    #[test]
    fn test_split_line_long_command() {
        // Simulate a long command that should wrap into multiple rows
        let cmd =
            "git commit -m \"This is a very long commit message that exceeds the terminal width\"";
        let line = Line::from(vec![Span::raw(cmd)]);
        let available_cols = 40u16;
        let rows = split_line_to_terminal_rows(&line, available_cols);
        // Each row should be at most available_cols wide (measured in terminal columns)
        for row in &rows {
            let row_width: usize = row
                .spans
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            assert!(
                row_width <= available_cols as usize,
                "Row too wide: {row_width}"
            );
        }
        // All content should be preserved
        let all_text: String = rows
            .iter()
            .flat_map(|r| r.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert_eq!(all_text, cmd);
    }
}

pub fn apply_match_indices_to_lines(
    palette: &Palette,
    lines: &[Line<'static>],
    match_indices: &[usize],
) -> Vec<Line<'static>> {
    let mut result = Vec::with_capacity(lines.len());
    let mut global_char_offset = 0usize;
    let match_style = palette.matching_char();

    for line in lines {
        let mut new_spans = Vec::new();
        for span in &line.spans {
            let span_start_char = global_char_offset;
            for (is_matching, group) in &span
                .content
                .chars()
                .enumerate()
                .chunk_by(|(char_idx, _)| match_indices.contains(&(span_start_char + char_idx)))
            {
                let s: String = group.map(|(_, c)| c).collect();
                let style = if is_matching {
                    span.style.patch(match_style)
                } else {
                    span.style
                };
                new_spans.push(Span::styled(s, style));
            }
            global_char_offset += span.content.chars().count();
        }
        result.push(Line::from(new_spans));
        global_char_offset += 1; // +1 for the '\n' separator between lines
    }

    result
}

pub fn highlight_matching_indices(
    palette: &Palette,
    s: &str,
    matching_indices: &[usize],
    base_style: Style,
) -> Vec<Line<'static>> {
    let mut normal_lines = Vec::new();

    let mut char_offset = 0usize;
    for text_line in s.split('\n') {
        let line_char_count = text_line.chars().count();
        let line_end_offset = char_offset + line_char_count;

        let relative_indices: Vec<usize> = matching_indices
            .iter()
            .filter(|&&idx| idx >= char_offset && idx < line_end_offset)
            .map(|&idx| idx - char_offset)
            .collect();

        let mut normal_spans = Vec::new();

        for (is_matching, chunk) in &text_line
            .char_indices()
            .chunk_by(|(idx, _)| relative_indices.contains(idx))
        {
            let chunk_str = chunk.map(|(_, c)| c).collect::<String>();
            if is_matching {
                normal_spans.push(Span::styled(
                    chunk_str,
                    base_style.patch(palette.matching_char()),
                ));
            } else {
                normal_spans.push(Span::styled(chunk_str, base_style));
            }
        }

        normal_lines.push(Line::from(normal_spans));

        char_offset = line_end_offset + 1; // +1 for the '\n' character
    }

    normal_lines
}

/// Format a [`Duration`] as a compact, exactly-5-character string.
///
/// The output always occupies exactly 5 terminal columns so it can be used
/// as a fixed-width column in the history list UI.
///
/// | Duration range | Example output |
/// |---|---|
/// | 0 ns (exact zero) | ` now ` |
/// | 1–999 ns | `  1ns` |
/// | 1–999 µs | `  1us` |
/// | 1–999 ms | `  1ms` |
/// | 1–59 s | ` 1sec` |
/// | 1–59 min | ` 1min` |
/// | 1–23 h | ` 1hou` |
/// | 1–30 days | ` 1day` |
/// | 1–11 months | ` 1Mon` |
/// | 1–99 years | ` 1Yea` |
/// | > 99 years | ` OLD ` |
pub fn duration_to_5chars(duration: std::time::Duration) -> String {
    const S_IN_MNTH: u64 = 2_628_003;
    let s = duration.as_secs();
    let raw = match s {
        0 => {
            let ns = duration.as_nanos() as u64;
            if ns == 0 {
                " now ".into()
            } else if ns < 1_000 {
                format!("{ns:03}ns")
            } else if ns < 1_000_000 {
                format!("{:03}us", ns / 1_000)
            } else {
                format!("{:03}ms", ns / 1_000_000)
            }
        }
        x if (1..60).contains(&x) => format!("{x:02}sec"),
        x if (60..3600).contains(&x) => format!("{:02}min", x / 60),
        x if (3600..86400).contains(&x) => format!("{:02}hou", x / 3600),
        x if (86400..S_IN_MNTH).contains(&x) => format!("{:02}day", x / 86400),
        x if (S_IN_MNTH..(12 * S_IN_MNTH)).contains(&x) => format!("{:02}Mon", x / S_IN_MNTH),
        x if ((12 * S_IN_MNTH)..=(99 * 12 * S_IN_MNTH)).contains(&x) => {
            format!("{:02}Yea", x / (12 * S_IN_MNTH))
        }
        _ => " OLD ".into(),
    };
    format!("{:>5}", raw.trim_start_matches('0'))
}

pub fn ts_to_timeago_string_5chars(ts: u64) -> String {
    let duration = std::time::Duration::from_secs(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(ts),
    );
    duration_to_5chars(duration)
}

#[cfg(test)]
mod tests {
    use super::duration_to_5chars;
    use std::time::Duration;

    #[test]
    fn test_duration_to_5chars_now() {
        assert_eq!(duration_to_5chars(Duration::from_secs(0)), " now ");
    }

    #[test]
    fn test_duration_to_5chars_nanoseconds() {
        assert_eq!(duration_to_5chars(Duration::from_nanos(1)), "  1ns");
        assert_eq!(duration_to_5chars(Duration::from_nanos(42)), " 42ns");
        assert_eq!(duration_to_5chars(Duration::from_nanos(999)), "999ns");
    }

    #[test]
    fn test_duration_to_5chars_microseconds() {
        assert_eq!(duration_to_5chars(Duration::from_micros(1)), "  1us");
        assert_eq!(duration_to_5chars(Duration::from_micros(42)), " 42us");
        assert_eq!(duration_to_5chars(Duration::from_micros(999)), "999us");
    }

    #[test]
    fn test_duration_to_5chars_milliseconds() {
        assert_eq!(duration_to_5chars(Duration::from_millis(1)), "  1ms");
        assert_eq!(duration_to_5chars(Duration::from_millis(42)), " 42ms");
        assert_eq!(duration_to_5chars(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn test_duration_to_5chars_seconds() {
        assert_eq!(duration_to_5chars(Duration::from_secs(1)), " 1sec");
        assert_eq!(duration_to_5chars(Duration::from_secs(9)), " 9sec");
        assert_eq!(duration_to_5chars(Duration::from_secs(59)), "59sec");
    }

    #[test]
    fn test_duration_to_5chars_minutes() {
        assert_eq!(duration_to_5chars(Duration::from_secs(60)), " 1min");
        assert_eq!(duration_to_5chars(Duration::from_secs(3599)), "59min");
    }

    #[test]
    fn test_duration_to_5chars_hours() {
        assert_eq!(duration_to_5chars(Duration::from_secs(3600)), " 1hou");
        assert_eq!(duration_to_5chars(Duration::from_secs(86399)), "23hou");
    }

    #[test]
    fn test_duration_to_5chars_days() {
        assert_eq!(duration_to_5chars(Duration::from_secs(86400)), " 1day");
    }

    #[test]
    fn test_duration_to_5chars_months() {
        assert_eq!(duration_to_5chars(Duration::from_secs(2_628_003)), " 1Mon");
    }

    #[test]
    fn test_duration_to_5chars_years() {
        assert_eq!(
            duration_to_5chars(Duration::from_secs(12 * 2_628_003)),
            " 1Yea"
        );
        assert_eq!(
            duration_to_5chars(Duration::from_secs(99 * 12 * 2_628_003)),
            "99Yea"
        );
    }

    #[test]
    fn test_duration_to_5chars_old() {
        assert_eq!(
            duration_to_5chars(Duration::from_secs(100 * 12 * 2_628_003)),
            " OLD "
        );
    }
}

/// Convert an ANSI-escaped string into a flat list of styled [`Span`]s.
///
/// The string is parsed through [`ansi_to_tui`] so that ANSI colour/style
/// codes are converted to ratatui span styles.  If parsing fails the raw text
/// is returned as a single unstyled span.  Spans from all resulting lines are
/// flattened into one sequence (descriptions are expected to be single-line).
pub fn ansi_string_to_spans(s: &str) -> Vec<Span<'static>> {
    let owned = s.to_owned();
    match owned.into_text() {
        Ok(text) => text.lines.into_iter().flat_map(|l| l.spans).collect(),
        Err(_) => vec![Span::raw(s.to_owned())],
    }
}

/// Build the ping-pong animation frames for the given easing function.
pub fn easing_animation_frames(easing: CursorEasing) -> Vec<Vec<Span<'static>>> {
    /// Easing preview cycle frequency in hertz.
    const EASING_ANIM_TARGET_HZ: f32 = 0.5;

    /// Total width (in terminal columns) of the easing-function dot animation.
    const EASING_ANIM_TOTAL_WIDTH: usize = 10;
    /// Dot can be in the left or right half of a cell, so logical width is double the total width.
    const EASING_ANIM_LOGICAL_WIDTH: usize = EASING_ANIM_TOTAL_WIDTH * 2;

    /// Inner boundary start column (inclusive) that represents easing value 0.0.
    const EASING_ANIM_BOUNDARY_START: usize = 1;
    const EASING_ANIM_BOUNDARY_LOGICAL_START: usize = EASING_ANIM_BOUNDARY_START * 2;

    /// Inner boundary end column (inclusive) that represents easing value 1.0.
    const EASING_ANIM_BOUNDARY_END: usize = EASING_ANIM_TOTAL_WIDTH.saturating_sub(2);
    const EASING_ANIM_BOUNDARY_LOGICAL_END: usize = EASING_ANIM_BOUNDARY_END * 2;

    let cycle_frames =
        ((ANIMATION_FRAME_FPS as f32 / EASING_ANIM_TARGET_HZ).round() as usize).max(2);
    let dot_logical_range =
        (EASING_ANIM_BOUNDARY_LOGICAL_END - EASING_ANIM_BOUNDARY_LOGICAL_START) as f32;
    let mut frames = Vec::with_capacity(cycle_frames);

    let make_frame = |pos: isize| -> Vec<Span<'static>> {
        let clamped_pos = pos.clamp(0, EASING_ANIM_LOGICAL_WIDTH as isize - 1) as usize;

        // Build a 4-row × EASING_ANIM_LOGICAL_WIDTH-col bool grid (row-major).
        let mut grid = vec![vec![false; EASING_ANIM_LOGICAL_WIDTH]; 4];

        // Rows 0 and 3: "rails" spanning the boundary region.
        for j in EASING_ANIM_BOUNDARY_LOGICAL_START..=EASING_ANIM_BOUNDARY_LOGICAL_END {
            grid[0][j] = true;
            grid[3][j] = true;
        }

        // Rows 1 and 2: the moving dot at the current logical position.
        grid[1][clamped_pos] = true;
        grid[2][clamped_pos] = true;

        let s = octant_from_grid(&grid, OctantStyle::Braille)
            .into_iter()
            .next()
            .unwrap_or_default();
        vec![Span::raw(s)]
    };

    for i in 0..cycle_frames {
        let t = i as f32 / (cycle_frames - 1) as f32;
        let pos = (EASING_ANIM_BOUNDARY_LOGICAL_START as f32 + easing.apply(t) * dot_logical_range)
            .round() as isize;
        frames.push(make_frame(pos));
    }

    frames
}
