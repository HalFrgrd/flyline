use crate::stateful_sliding_window::StatefulSlidingWindow;
use rand::prelude::*;
use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, StyledGrapheme};
use std::collections::HashMap;
use std::sync::Mutex;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::palette::{ButtonState, Palette};
use crate::unicode_helpers::{Directions, PipeStyle, pipe};

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

    /// Consume `self` and return a new `TaggedSpan` whose style has the
    /// styling for the given non-normal [`ButtonState`] applied on top of the
    /// original style.
    pub fn with_button_state(self, state: ButtonState) -> Self {
        let new_style = Palette::apply_button_style(self.span.style, state);
        TaggedSpan {
            span: self.span.style(new_style),
            tag: self.tag,
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

impl<'a> From<TaggedSpan<'a>> for TaggedLine<'a> {
    fn from(span: TaggedSpan<'a>) -> Self {
        TaggedLine { spans: vec![span] }
    }
}

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

/// Identifies which clipboard slot a [`Tag::Clipboard`] cell belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipboardTypes {
    TutorialClickExample,
    TutorialRP1,
    TutorialMouseMode,
    TutorialRecommendedSettings,
    TutorialCursor0,
    TutorialCursor1,
    TutorialCursor2,
    TutorialCursor3,
    TutorialCursor4,
    TutorialCursor5,
    TutorialCursor6,
    TutorialFineGrainDeletion,
    TutorialSetColor1,
    TutorialSetColor2,
    TutorialSetColor3,
    TutorialSetColor4,
    TutorialSetColor5,
    TutorialRunHelp,
    TutorialAutoClose,
    TutorialAgentMode,
    TutorialGrep,
    TutorialBashCompletion,
    TutorialKeybindingsList,
    TutorialKeybindingsBind1,
    TutorialKeybindingsBind2,
    TutorialKeybindingsBind3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Blank,
    Normal,
    Ps1Prompt,
    Ps1PromptCwdWidget(usize),
    Ps1PromptDynamicTime,
    Ps1PromptAnimation,
    PromptCopyBufferWidget,
    Ps2Prompt,
    Command(usize),
    TabSuggestion,
    Suggestion(usize),
    HistorySuggestion,
    FuzzySearch,
    HistoryResult(usize),
    Tooltip,
    AiResult(usize),
    TutorialPrev,
    TutorialNext,
    Tutorial,
    Clipboard(ClipboardTypes),
    MultiWidthContinuation,
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
    /// Clipboard content for each [`ClipboardTypes`] slot, populated via [`Contents::setup_clipboard`].
    pub clipboards: HashMap<ClipboardTypes, String>,
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
            clipboards: HashMap::new(),
        }
    }

    /// Register clipboard content for the given `clipboard_type`.
    /// The `text` is appended to any content already stored for that type.
    pub fn setup_clipboard(&mut self, clipboard_type: ClipboardTypes, text: String) {
        self.clipboards
            .entry(clipboard_type)
            .or_default()
            .push_str(&text);
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

    pub fn move_to_next_insertion_point(
        &mut self,
        graph: &StyledGrapheme,
        overwrite: bool,
        area: Option<Rect>,
    ) -> bool {
        let graph_w = graph.symbol.width() as u16;
        let (left, right, bottom) = if let Some(area) = area {
            (area.left(), area.right(), area.bottom())
        } else {
            (0, self.width, u16::MAX)
        };

        const MAX_ITERATIONS: usize = 1000; // safety to prevent infinite loops
        let mut iterations = 0;
        loop {
            if iterations >= MAX_ITERATIONS {
                return false;
            }
            iterations += 1;

            if self.cursor_pos.row >= self.buf.len() as u16 {
                if self.cursor_pos.row >= bottom {
                    return false;
                }
                self.increase_buf_single_row();
            }

            if self.cursor_pos.col < left {
                self.cursor_pos.col = left;
            }

            if self.cursor_pos.col + graph_w > right {
                if self.cursor_pos.row + 1 >= bottom {
                    return false;
                }
                self.cursor_pos.row += 1;
                self.cursor_pos.col = left;
                continue;
            }

            if !overwrite {
                let row = &self.buf[self.cursor_pos.row as usize];
                let cells =
                    &row[self.cursor_pos.col as usize..(self.cursor_pos.col + graph_w) as usize];
                if cells.iter().all(|cell| cell.tag == Tag::Blank) {
                    return true;
                } else {
                    self.cursor_pos.col += 1;
                    continue;
                }
            } else {
                return true;
            }
        }
    }

    /// Write a single tagged span at the current cursor position.
    /// Will automatically wrap to the next line if necessary.
    fn write_span_internal(
        &mut self,
        tagged_span: &TaggedSpan,
        overwrite: bool,
        area: Option<Rect>,
    ) -> bool {
        if let SpanTag::Constant(Tag::Clipboard(cb_type)) = &tagged_span.tag {
            self.setup_clipboard(*cb_type, tagged_span.span.content.to_string());
        }

        let graphemes = tagged_span.span.styled_graphemes(tagged_span.span.style);

        for (i, graph) in graphemes.enumerate() {
            let graph_w = graph.symbol.width() as u16;
            if graph_w == 0 {
                continue;
            }

            if !self.move_to_next_insertion_point(&graph, overwrite, area) {
                return false;
            }

            let next_graph_x = self.cursor_pos.col + graph_w;
            let tag = tagged_span.tag.get(i);
            self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                .update(&graph, tag);
            self.cursor_pos.col += 1;
            // Reset following cells if multi-width (they would be hidden by the grapheme),
            while self.cursor_pos.col < next_graph_x {
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize]
                    .cell
                    .reset();
                self.buf[self.cursor_pos.row as usize][self.cursor_pos.col as usize].tag =
                    Tag::MultiWidthContinuation;
                self.cursor_pos.col += 1;
            }
        }
        true
    }

    /// Write a tagged span at the current cursor position, skipping cells that are already filled.
    pub fn write_tagged_span_dont_overwrite(&mut self, tagged_span: &TaggedSpan) -> bool {
        self.write_span_internal(tagged_span, false, None)
    }

    /// Write a tagged span at the current cursor position, overwriting any existing content.
    pub fn write_tagged_span(&mut self, tagged_span: &TaggedSpan) -> bool {
        self.write_span_internal(tagged_span, true, None)
    }

    /// Write a tagged span at the current cursor position, overwriting any existing content,
    /// but only within the given `area`.
    pub fn write_tagged_span_area(&mut self, tagged_span: &TaggedSpan, area: Rect) -> bool {
        self.write_span_internal(tagged_span, true, Some(area))
    }

    /// Write a tagged line at the current cursor position.
    /// If `insert_new_line` is true, moves to the next line after writing.
    pub fn write_tagged_line(&mut self, line: &TaggedLine, insert_new_line: bool) -> bool {
        for tagged_span in &line.spans {
            if !self.write_tagged_span(tagged_span) {
                return false;
            }
        }
        if insert_new_line {
            self.newline();
        }
        true
    }

    /// Write a tagged line at the current cursor position, but only within the given `area`.
    pub fn write_tagged_line_area(&mut self, line: &TaggedLine, area: Rect) -> bool {
        for tagged_span in &line.spans {
            if !self.write_span_internal(tagged_span, true, Some(area)) {
                return false;
            }
        }
        true
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

    /// Move the cursor to a specific row and column.
    /// Row is clamped to the last buffer row; col is clamped to `self.width`.
    pub fn move_cursor_to(&mut self, row: u16, col: u16) {
        self.cursor_pos.row = row.min(self.buf.len().saturating_sub(1) as u16);
        self.cursor_pos.col = col.min(self.width);
    }

    /// Clears any multi-width grapheme that spans across `col` on the given `row`,
    /// and writes the specified `symbol` with `style` and `tag` at the start of that grapheme or at `col`.
    pub fn overwrite_with_char(
        &mut self,
        row: usize,
        col: usize,
        symbol: &str,
        style: ratatui::style::Style,
        tag: Tag,
    ) {
        if row >= self.buf.len() {
            return;
        }
        let width = self.width as usize;
        if col >= width {
            return;
        }

        // Find the start of the multi-width grapheme if we are on a continuation cell
        let mut start_col = col;
        if self.buf[row][start_col].tag == Tag::MultiWidthContinuation {
            while start_col > 0 && self.buf[row][start_col].tag == Tag::MultiWidthContinuation {
                start_col -= 1;
            }
        }

        // Find the end of the multi-width grapheme
        let mut end_col = start_col;
        while end_col + 1 < width && self.buf[row][end_col + 1].tag == Tag::MultiWidthContinuation {
            end_col += 1;
        }

        // Clear all cells spanning this grapheme
        for c in start_col..=end_col {
            self.buf[row][c].cell.reset();
            self.buf[row][c].tag = Tag::Blank;
        }

        // Set cursor to start_col and write the character using write_span_internal
        let old_cursor = self.cursor_pos;
        self.cursor_pos = Coord::new(row as u16, start_col as u16);
        let span = TaggedSpan::new(Span::styled(symbol.to_string(), style), tag);
        self.write_span_internal(&span, true, None);
        self.cursor_pos = old_cursor;
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
            StatefulSlidingWindow::new(0, term_height as usize, self.height() as usize, None);
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

    #[allow(dead_code)]
    pub fn write_buffer(&mut self, buffer: &ratatui::buffer::Buffer, tag: Tag) {
        for pos in buffer.area().positions() {
            for _ in self.buf.len()..=pos.y as usize {
                self.increase_buf_single_row();
            }
            if let Some(cell) = buffer.cell(pos) {
                if let Some(row) = self.buf.get_mut(pos.y as usize)
                    && let Some(tagged_cell) = row.get_mut(pos.x as usize)
                {
                    tagged_cell.cell = cell.clone();
                    tagged_cell.tag = tag;

                    self.cursor_pos = Coord::new(pos.y, pos.x);
                }
            }
        }
    }

    fn get_char(x: u16, y: u16, area: Rect, is_selected: bool) -> char {
        let char = match (x, y) {
            (x, y) if x == area.left() && y == area.top() => '╭',
            (x, y) if x == area.right() - 1 && y == area.top() => '╮',
            (x, y) if x == area.left() && y == area.bottom() - 1 => '╰',
            (x, y) if x == area.right() - 1 && y == area.bottom() - 1 => '╯',
            (_x, y) if y == area.bottom() - 1 => '─',
            (_x, y) if y == area.top() => '─',
            (x, _y) if x == area.left() => '│',
            (x, _y) if x == area.right() - 1 => '│',
            _ => ' ',
        };

        if !is_selected {
            return char;
        }

        match char {
            '╭' => '╔',
            '╮' => '╗',
            '╰' => '╚',
            '╯' => '╝',
            '─' => '═', // if y == area.top() { '▂' } else { '🬂' },
            '│' => '║', // if x == area.left() { '🮇' } else { '🯏' },
            // ' ' => unicode_helpers::FULL_BLOCK, // 🮖 █
            _ => char,
        }
        // match char {
        //     '╭' => '▗',
        //     '╮' => '▖',
        //     '╰' => '▝',
        //     '╯' => '▘',
        //     '─' =>  if y == area.top() { '▄' } else { '▀' },
        //     '│' =>  if x == area.left() { '▐' } else { '▌' },
        //     ' ' => '🮖', // 🮖 █
        //     _ => char
        // }
        // match char {
        //     '╭' => '▗',
        //     '╮' => '▖',
        //     '╰' => '🯬',
        //     '╯' => '▘',
        //     '─' =>  if y == area.top() { '🮏' } else { '🮎' },
        //     '│' =>  if x == area.left() { '🮍' } else { '🮌' },
        //     ' ' => '🮐', // 🮖 █
        //     _ => char
        // }
        // match char {
        //     '╭' => '🬆',
        //     '╮' => '🬊',
        //     '╰' => '🬱',
        //     '╯' => '🬵',
        //     '─' =>  if y == area.top() { '🮏' } else { '🮎' },
        //     '│' =>  if x == area.left() { '🮍' } else { '🮌' },
        //     ' ' => '🮐', // 🮖 █
        //     _ => char
        // }
        // match char {                    // These ones are good but generally people dont have the font to render them
        //     // '╭' => '🬆',
        //     // '╮' => '🬊',
        //     // '╰' => '🬱',
        //     // '╯' => '🬵',
        //     '─' => {
        //         if y == area.top() {
        //             '🮏'
        //         } else {
        //             '🮎'
        //         }
        //     }
        //     '│' => {
        //         if x == area.left() {
        //             '🮍'
        //         } else {
        //             '🮌'
        //         }
        //     }
        //     ' ' => '🮐', // 🮖 █
        //     _ => char,
        // }
    }

    pub fn render_block(&mut self, area: Rect, label: &str, tag: Tag, state: ButtonState) {
        for _ in self.buf.len()..area.bottom() as usize {
            self.increase_buf_single_row();
        }

        let is_selected = !matches!(state, ButtonState::Normal);

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(row) = self.buf.get_mut(y as usize)
                    && let Some(tagged_cell) = row.get_mut(x as usize)
                {
                    let char = Self::get_char(x, y, area, is_selected);

                    let style = if matches!(state, ButtonState::Normal) {
                        ratatui::style::Style::default()
                    } else {
                        Palette::apply_button_style(ratatui::style::Style::default(), state)
                    };

                    tagged_cell
                        .cell
                        .set_symbol(&char.to_string())
                        .set_style(style);
                    tagged_cell.tag = tag;
                }
            }

            // write label in center of block:
            let label_style = if matches!(state, ButtonState::Normal) {
                ratatui::style::Style::default()
            } else {
                Palette::apply_button_style(ratatui::style::Style::default(), state)
            };
            let label_span = Span::styled(label.to_string(), label_style);
            let label_width = label_span.width() as u16;
            if label_width < area.width {
                let label_x = area.left() + (area.width - label_width) / 2;
                let label_y = area.top() + ((area.height - 1) / 2);
                self.set_cursor_col(label_x);
                self.cursor_pos.row = label_y;
                self.write_tagged_span(&TaggedSpan::new(label_span, tag));
            }
        }
    }

    pub fn render_border(
        &mut self,
        area: Rect,
        tag: Tag,
        style: ratatui::style::Style,
        is_selected: bool,
        connector_from: Option<Coord>,
    ) {
        if area.width < 2 || area.height < 2 {
            return;
        }

        let saved_cursor_pos = self.cursor_pos;

        for _ in self.buf.len()..area.bottom() as usize {
            self.increase_buf_single_row();
        }

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let is_border = y == area.top()
                    || y == area.bottom() - 1
                    || x == area.left()
                    || x == area.right() - 1;
                if !is_border {
                    continue;
                }

                if let Some(row) = self.buf.get_mut(y as usize)
                    && let Some(tagged_cell) = row.get_mut(x as usize)
                {
                    let ch = Self::get_char(x, y, area, is_selected);
                    tagged_cell.cell.reset();
                    tagged_cell
                        .cell
                        .set_symbol(&ch.to_string())
                        .set_style(style);
                    tagged_cell.tag = tag;
                }
            }
        }

        if let Some(cursor_pos) = connector_from
            && cursor_pos.row < area.y
        {
            let box_left = area.x;
            let box_right = area.right().saturating_sub(1);
            let connector_col = if cursor_pos.col >= box_left && cursor_pos.col <= box_right {
                cursor_pos.col
            } else {
                (box_left + 1).min(box_right.saturating_sub(1))
            };

            let vertical =
                pipe(Directions::TOP | Directions::BOTTOM, PipeStyle::Single).unwrap_or(' ');
            let box_join_dirs = if connector_col == box_left {
                Directions::TOP | Directions::RIGHT | Directions::BOTTOM
            } else if connector_col == box_right {
                Directions::TOP | Directions::LEFT | Directions::BOTTOM
            } else {
                Directions::TOP | Directions::LEFT | Directions::RIGHT
            };
            let box_join = pipe(box_join_dirs, PipeStyle::Single).unwrap_or(' ');

            for row in cursor_pos.row.saturating_add(1)..area.y {
                self.move_cursor_to(row, connector_col);
                self.write_tagged_span(&TaggedSpan::new(
                    Span::styled(vertical.to_string(), style),
                    tag,
                ));
            }

            self.move_cursor_to(area.y, connector_col);
            self.write_tagged_span(&TaggedSpan::new(
                Span::styled(box_join.to_string(), style),
                tag,
            ));
        }

        self.cursor_pos = saved_cursor_pos;
    }

    pub fn tag_rect(&mut self, area: Rect, tag: Tag) {
        for _ in self.buf.len()..area.bottom() as usize {
            self.increase_buf_single_row();
        }

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(row) = self.buf.get_mut(y as usize)
                    && let Some(tagged_cell) = row.get_mut(x as usize)
                {
                    tagged_cell.tag = tag;
                }
            }
        }
    }

    pub fn delete_rows(&mut self, start_row: u16, end_row: u16) {
        // safely delete rows from start_row to end_row (exclusive), shifting up the rows below
        if start_row >= end_row || start_row >= self.height() {
            return;
        }
        let end_row = end_row.min(self.height());
        self.buf.drain(start_row as usize..end_row as usize);
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

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    #[test]
    fn test_basic_write() {
        let mut contents = Contents::new(10);
        let span = TaggedSpan::new(
            Span::styled("hello", Style::default().fg(Color::Red)),
            Tag::Normal,
        );
        contents.write_tagged_span(&span);

        assert_eq!(contents.cursor_position(), Coord::new(0, 5));
        assert_eq!(contents.buf[0][0].tag, Tag::Normal);
        assert_eq!(contents.buf[0][0].cell.symbol(), "h");
        assert_eq!(contents.buf[0][0].cell.style().fg, Some(Color::Red));
    }

    #[test]
    fn test_wrapping() {
        let mut contents = Contents::new(5);
        let span = TaggedSpan::new(Span::raw("hello world"), Tag::Normal);
        contents.write_tagged_span(&span);

        assert_eq!(contents.height(), 3);
        assert_eq!(contents.cursor_position(), Coord::new(2, 1));

        let row0: String = contents.buf[0].iter().map(|c| c.cell.symbol()).collect();
        let row1: String = contents.buf[1].iter().map(|c| c.cell.symbol()).collect();
        let row2: String = contents.buf[2].iter().map(|c| c.cell.symbol()).collect();

        assert_eq!(row0, "hello");
        assert_eq!(row1, " worl");
        assert_eq!(row2, "d    ");
    }

    #[test]
    fn test_dont_overwrite() {
        let mut contents = Contents::new(10);
        contents.write_tagged_span(&TaggedSpan::new(Span::raw("hello"), Tag::Normal));
        contents.move_cursor_to(0, 0);

        // Should skip "hello" and write "world" after it
        contents.write_tagged_span_dont_overwrite(&TaggedSpan::new(
            Span::raw("world"),
            Tag::Command(0),
        ));

        assert_eq!(contents.buf[0][0].tag, Tag::Normal);
        assert_eq!(contents.buf[0][5].tag, Tag::Command(0));
    }

    #[test]
    fn test_multi_width_grapheme() {
        let mut contents = Contents::new(5);
        // "🌟" is width 2
        let span = TaggedSpan::new(Span::raw("🌟🌟🌟"), Tag::Normal);
        contents.write_tagged_span(&span);

        // Row 0: "🌟🌟" (width 4), then "🌟" (width 2) doesn't fit (4+2 > 5)
        // Row 1: "🌟"
        assert_eq!(contents.cursor_position(), Coord::new(1, 2));

        assert_eq!(contents.buf[0][0].cell.symbol(), "🌟");
        assert_eq!(contents.buf[0][1].cell.symbol(), " "); // second cell of 🌟 (after reset())
        assert_eq!(contents.buf[0][2].cell.symbol(), "🌟");
        assert_eq!(contents.buf[0][3].cell.symbol(), " ");
        assert_eq!(contents.buf[0][4].cell.symbol(), " "); // empty since next 🌟 didn't fit

        assert_eq!(contents.buf[1][0].cell.symbol(), "🌟");
        assert_eq!(contents.buf[1][1].cell.symbol(), " ");
    }

    #[test]
    fn test_write_area_wrapping() {
        let mut contents = Contents::new(20);
        let area = Rect {
            x: 5,
            y: 0,
            width: 10,
            height: 2,
        };
        let span = TaggedSpan::new(
            Span::raw("this is a long span that should wrap"),
            Tag::Normal,
        );

        contents.move_cursor_to(0, 5);
        let completed = contents.write_tagged_span_area(&span, area);

        assert!(!completed); // Should be truncated
        assert_eq!(contents.height(), 2);

        // Row 0: "this is a " (10 chars at col 5-14)
        // Row 1: "long span " (10 chars at col 5-14)
        let row0: String = contents.buf[0].iter().map(|c| c.cell.symbol()).collect();
        let row1: String = contents.buf[1].iter().map(|c| c.cell.symbol()).collect();

        assert_eq!(&row0[5..15], "this is a ");
        assert_eq!(&row1[5..15], "long span ");
        assert_eq!(contents.cursor_position().row, 1);
    }

    #[test]
    fn test_write_area_truncation() {
        let mut contents = Contents::new(20);
        let area = Rect {
            x: 0,
            y: 0,
            width: 5,
            height: 1,
        };
        let span = TaggedSpan::new(Span::raw("hello world"), Tag::Normal);

        let completed = contents.write_tagged_span_area(&span, area);
        assert!(!completed);
        assert_eq!(contents.cursor_position(), Coord::new(0, 5));

        let row0: String = contents.buf[0].iter().map(|c| c.cell.symbol()).collect();
        assert_eq!(&row0[..5], "hello");
        assert_eq!(&row0[5..6], " "); // should be blank
    }

    #[test]
    fn test_write_area_single_row_wrap() {
        let mut contents = Contents::new(20);
        let area = Rect {
            x: 5,
            y: 0,
            width: 5,
            height: 1,
        };
        let span = TaggedSpan::new(Span::raw("1234567"), Tag::Normal);

        contents.move_cursor_to(0, 5);
        let completed = contents.write_tagged_span_area(&span, area);

        assert!(!completed); // Should truncate at col 10
        assert_eq!(contents.cursor_position(), Coord::new(0, 10));

        let row0: String = contents.buf[0].iter().map(|c| c.cell.symbol()).collect();
        assert_eq!(&row0[5..10], "12345");
        assert_eq!(&row0[10..11], " ");
    }

    #[test]
    fn test_overwrite_with_char() {
        use ratatui::style::Style;
        let mut contents = Contents::new(10);

        let span1 = TaggedSpan::new(Span::raw("hello"), Tag::Normal);
        contents.write_tagged_span(&span1);

        let span2 = TaggedSpan::new(Span::raw("🌟"), Tag::Normal);
        contents.write_tagged_span(&span2);

        // Check buffer state
        assert_eq!(contents.buf[0][0].cell.symbol(), "h");
        assert_eq!(contents.buf[0][5].cell.symbol(), "🌟");
        assert_eq!(contents.buf[0][5].tag, Tag::Normal);
        assert_eq!(contents.buf[0][6].cell.symbol(), " ");
        assert_eq!(contents.buf[0][6].tag, Tag::MultiWidthContinuation);

        // Overwrite a normal character
        contents.overwrite_with_char(0, 4, "…", Style::default(), Tag::Normal);
        assert_eq!(contents.buf[0][4].cell.symbol(), "…");

        // Overwrite the multi-width continuation cell
        contents.overwrite_with_char(0, 6, "…", Style::default(), Tag::Normal);
        assert_eq!(contents.buf[0][5].cell.symbol(), "…");
        assert_eq!(contents.buf[0][6].cell.symbol(), " ");
        assert_eq!(contents.buf[0][6].tag, Tag::Blank);
    }
}
