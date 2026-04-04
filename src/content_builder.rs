use rand::prelude::*;
use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, StyledGrapheme};
use std::collections::HashMap;
use std::sync::Mutex;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Describes how [`Tag`]s are applied to the graphemes of a [`TaggedSpan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanTag {
    /// Every grapheme in the span gets the same tag.
    Constant(Tag),
    /// One tag per grapheme in the span, indexed by grapheme position.
    /// Falls back to [`Tag::Normal`] for out-of-range indices.
    PerGrapheme(Vec<Tag>),
}

impl SpanTag {
    /// Return the tag for the grapheme at `idx`.
    pub fn get(&self, idx: usize) -> Tag {
        match self {
            SpanTag::Constant(tag) => *tag,
            SpanTag::PerGrapheme(tags) => tags.get(idx).copied().unwrap_or(Tag::Normal),
        }
    }
}

/// A ratatui [`Span`] paired with a [`SpanTag`] that describes the semantic tag
/// for each grapheme in the span.
#[derive(Debug, Clone)]
pub struct TaggedSpan<'a> {
    pub span: Span<'a>,
    pub tag: SpanTag,
}

impl<'a> TaggedSpan<'a> {
    /// Create a `TaggedSpan` where every grapheme gets the same `tag`.
    pub fn new(span: Span<'a>, tag: Tag) -> Self {
        TaggedSpan {
            span,
            tag: SpanTag::Constant(tag),
        }
    }

    /// Create a `TaggedSpan` with a per-grapheme tag vector.
    pub fn per_grapheme(span: Span<'a>, tags: Vec<Tag>) -> Self {
        TaggedSpan {
            span,
            tag: SpanTag::PerGrapheme(tags),
        }
    }
}

impl<'a> From<Span<'a>> for TaggedSpan<'a> {
    /// Converts a [`Span`] into a [`TaggedSpan`] with [`Tag::Normal`] applied to all graphemes.
    fn from(span: Span<'a>) -> Self {
        TaggedSpan::new(span, Tag::Normal)
    }
}

/// A sequence of [`TaggedSpan`]s forming a logical line, analogous to ratatui's [`Line`].
#[derive(Debug, Clone, Default)]
pub struct TaggedLine<'a> {
    pub spans: Vec<TaggedSpan<'a>>,
}

impl<'a> TaggedLine<'a> {
    /// Create a [`TaggedLine`] from a ratatui [`Line`], assigning `tag` to every span.
    pub fn from_line(line: Line<'a>, tag: Tag) -> Self {
        TaggedLine {
            spans: line
                .spans
                .into_iter()
                .map(|s| TaggedSpan::new(s, tag))
                .collect(),
        }
    }

    /// Return the total display width of all spans in the line, in terminal columns.
    pub fn width(&self) -> u16 {
        self.spans.iter().map(|ts| ts.span.width() as u16).sum()
    }
}

impl<'a> From<Vec<TaggedSpan<'a>>> for TaggedLine<'a> {
    fn from(spans: Vec<TaggedSpan<'a>>) -> Self {
        TaggedLine { spans }
    }
}

use crate::stateful_sliding_window::StatefulSlidingWindow;

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
    Normal,
    Ps1Prompt,
    Ps1PromptCwd(usize),
    Ps1PromptDynamicTime,
    Ps1PromptAnimation,
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
    /// The row to keep visible when content exceeds the terminal height.
    /// Falls back to the cursor row when `None`; set by fuzzy search, tab completions,
    /// and AI selection mode to point at the currently selected item.
    pub focus_row: Option<u16>,
    pub prompt_start: Option<Coord>,
    pub prompt_end: Option<Coord>,
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
            focus_row: None,
            prompt_start: None,
            prompt_end: None,
        }
    }

    /// Set the focus row – the row that `get_row_range_to_show` will try to keep visible.
    pub fn set_focus_row(&mut self, row: u16) {
        self.focus_row = Some(row);
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

    /// Write a single tagged span at the current cursor position.
    /// Will automatically wrap to the next line if necessary.
    fn write_span_internal(
        &mut self,
        tagged_span: &TaggedSpan,
        overwrite: bool,
        mark_nth_grapheme: Option<usize>,
    ) -> Option<Coord> {
        let graphemes = tagged_span.span.styled_graphemes(tagged_span.span.style);
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

            let tag = tagged_span.tag.get(i);
            self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                .update(&graph, tag);
            self.cursor_pos.col += 1;
            // Reset following cells if multi-width (they would be hidden by the grapheme),
            while self.cursor_pos.col < next_graph_x {
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                    .cell
                    .reset();
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize].tag = tag;
                self.cursor_pos.col += 1;
            }
        }
        marked_graph_coord
    }

    /// Write a tagged span at the current cursor position, skipping cells that are already filled.
    /// Returns the coordinate of the `mark_nth_grapheme`-th grapheme, if present.
    pub fn write_tagged_span_dont_overwrite(
        &mut self,
        tagged_span: &TaggedSpan,
        mark_nth_grapheme: Option<usize>,
    ) -> Option<Coord> {
        self.write_span_internal(tagged_span, false, mark_nth_grapheme)
    }

    /// Write a tagged span at the current cursor position, overwriting any existing content.
    pub fn write_tagged_span(&mut self, tagged_span: &TaggedSpan) {
        self.write_span_internal(tagged_span, true, None);
    }

    /// Write a tagged line at the current cursor position.
    /// If `insert_new_line` is true, moves to the next line after writing.
    pub fn write_tagged_line(&mut self, line: &TaggedLine, insert_new_line: bool) {
        for tagged_span in &line.spans {
            self.write_tagged_span(tagged_span);
        }
        if insert_new_line {
            self.newline();
        }
    }

    /// Write a tagged line left-aligned, fill the gap, then write another tagged line
    /// right-aligned — all on the same terminal row.
    ///
    /// If the left line wraps to a second row the fill and right line are skipped.
    /// When `leave_cursor_after_l_line` is true the cursor is restored to the position
    /// immediately after the left line once the function returns.
    pub fn write_tagged_line_lrjustified(
        &mut self,
        l_line: &TaggedLine,
        fill_line: &TaggedLine,
        r_line: &TaggedLine,
        leave_cursor_after_l_line: bool,
    ) {
        let r_width = r_line.width();
        let starting_row = self.cursor_pos.row;
        self.write_tagged_line(l_line, false);

        let cursor_after_l_line = self.cursor_pos.col;

        if self.cursor_pos.row == starting_row {
            let target_col = self.width.saturating_sub(r_width);

            // Collect styled graphemes and their tags from the fill line.
            let fill_graphemes: Vec<StyledGrapheme> = fill_line
                .spans
                .iter()
                .flat_map(|ts| ts.span.styled_graphemes(ts.span.style))
                .collect();
            let fill_grapheme_tags: Vec<Tag> = fill_line
                .spans
                .iter()
                .flat_map(|ts| {
                    ts.span
                        .content
                        .graphemes(true)
                        .enumerate()
                        .map(|(i, _)| ts.tag.get(i))
                })
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
                    let fill_tag = fill_grapheme_tags[idx % fill_grapheme_tags.len()];
                    let span = Span::styled(graph.symbol.to_string(), graph.style);
                    self.write_tagged_span(&TaggedSpan::new(span, fill_tag));
                    idx += 1;
                }
                // Move cursor to where right-aligned content should start
                self.cursor_pos.col = target_col;
            }
        }
        if r_width > 0 {
            self.write_tagged_line(r_line, false);
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
            self.write_tagged_span(&TaggedSpan::new(Span::raw(" ".repeat(remaining)), tag));
        }
    }

    pub fn move_to_final_line(&mut self) {
        self.cursor_pos.row = self.buf.len().saturating_sub(1) as u16;
        self.cursor_pos.col = 0;
    }

    /// Move to the next line (carriage return + line feed)
    pub fn newline(&mut self) {
        self.cursor_pos.row += 1;
        self.cursor_pos.col = 0;
        for _ in self.buf.len()..(self.cursor_pos.row as usize + 1) {
            self.increase_buf_single_row();
        }
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

    pub fn set_term_cursor_pos(&mut self, cursor: Coord, style: Option<ratatui::style::Style>) {
        self.term_cursor_pos = Some(cursor);
        if let Some(style) = style {
            self.set_style(Rect::new(cursor.col, cursor.row, 1, 1), style);
        }
    }

    pub fn get_row_range_to_show(&self, term_height: u16) -> std::ops::Range<u16> {
        let mut window =
            StatefulSlidingWindow::new(0, term_height as usize, self.height() as usize);
        if let Some(focus_row) = self.focus_row {
            window.move_index_to(focus_row as usize);
        } else if let Some(term_cursor_pos) = self.term_cursor_pos {
            window.move_index_to(term_cursor_pos.row as usize);
        }

        let range = window.get_window_range();
        range.start as u16..range.end as u16
    }

    pub fn apply_matrix_anim(
        &mut self,
        now: std::time::Instant,
        viewport_top: u16,
        terminal_height: u16,
    ) {
        // Extend the buffer so it reaches the bottom of the terminal from the viewport top.
        let rows_needed = terminal_height.saturating_sub(viewport_top) as usize;
        if rows_needed == 0 {
            return;
        }
        for _ in self.buf.len()..rows_needed {
            self.increase_buf_single_row();
        }

        let mut state_guard = MATRIX_ANIM_STATE.lock().unwrap();
        let state = state_guard.get_or_insert_with(MatrixAnimState::new);
        let just_started = state.tendrils.is_empty();
        // State is updated using the full terminal height so tendril positions are
        // terminal-absolute (row 0 = top of terminal, not top of viewport).
        state.update(now, self.width, terminal_height);
        // When the animation has just started and the viewport is below the top of the
        // terminal, fast-forward so that tendrils are already visible in the viewport
        // rather than needing to fall viewport_top rows before becoming visible.
        if just_started && viewport_top > 0 {
            for _ in 0..viewport_top {
                state.step(terminal_height);
            }
        }

        for (col_idx, _tendril) in state.tendrils.iter().enumerate() {
            let styled_graphs = state.tendril_idx_to_graphemes(col_idx);
            // styled_graphs[i] corresponds to terminal-absolute row i.
            // Skip rows above the viewport; map the rest into the buffer.
            for (term_row, styled_graph) in styled_graphs
                .into_iter()
                .enumerate()
                .skip(viewport_top as usize)
            {
                let buf_row = term_row - viewport_top as usize;
                if let Some(row) = self.buf.get_mut(buf_row)
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

static MATRIX_ANIM_STATE: Mutex<Option<MatrixAnimState>> = Mutex::new(None);

#[derive(Debug, Clone)]
struct MatrixAnimState {
    last_update_time: std::time::Instant,
    // tendrils[i] is the y position of the falling "tendril" in column i, or None if there is no tendril currently in that column
    // y might be off the screen but we still want to show the tail of the tendril until it fully disappears
    tendrils: Vec<Option<(usize, HashMap<usize, usize>)>>, // (current max y of the tendril, offsets for each y in the tendril to determine which char to show)
}

impl MatrixAnimState {
    fn new() -> Self {
        MatrixAnimState {
            last_update_time: std::time::Instant::now(),
            tendrils: vec![],
        }
    }

    const TENDRIL_MAX_LEN: usize = 25;

    fn tendril_idx_to_graphemes(&self, idx: usize) -> Vec<StyledGrapheme<'static>> {
        // Some observations:
        // The leading char in the tendril should be bright, bold white
        // Characters should fade with age down the tendril, with the tail being very dim (e.g. dark green)
        // A mix of non-English chars looks good
        // Occasionally a character will change while the tendril is falling.

        static CHAR_SET: &[&str] = &[
            // For now, just use ASCII so that it renders on every terminal
            // "ｱ", "ｲ", "ｳ", "ｴ", "ｵ", "ｶ", "ｷ", "ｸ", "ｹ", "ｺ", "ｻ", "ｼ", "ｽ", "ｾ", "ｿ", "ﾀ", "ﾁ",
            // "ﾂ", "ﾃ", "ﾄ", "ﾅ", "ﾆ", "ﾇ", "ﾈ", "ﾉ", "ﾊ", "ﾋ", "ﾌ", "ﾍ", "ﾎ", "ﾏ", "ﾐ", "ﾑ", "ﾒ",
            // "ﾓ", "ﾔ", "ﾕ", "ﾖ", "ﾗ", "ﾘ", "ﾙ", "ﾚ", "ﾛ", "ﾜ", "ｦ",
            "ｱ", "ｲ", "ｳ", "ｴ", "ｵ", "ｶ", "ｷ", "ｸ", "ｹ", "ｺ", "ｻ", "ｼ", "ｽ", "ｾ", "ｿ", "ﾀ", "ﾁ",
            "ﾂ", "ﾃ", "ﾄ", "ﾅ", "ﾆ", "ﾇ", "ﾈ", "ﾉ", "ﾊ", "ﾋ", "ﾌ", "ﾍ", "ﾎ", "ﾏ", "ﾐ", "ﾑ", "ﾒ",
            "ﾓ", "ﾔ", "ﾕ", "ﾖ", "ﾗ", "ﾘ", "ﾙ", "ﾚ", "ﾛ", "ﾜ", "ｦ",
            // Some ASCII chars mixed in
            "@", "#", "$", "%", "&", "*", "+", "-", "=", "?", "A", "B", "C", "D", "E", "F", "G",
            "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X",
            "Y", "Z",
        ];

        let blank_graph = StyledGrapheme::new(" ", ratatui::style::Style::default());

        let mut rng = rand::rngs::StdRng::seed_from_u64(idx as u64);

        if let Some(Some((tendril_max_y, offsets))) = self.tendrils.get(idx) {
            let mut graphemes = vec![];
            for y in 0..=*tendril_max_y {
                let char_indx = (rng.next_u32() as usize) + offsets.get(&y).cloned().unwrap_or(0);

                if y <= tendril_max_y.saturating_sub(Self::TENDRIL_MAX_LEN) {
                    graphemes.push(blank_graph.clone());
                    continue;
                }
                // age_factor of 0 means the leading char, age_factor of 1 means the tail
                let age_factor =
                    tendril_max_y.saturating_sub(y) as f32 / Self::TENDRIL_MAX_LEN as f32;

                let symbol = CHAR_SET[char_indx % CHAR_SET.len()];
                let style = match age_factor {
                    0.0 => ratatui::style::Style::default()
                        .fg(ratatui::style::Color::White)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                    // _ if age_factor < 0.3 => ratatui::style::Style::default()
                    //     .fg(ratatui::style::Color::Green)
                    //     .add_modifier(ratatui::style::Modifier::BOLD),
                    // _ if age_factor < 0.6 => ratatui::style::Style::default().fg(ratatui::style::Color::Green),
                    // _ => ratatui::style::Style::default()
                    //     .fg(ratatui::style::Color::Green)
                    //     .add_modifier(ratatui::style::Modifier::DIM)
                    _ => {
                        let green_value = 255 - (age_factor.max(0.3) * 255.0) as u8;
                        ratatui::style::Style::default().fg(ratatui::style::Color::Rgb(
                            0,
                            green_value,
                            0,
                        ))
                    }
                };
                graphemes.push(StyledGrapheme::new(symbol, style));
            }

            graphemes
        } else {
            vec![]
        }
    }

    fn update(&mut self, now: std::time::Instant, num_cols: u16, num_rows: u16) {
        const NUM_ROWS_PER_SECOND: f32 = 12.0;
        const MS_PER_ROW: f32 = 1000.0 / NUM_ROWS_PER_SECOND;
        let steps_elapsed =
            (now.duration_since(self.last_update_time).as_millis() as f32 / MS_PER_ROW) as usize;

        self.tendrils.resize(num_cols as usize, None);

        if steps_elapsed == 0 {
            return;
        }
        self.last_update_time = now;

        for _ in 0..steps_elapsed {
            self.step(num_rows);
        }
    }

    fn step(&mut self, num_rows: u16) {
        // Move existing tendrils down
        for tendril in &mut self.tendrils {
            if let Some((y, offsets)) = tendril {
                *y += 1;
                // Randomly change an offset for some y in the tendril to create a flickering effect
                if rand::random::<f32>() < 0.9 {
                    let rand_row = rand::random::<u64>() as usize % num_rows as usize;
                    let rand_offset = rand::random::<u64>() as usize;
                    offsets.insert(rand_row, rand_offset);
                }
            }
        }

        // Remove tendrils that have moved off the bottom of the screen
        let max_possible_tendril_height = num_rows as usize + Self::TENDRIL_MAX_LEN;
        for tendril in &mut self.tendrils {
            if let Some((y, _)) = tendril
                && *y >= max_possible_tendril_height
            {
                *tendril = None;
            }
        }

        // Spawn new tendrils with some probability
        for tendril in &mut self.tendrils {
            let rand = rand::random::<f32>();
            if tendril.is_none() && rand < 0.02 {
                *tendril = Some((0, HashMap::new()));
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
