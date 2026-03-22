use rand::prelude::*;
use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, StyledGrapheme};
use std::iter;
use std::sync::{Mutex, OnceLock};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coord {
    pub row: u16,
    pub col: u16,
}

impl Coord {
    pub fn new(row: u16, col: u16) -> Self {
        Coord { row, col }
    }

    pub fn abs_diff(&self, other: &Coord) -> usize {
        self.col.abs_diff(other.col) as usize + self.row.abs_diff(other.row) as usize
    }

    pub fn interpolate(&self, other: &Coord, factor: f32) -> Coord {
        // factor = 0.0 => self
        // factor = 1.0 => other
        let col = self.col as f32 + (other.col as f32 - self.col as f32) * factor;
        let row = self.row as f32 + (other.row as f32 - self.row as f32) * factor;
        Coord::new(row.round() as u16, col.round() as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Blank,
    Ps1Prompt,
    Ps2Prompt,
    Command(usize),
    TabSuggestion,
    Suggestion(usize),
    HistorySuggestion,
    FuzzySearch,
    HistoryResult(usize),
    Tooltip,
    AiResult(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedCell {
    pub cell: Cell,
    pub tag: Tag,
}

impl Default for TaggedCell {
    fn default() -> Self {
        TaggedCell {
            cell: Cell::default(),
            tag: Tag::Blank,
        }
    }
}

impl TaggedCell {
    pub fn update(&mut self, graph: &StyledGrapheme, tag: Tag) {
        self.cell.set_symbol(graph.symbol).set_style(graph.style);
        self.tag = tag;
    }
}

pub struct Contents {
    pub buf: Vec<Vec<TaggedCell>>, // each inner Vec is a row of Cells of width `width`
    pub width: u16,
    cursor_pos: Coord, // visual cursor position with line wrapping
    /// Where the terminal emulator thinks the cursor is.
    pub term_cursor_pos: Option<Coord>,
    /// Whether to tell the term emulator to move the cursor here
    pub use_term_emulator_cursor: bool,
}

impl Contents {
    // All the line wrapping logic is handled here.
    // So app::ui just handles lines according to the edit buffer

    /// Create a new Content with an empty buffer for the given area
    pub fn new(width: u16) -> Self {
        Contents {
            buf: vec![],
            width,
            cursor_pos: Coord::new(0, 0),
            term_cursor_pos: None,
            use_term_emulator_cursor: false,
        }
    }

    /// Get the current cursor position (x, y)
    pub fn cursor_position(&self) -> Coord {
        self.cursor_pos
    }

    pub fn increase_buf_single_row(&mut self) {
        let blank_row = vec![TaggedCell::default(); self.width as usize];
        self.buf.push(blank_row);
    }

    pub fn height(&self) -> u16 {
        self.buf.len() as u16
    }

    pub fn move_to_next_insertion_point(&mut self, graph: &StyledGrapheme, overwrite: bool) {
        let graph_w = graph.symbol.width() as u16;
        loop {
            if self.cursor_pos.row >= self.buf.len() as u16 {
                self.increase_buf_single_row();
            } else if self.cursor_pos.col as usize + graph_w as usize > self.width as usize {
                // log::debug!("Wrapping line for grapheme of width {}", graph_w);
                self.cursor_pos.row += 1;
                self.cursor_pos.col = 0;
            } else if !overwrite
                && self.buf[self.cursor_pos.row as usize][(self.cursor_pos.col as usize)
                    ..(self.cursor_pos.col as usize + graph_w as usize)]
                    .iter()
                    .all(|cell| cell.tag == Tag::Blank)
            {
                break;
            } else if overwrite {
                break;
            } else {
                self.cursor_pos.col += 1;
            }
        }
    }

    /// Write a single span at the current cursor position
    /// Will automatically wrap to the next line if necessary
    fn write_span_internal(
        &mut self,
        span: &Span,
        graph_idx_to_tag: impl Fn(usize) -> Tag,
        overwrite: bool,
        mark_nth_grapheme: Option<usize>,
    ) -> Option<Coord> {
        let graphemes = span.styled_graphemes(span.style);
        let mut marked_graph_coord = None;

        for (i, graph) in graphemes.enumerate() {
            let graph_w = graph.symbol.width() as u16;
            if graph_w == 0 {
                continue;
            }

            self.move_to_next_insertion_point(&graph, overwrite);

            let next_graph_x = self.cursor_pos.col + graph_w;
            if next_graph_x > self.width {
                // cold_path();
                // If the grapheme is still too wide after wrapping, skip it
                // We probably start at cursor_pos_x=0 here, so very unlikely to happen
                log::warn!(
                    "Grapheme too wide for line: '{}' (width {})",
                    graph.symbol,
                    graph_w
                );
                continue;
            }
            if Some(i) == mark_nth_grapheme {
                marked_graph_coord = Some(self.cursor_pos);
            }

            self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                .update(&graph, graph_idx_to_tag(i));
            self.cursor_pos.col += 1;
            // Reset following cells if multi-width (they would be hidden by the grapheme),
            while self.cursor_pos.col < next_graph_x {
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                    .cell
                    .reset();
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize].tag =
                    graph_idx_to_tag(i);
                self.cursor_pos.col += 1;
            }
        }
        marked_graph_coord
    }

    pub fn write_span_dont_overwrite(
        &mut self,
        span: &Span,
        graph_idx_to_tag: impl Fn(usize) -> Tag,
        mark_nth_grapheme: Option<usize>,
    ) -> Option<Coord> {
        self.write_span_internal(span, graph_idx_to_tag, false, mark_nth_grapheme)
    }

    pub fn write_span(&mut self, span: &Span, tag: Tag) {
        self.write_span_internal(span, |_| tag, true, None);
    }

    /// Write a line at the current cursor position
    /// If insert_new_line is true, moves to the next line after writing
    pub fn write_line(&mut self, line: &Line, insert_new_line: bool, tag: Tag) {
        for span in &line.spans {
            self.write_span(span, tag);
        }
        if insert_new_line {
            self.newline();
        }
    }

    pub fn write_line_lrjustified(
        &mut self,
        l_line: &Line,
        fill_line: &Line,
        r_line: &Line,
        tag: Tag,
        leave_cursor_after_l_line: bool,
    ) {
        let r_width = r_line.width() as u16;
        let starting_row = self.cursor_pos.row;
        self.write_line(l_line, false, tag);

        let cursor_after_l_line = self.cursor_pos.col;

        if self.cursor_pos.row == starting_row {
            let target_col = self.width.saturating_sub(r_width);

            // Collect all styled graphemes from the fill line
            let fill_graphemes: Vec<StyledGrapheme> = fill_line
                .spans
                .iter()
                .flat_map(|span| span.styled_graphemes(span.style))
                .collect();

            let has_nonzero_width = fill_graphemes.iter().any(|g| g.symbol.width() > 0);

            if !has_nonzero_width {
                // Zero-width fill: no progress can be made, just move the cursor
                self.cursor_pos.col = target_col;
            } else if fill_graphemes.len() == 1
                && fill_graphemes[0].symbol == " "
                && fill_graphemes[0].style == ratatui::style::Style::default()
            {
                // Filling with unstyled spaces: just move the cursor without writing fill chars
                self.cursor_pos.col = target_col;
            } else {
                // Cycle through graphemes one at a time until there isn't room for the next one
                let mut idx = 0;
                loop {
                    let graph = &fill_graphemes[idx % fill_graphemes.len()];
                    let graph_w = graph.symbol.width() as u16;
                    if graph_w == 0 {
                        idx += 1;
                        continue;
                    }
                    if self.cursor_pos.col + graph_w > target_col {
                        break;
                    }
                    let span = Span::styled(graph.symbol.to_string(), graph.style);
                    self.write_span(&span, tag);
                    idx += 1;
                }
                // Move cursor to where right-aligned content should start
                self.cursor_pos.col = target_col;
            }
        }
        if r_width > 0 {
            self.write_line(r_line, false, tag);
        }

        if leave_cursor_after_l_line {
            self.cursor_pos.row = starting_row;
            self.cursor_pos.col = cursor_after_l_line;
        }
    }

    /// Move the cursor to a specific column on the current row.
    /// This allows positioning the cursor before writing content (e.g. right-aligned ellipsis).
    /// `col` is clamped to `self.width` to avoid an inconsistent cursor position.
    pub fn set_cursor_col(&mut self, col: u16) {
        self.cursor_pos.col = col.min(self.width);
    }

    /// Fill the rest of the current row with spaces tagged with the given tag
    pub fn fill_line(&mut self, tag: Tag) {
        let remaining = self.width.saturating_sub(self.cursor_pos.col) as usize;
        if remaining > 0 {
            self.write_span(&Span::raw(" ".repeat(remaining)), tag);
        }
    }

    /// Move to the next line (carriage return + line feed)
    pub fn newline(&mut self) {
        self.cursor_pos.row += 1;
        self.cursor_pos.col = 0;
    }

    fn set_style(&mut self, area: Rect, style: ratatui::style::Style) {
        for _ in self.buf.len()..area.bottom() as usize {
            self.increase_buf_single_row();
        }

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(row) = self.buf.get_mut(y as usize)
                    && let Some(tagged_cell) = row.get_mut(x as usize)
                {
                    tagged_cell.cell.set_style(style);
                }
            }
        }
    }

    pub fn set_term_cursor_pos(
        &mut self,
        cursor: Coord,
        style: Option<ratatui::style::Style>,
        use_term_emulator_cursor: bool,
    ) {
        self.term_cursor_pos = Some(cursor);
        self.use_term_emulator_cursor = use_term_emulator_cursor;
        if let Some(style) = style {
            self.set_style(Rect::new(cursor.col, cursor.row, 1, 1), style);
        }
    }

    pub fn get_row_range_to_show(&self, height: u16) -> (u16, u16) {
        // Returns the range of visual rows to show given the available height
        let total_rows = self.height();
        if total_rows <= height {
            (0, total_rows)
        } else if let Some(cursor) = self.term_cursor_pos {
            let bottom = std::cmp::min(cursor.row.saturating_add(1), total_rows);
            let top = bottom.saturating_sub(height);
            (top, bottom)
        } else {
            // Show the final rows
            (total_rows.saturating_sub(height), total_rows)
        }
    }

    pub fn get_tagged_cell(
        &self,
        term_em_x: u16,
        term_em_y: u16,
        term_em_offset: i16,
    ) -> Option<(Tag, bool)> {
        // log::debug!(
        //     "Getting tagged cell at terminal em coords ({}, {}), offset {}",
        //     term_em_x,
        //     term_em_y,
        //     term_em_offset
        // );
        if term_em_offset > term_em_y as i16 {
            // log::debug!(
            //     "Offset {} is greater than term_em_y {}, returning None",
            //     term_em_offset,
            //     term_em_y
            // );
            return None;
        }

        let direct_contact = self
            .buf
            .get(term_em_y.saturating_sub_signed(term_em_offset) as usize)
            .and_then(|row| row.get(term_em_x as usize));

        if direct_contact.is_some_and(|cell| {
            matches!(
                cell.tag,
                Tag::Command(_) | Tag::Suggestion(_) | Tag::HistoryResult(_) | Tag::AiResult(_)
            )
        }) {
            return direct_contact.map(|cell| (cell.tag, true));
        }

        self.buf
            .get(term_em_y.saturating_sub_signed(term_em_offset) as usize)
            .and_then(|row| {
                row.iter().enumerate().rev().find(|(col_idx, tagged_cell)| {
                    *col_idx <= term_em_x as usize && matches!(tagged_cell.tag, Tag::Command(_))
                })
            })
            .map(|(_, cell)| (cell.tag, false))
    }

    pub fn apply_matrix_anim(&mut self, now: std::time::Instant) {
        // TODO choose a better max height:
        for _ in self.buf.len()..20 {
            self.increase_buf_single_row();
        }

        let mut state = MATRIX_ANIM_STATE
            .get_or_init(|| Mutex::new(MatrixAnimState::new()))
            .lock()
            .unwrap();
        state.update(now, self.width, self.height());

        for (col_idx, _tendril) in state.tendrils.iter().enumerate() {
            let styled_graphs = state.tendril_idx_to_graphemes(col_idx);
            for (row_idx, styled_graph) in styled_graphs.into_iter().enumerate() {
                if let Some(row) = self.buf.get_mut(row_idx)
                    && let Some(cell) = row.get_mut(col_idx)
                    && cell.tag == Tag::Blank
                {
                    cell.cell
                        .set_symbol(styled_graph.symbol)
                        .set_style(styled_graph.style);
                }
            }
        }
    }
}

static MATRIX_ANIM_STATE: OnceLock<Mutex<MatrixAnimState>> = OnceLock::new();

#[derive(Debug, Clone)]
struct MatrixAnimState {
    last_update_time: std::time::Instant,
    // tendrils[i] is the y position of the falling "tendril" in column i, or None if there is no tendril currently in that column
    // y might be off the screen but we still want to show the tail of the tendril until it fully disappears
    tendrils: Vec<Option<usize>>,
}

impl MatrixAnimState {
    fn new() -> Self {
        MatrixAnimState {
            last_update_time: std::time::Instant::now(),
            tendrils: vec![],
        }
    }

    const TENDRIL_MAX_LEN: usize = 5;

    fn tendril_idx_to_graphemes(&self, idx: usize) -> Vec<StyledGrapheme<'static>> {
        // Some observations:
        // The leading char in the tendril should be bright, bold white
        // Characters should fade with age down the tendril, with the tail being very dim (e.g. dark green)
        // A mix of non-English chars looks good
        // Occasionally a character will change while the tendril is falling.

        const CHAR_SET: &[&str] = &[
            "ｱ", "ｲ", "ｳ", "ｴ", "ｵ", "ｶ", "ｷ", "ｸ", "ｹ", "ｺ", "ｻ", "ｼ", "ｽ", "ｾ", "ｿ", "ﾀ", "ﾁ",
            "ﾂ", "ﾃ", "ﾄ", "ﾅ", "ﾆ", "ﾇ", "ﾈ", "ﾉ", "ﾊ", "ﾋ", "ﾌ", "ﾍ", "ﾎ", "ﾏ", "ﾐ", "ﾑ", "ﾒ",
            "ﾓ", "ﾔ", "ﾕ", "ﾖ", "ﾗ", "ﾘ", "ﾙ", "ﾚ", "ﾛ", "ﾜ", "ｦ",
            // Some ASCII chars mixed in
            "@", "#", "$", "%", "&", "*", "+", "-", "=", "?", "A", "B", "C", "D", "E", "F", "G",
            "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X",
            "Y", "Z",
        ];

        if let Some(tendril_max_y) = self.tendrils.get(idx).and_then(|&t| t) {
            let mut graphemes = vec![];
            for y in 0..Self::TENDRIL_MAX_LEN {
                let age_factor = y as f32 / Self::TENDRIL_MAX_LEN as f32;
                // let symbol = CHAR_SET.choose(&mut rand::thread_rng()).unwrap_or(&' ');
                // let symbol = CHAR_SET[(y + idx) % CHAR_SET.len()];
                let symbol = CHAR_SET[(y + idx) % CHAR_SET.len()];
                let style = if y == Self::TENDRIL_MAX_LEN - 1 {
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::White)
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else if age_factor > 0.5 {
                    ratatui::style::Style::default().fg(ratatui::style::Color::Green)
                } else {
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::Green)
                        .add_modifier(ratatui::style::Modifier::DIM)
                };
                graphemes.push(StyledGrapheme::new(symbol, style));
            }

            iter::repeat(&StyledGrapheme::new(" ", ratatui::style::Style::default()))
                .take(tendril_max_y.saturating_sub(Self::TENDRIL_MAX_LEN))
                .chain(graphemes.iter().take(tendril_max_y + 1))
                .cloned()
                .collect()
        } else {
            vec![]
        }
    }

    fn update(&mut self, now: std::time::Instant, num_cols: u16, num_rows: u16) {
        const NUM_ROWS_PER_SECOND: f32 = 12.0;
        const MS_PER_ROW: f32 = 1000.0 / NUM_ROWS_PER_SECOND;
        let steps_elapsed =
            (now.duration_since(self.last_update_time).as_millis() as f32 / MS_PER_ROW) as usize;
        if steps_elapsed == 0 {
            return;
        }
        self.last_update_time = now;

        self.tendrils.resize(num_cols as usize, None);

        for _ in 0..steps_elapsed {
            // Move existing tendrils down
            for tendril in &mut self.tendrils {
                if let Some(y) = tendril {
                    *y += 1;
                }
            }

            // Remove tendrils that have moved off the bottom of the screen
            let max_possible_tendril_height = num_rows as usize + Self::TENDRIL_MAX_LEN;
            for tendril in &mut self.tendrils {
                if let Some(y) = tendril {
                    if *y >= max_possible_tendril_height {
                        *tendril = None;
                    }
                }
            }

            // Spawn new tendrils with some probability
            for tendril in &mut self.tendrils {
                let rand = rand::random::<f32>();
                if tendril.is_none() && rand < 0.1 {
                    *tendril = Some(0);
                }
            }
        }
    }
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
mod tests {
    use super::*;
    use ratatui::text::{Line, Span};

    #[test]
    fn test_write_zero_width_span_to_full_buffer() {
        let width = 10u16;
        let mut contents = Contents::new(width);

        // Fill two rows completely
        let row_text = "a".repeat(width as usize);
        contents.write_line(&Line::from(row_text.clone()), true, Tag::Blank);
        contents.write_line(&Line::from(row_text.clone()), true, Tag::Blank);

        assert_eq!(contents.height(), 2);

        // Append a span containing a zero-width character
        let zero_width_span = Span::raw("\u{200B}"); // zero-width space
        contents.write_span(&zero_width_span, Tag::Blank);

        // The buffer should still have 2 rows — zero-width span must not add a new row
        assert_eq!(contents.height(), 2);
    }

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
