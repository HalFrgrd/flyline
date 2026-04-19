use crate::{cursor::CursorEasing, palette::Palette};
use ansi_to_tui::IntoText;
use itertools::Itertools;
use ratatui::prelude::*;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

pub fn ts_to_timeago_string_5chars(ts: u64) -> String {
    let duration = std::time::Duration::from_secs(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(ts),
    );
    let s = timeago::format_5chars(duration);
    format!("{:>5}", s.trim_start_matches('0'))
}

/// Total width (in terminal columns) of the easing-function dot animation.
const EASING_ANIM_TOTAL_WIDTH: usize = 7;

/// Number of frames in each half of the ping-pong animation (forward + backward).
const EASING_ANIM_HALF_FRAMES: usize = 16;

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
///
/// Returns `2 * EASING_ANIM_HALF_FRAMES - 2` frames showing a dot (`·`) that
/// travels from the left edge to the right and back, using `easing` for the
/// position curve in both directions.  Each frame is a `Vec<Span<'static>>`.
pub fn easing_animation_frames(easing: CursorEasing) -> Vec<Vec<Span<'static>>> {
    let half = EASING_ANIM_HALF_FRAMES;
    let dot_range = (EASING_ANIM_TOTAL_WIDTH - 1) as f32;
    let total_frames = half * 2 - 2;
    let mut frames = Vec::with_capacity(total_frames);

    let make_frame = |pos: usize| -> Vec<Span<'static>> {
        let mut s = String::with_capacity(EASING_ANIM_TOTAL_WIDTH);
        for j in 0..EASING_ANIM_TOTAL_WIDTH {
            if j == pos {
                s.push('·');
            } else {
                s.push(' ');
            }
        }
        vec![Span::raw(s)]
    };

    // Forward: t goes 0 → 1
    for i in 0..half {
        let t = i as f32 / (half - 1) as f32;
        let pos = (easing.apply(t) * dot_range).round() as usize;
        frames.push(make_frame(pos.min(EASING_ANIM_TOTAL_WIDTH - 1)));
    }
    // Backward: t goes from the second-to-last back to 0 (skip first and last to avoid repeats)
    for i in (1..half - 1).rev() {
        let t = i as f32 / (half - 1) as f32;
        let pos = (easing.apply(t) * dot_range).round() as usize;
        frames.push(make_frame(pos.min(EASING_ANIM_TOTAL_WIDTH - 1)));
    }

    frames
}
