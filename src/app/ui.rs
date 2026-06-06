use super::*;
use crate::content_builder::Coord;
use crate::content_utils::{
    gaussian_wave_animated, split_line_to_terminal_rows, ts_to_timeago_string_5chars,
};
use crate::tutorial;
use ratatui::prelude::*;

pub(crate) struct DrawnContent {
    pub(crate) contents: Contents,
    /// The terminal row (absolute) where the content starts. Used for translating mouse coordinates.
    pub(crate) viewport_start: u16,
    pub(crate) content_visible_row_range: std::ops::Range<u16>,
}
impl DrawnContent {
    pub(crate) fn content_row_to_term_em_row(&self, content_row: u16) -> u16 {
        content_row.saturating_sub(self.content_visible_row_range.start) + self.viewport_start
    }

    pub(crate) fn term_em_row_to_content_row(&self, term_em_row: u16) -> isize {
        term_em_row as isize - self.viewport_start as isize
            + self.content_visible_row_range.start as isize
    }

    pub fn term_em_cursor_pos(&self) -> Option<Position> {
        self.contents.term_cursor_pos.map(|cursor_pos| Position {
            x: cursor_pos.col,
            y: self.content_row_to_term_em_row(cursor_pos.row),
        })
    }

    pub fn term_em_prompt_start(&self) -> Option<Position> {
        self.contents.prompt_start.map(|prompt_start| Position {
            x: prompt_start.col,
            y: self.content_row_to_term_em_row(prompt_start.row),
        })
    }

    pub fn term_em_prompt_end(&self) -> Option<Position> {
        self.contents.prompt_end.map(|prompt_end| Position {
            x: prompt_end.col,
            y: self.content_row_to_term_em_row(prompt_end.row),
        })
    }

    pub fn get_tagged_cell(&self, term_em_x: u16, term_em_y: u16) -> Option<(Tag, bool)> {
        let content_row = self.term_em_row_to_content_row(term_em_y);
        if content_row < 0 {
            return None;
        }

        let content_buf_row = self.contents.buf.get(content_row as usize)?;

        let direct_contact = content_buf_row.get(term_em_x as usize);

        if direct_contact.is_some_and(|cell| {
            matches!(
                cell.tag,
                Tag::Command(_)
                    | Tag::Suggestion(_)
                    | Tag::HistoryResult(_)
                    | Tag::AiResult(_)
                    | Tag::TutorialPrev
                    | Tag::TutorialNext
                    | Tag::PromptCopyBufferWidget
                    | Tag::Clipboard(_)
                    | Tag::Ps1PromptCwdWidget(_)
                    | Tag::TabCompletionScrollBar { .. }
            )
        }) {
            return direct_contact.map(|cell| (cell.tag, true));
        }

        if let Some(hit) = content_buf_row
            .iter()
            .enumerate()
            .rev()
            .find(|(col_idx, tagged_cell)| {
                *col_idx <= term_em_x as usize
                    && matches!(
                        tagged_cell.tag,
                        Tag::Command(_) | Tag::TabCompletionScrollBar { .. }
                    )
            })
            .map(|(_, cell)| (cell.tag, false))
        {
            return Some(hit);
        }

        // Mirror of the leftward search above: when the click is below the
        // command buffer, walk upward row-by-row and return the closest
        // `Tag::Command` cell. Within each row we pick the rightmost command
        // cell so that for a multi-line buffer we land on the end of the
        // last preceding line.
        for row_idx in (0..content_row as usize).rev() {
            let row = match self.contents.buf.get(row_idx) {
                Some(row) => row,
                None => continue,
            };
            if let Some(cell) = row
                .iter()
                .rev()
                .find(|tagged_cell| matches!(tagged_cell.tag, Tag::Command(_)))
            {
                return Some((cell.tag, false));
            }
        }

        None
    }
}

impl<'a> App<'a> {
    fn render_history_entry(
        content: &mut Contents,
        formatted_entry: &HistoryEntryFormatted,
        entries: &[HistoryEntry],
        entry_idx: usize,
        fuzzy_search_index: usize,
        num_digits_for_index: usize,
        num_digits_for_score: usize,
        header_prefix_width: usize,
        available_cols: u16,
        palette: &Palette,
    ) {
        let is_selected = fuzzy_search_index == entry_idx;
        let tag = Tag::HistoryResult(entry_idx);

        let entry = &entries[formatted_entry.entry_index];
        let timeago_str = entry
            .timestamp
            .map(ts_to_timeago_string_5chars)
            .unwrap_or_else(|| "     ".to_string());

        let indicator_span = || {
            if is_selected {
                Span::styled(
                    "▐",
                    palette
                        .matching_char()
                        .remove_modifier(Modifier::UNDERLINED),
                )
            } else {
                Span::styled(" ", palette.secondary_text())
            }
        };

        let formatted_text = formatted_entry.command_spans(entries, palette);
        let max_display_rows = if is_selected { 4 } else { 1 };

        let ellipsis_style = if is_selected {
            Palette::convert_to_highlighted(palette.secondary_text())
        } else {
            palette.secondary_text()
        };

        let start_row = content.cursor_position().row as usize;
        let mut current_display_row = 0;
        let mut truncated = false;
        let mut last_content_end_col = header_prefix_width;

        for (logical_idx, line) in formatted_text.iter().enumerate() {
            if current_display_row >= max_display_rows {
                truncated = true;
                break;
            }

            content.newline();
            let row_y = content.cursor_position().row as usize;

            content.set_cursor_col(header_prefix_width as u16);

            let remaining_rows = max_display_rows - current_display_row;
            let rect = Rect {
                x: header_prefix_width as u16,
                y: row_y as u16,
                width: available_cols,
                height: remaining_rows as u16,
            };

            for span in &line.spans {
                let mut styled_span = span.clone();
                if is_selected {
                    styled_span.style = Palette::convert_to_highlighted(styled_span.style);
                }
                let tagged_span = TaggedSpan::new(styled_span, tag);
                if !content.write_tagged_span_area(&tagged_span, rect) {
                    truncated = true;
                    break;
                }
            }

            last_content_end_col = content.cursor_position().col as usize;

            // Fill the rest of the logical line's final row with empty space
            content.fill_line(tag);

            let end_row = content.cursor_position().row as usize;
            let logical_line_rows = end_row - row_y + 1;
            current_display_row += logical_line_rows;

            // Write prefixes for this logical line's rows (row_y..=end_row)
            for r in row_y..=end_row {
                let is_first_row_of_logical = r == row_y;

                content.move_cursor_to(r as u16, 0);

                if r == start_row + 1 {
                    let prefix_spans = vec![
                        Span::styled(
                            format!("{:>num_digits_for_index$} ", entry.index + 1),
                            palette.secondary_text(),
                        ),
                        Span::styled(
                            format!("{:>num_digits_for_score$} ", formatted_entry.score),
                            palette.secondary_text(),
                        ),
                        Span::styled(timeago_str.clone(), palette.secondary_text()),
                        indicator_span(),
                    ];
                    for prefix_span in prefix_spans {
                        content.write_tagged_span(&TaggedSpan::new(prefix_span, tag));
                    }
                } else {
                    let indent_prefix = if is_first_row_of_logical {
                        let line_num_str = format!("{}/{}", logical_idx + 1, formatted_text.len());
                        format!("{:>width$}", line_num_str, width = header_prefix_width - 1)
                    } else {
                        " ".repeat(header_prefix_width - 1)
                    };

                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(indent_prefix, palette.secondary_text()),
                        tag,
                    ));
                    content.write_tagged_span(&TaggedSpan::new(indicator_span(), tag));
                }
            }

            // Restore cursor to the end of the written content to allow newline() to proceed correctly
            content.move_cursor_to(end_row as u16, content.width);

            if truncated {
                break;
            }
        }

        let end_row = content.cursor_position().row as usize;

        // Retroactive Ellipsis Pass
        if truncated {
            let last_col = last_content_end_col.min((content.width as usize).saturating_sub(1));
            content.overwrite_with_char(end_row, last_col, "…", ellipsis_style, tag);
        }

        // Restore cursor position to the end of the written area
        content.move_cursor_to(end_row as u16, content.width);
    }
    pub(crate) fn create_content(
        &mut self,
        width: u16,
        viewport_top: u16,
        terminal_height: u16,
    ) -> Contents {
        // Basically build the entire frame in a Content first
        // Then figure out how to fit that into the actual frame area
        let mut content = Contents::new(width);

        let now = std::time::Instant::now();

        // When terminal log streaming is enabled, show the last 20 log lines at
        // the top of the content before anything else.
        if crate::logging::is_terminal_streaming() {
            let log_lines = crate::logging::last_n_logs(20);
            for line_text in log_lines {
                let tagged_line = TaggedLine::from(vec![TaggedSpan::new(
                    ratatui::text::Span::raw(line_text),
                    Tag::Normal,
                )]);
                content.write_tagged_line(&tagged_line, true);
            }
        }

        // Render tutorial text above the prompt when a tutorial step is active.
        if self.mode.is_running() {
            if self.settings.tutorial_step == tutorial::TutorialStep::Welcome {
                // Welcome step: draw the large block-art logo, then overlay the
                // animated action prompt in the lower-right of the logo.
                let logo_lines = crate::tutorial::generate_welcome_logo_lines(width);
                for line in logo_lines {
                    content.write_tagged_line(&TaggedLine::from_line(line, Tag::Tutorial), true);
                }

                let second_to_last = content.height().saturating_sub(3);
                let (offset, action_line) =
                    crate::tutorial::generate_welcome_action_line(now, width);
                content.move_cursor_to(second_to_last, offset);
                content
                    .write_tagged_line(&TaggedLine::from_line(action_line, Tag::Tutorial), false);

                content.move_to_final_line();
                content.newline();
            } else if let Some(tutorial_tagged_lines) = crate::tutorial::generate_tutorial_text(
                self.settings,
                self.settings.tutorial_step,
                &self.settings.colour_palette,
            ) {
                const BUTTON_HEIGHT: u16 = 30;

                let layout = Layout::horizontal([
                    Constraint::Max(10),
                    Constraint::Min(10),
                    Constraint::Max(10),
                ]);

                let tutorial_start_row = 1;
                content.newline();

                let [mut prev_block, text_block, mut next_block] = Rect {
                    x: 0,
                    y: 0,
                    width,
                    height: BUTTON_HEIGHT,
                }
                .layout(&layout);

                // Draw prev and next buttons first.
                let prev_state = self.button_state_for(Tag::TutorialPrev);
                let next_state = self.button_state_for(Tag::TutorialNext);
                let draw_prev_block = |block, content: &mut Contents| {
                    content.render_block(block, "prev", Tag::TutorialPrev, prev_state);
                    content.tag_rect(
                        block.outer(Margin {
                            horizontal: 1,
                            vertical: 0,
                        }),
                        Tag::TutorialPrev,
                    );
                };

                draw_prev_block(prev_block, &mut content);

                let draw_next_block = |block, content: &mut Contents| {
                    content.render_block(block, "next", Tag::TutorialNext, next_state);
                    content.tag_rect(
                        block.outer(Margin {
                            horizontal: 1,
                            vertical: 0,
                        }),
                        Tag::TutorialNext,
                    );
                };
                draw_next_block(next_block, &mut content);

                // Move cursor to the start of the text area and write tutorial
                // lines using overwrite=false so the text sits between the buttons.
                content.move_cursor_to(tutorial_start_row, text_block.x);

                let mut text_end_row = tutorial_start_row;
                for tagged_line in &tutorial_tagged_lines {
                    for tagged_span in &tagged_line.spans {
                        // If the mouse is hovering over a clipboard-tagged span,
                        // apply the appropriate button styling (highlight while
                        // hovered, plus bold while the left button is held).
                        let span_state = if let SpanTag::Constant(tag) = &tagged_span.tag {
                            self.button_state_for(*tag)
                        } else {
                            ButtonState::Normal
                        };
                        if matches!(span_state, ButtonState::Normal) {
                            content.write_tagged_span_dont_overwrite(tagged_span);
                        } else {
                            content.write_tagged_span_dont_overwrite(
                                &tagged_span.clone().with_button_state(span_state),
                            );
                        }
                    }
                    text_end_row = content.cursor_position().row;
                    content.newline();
                }

                if !self.mouse_state.is_enabled() {
                    let red = Style::default().fg(Color::Red).slow_blink();
                    let escape_hint = TaggedLine::from(vec![TaggedSpan::new(
                        Span::styled("Press Escape  to re-enable mouse mode.", red),
                        Tag::Tutorial,
                    )]);
                    for tagged_span in &escape_hint.spans {
                        content.write_tagged_span_dont_overwrite(tagged_span);
                    }
                    text_end_row = content.cursor_position().row;
                    content.newline();
                }

                let drain_start = text_end_row + 2;
                content.delete_rows(drain_start, tutorial_start_row + BUTTON_HEIGHT);

                let final_height = content.height().max(7);

                prev_block.height = final_height;
                next_block.height = final_height;

                draw_prev_block(prev_block, &mut content);
                draw_next_block(next_block, &mut content);

                content.move_to_final_line();
                content.newline();
            }
        }

        if self.mode.is_running()
            && self.settings.key_debug
            && let Some(last_key) = &self.last_key
        {
            content.write_tagged_line(
                &TaggedLine::from_line(
                    Line::from(format!(
                        "key: {}  context: {}  action: {}",
                        last_key.display, last_key.context, last_key.action
                    ))
                    .style(
                        self.settings
                            .colour_palette
                            .secondary_text()
                            .add_modifier(Modifier::BOLD),
                    ),
                    Tag::Normal,
                ),
                true,
            );
        }

        content.prompt_start = Some(content.cursor_position());

        let (mut lprompt, rprompt, fill_span) = self
            .prompt_manager
            .get_ps1_lines(self.settings.show_animations, self.mouse_state.is_enabled());

        let copy_buffer_state = self.button_state_for(Tag::PromptCopyBufferWidget);
        let copy_buffer_active = !matches!(copy_buffer_state, ButtonState::Normal);
        if copy_buffer_active {
            for line in &mut lprompt {
                for span in &mut line.spans {
                    if span.tag == SpanTag::Constant(Tag::PromptCopyBufferWidget) {
                        span.span.style =
                            Palette::apply_button_style(span.span.style, copy_buffer_state);
                    }
                }
            }
        }

        let mut rprompt = rprompt;
        if copy_buffer_active {
            for line in &mut rprompt {
                for span in &mut line.spans {
                    if span.tag == SpanTag::Constant(Tag::PromptCopyBufferWidget) {
                        span.span.style =
                            Palette::apply_button_style(span.span.style, copy_buffer_state);
                    }
                }
            }
        }

        let mut fill_span = fill_span;
        if copy_buffer_active {
            for span in &mut fill_span.spans {
                if span.tag == SpanTag::Constant(Tag::PromptCopyBufferWidget) {
                    span.span.style =
                        Palette::apply_button_style(span.span.style, copy_buffer_state);
                }
            }
        }

        // When in PromptCwdEdit mode, highlight the selected CWD path segment.
        if self.mode.is_running()
            && let ContentMode::PromptDirSelect(cwd_index) = self.content_mode
        {
            for line in &mut lprompt {
                for span in &mut line.spans {
                    if span.tag == SpanTag::Constant(Tag::Ps1PromptCwdWidget(cwd_index)) {
                        span.span.style = Palette::convert_to_highlighted(span.span.style);
                    }
                }
            }
        }

        // Apply hover/depress styling to whichever CWD segment the mouse is over.
        if self.mode.is_running()
            && let Some(Tag::Ps1PromptCwdWidget(hovered_idx)) =
                self.mouse_state.last_mouse_over_cell
        {
            let cwd_state = self.button_state_for(Tag::Ps1PromptCwdWidget(hovered_idx));
            if !matches!(cwd_state, ButtonState::Normal) {
                for line in &mut lprompt {
                    for span in &mut line.spans {
                        if span.tag == SpanTag::Constant(Tag::Ps1PromptCwdWidget(hovered_idx)) {
                            span.span.style =
                                Palette::apply_button_style(span.span.style, cwd_state);
                        }
                    }
                }
            }
        }

        let empty_tagged_line = TaggedLine::default();
        for (_, is_last, either_or_both) in
            lprompt.iter().zip_longest(rprompt.iter()).flag_first_last()
        {
            let (tagged_l, tagged_r) = either_or_both.or(&empty_tagged_line, &empty_tagged_line);
            if is_last {
                content.write_tagged_line_lrjustified(
                    tagged_l,
                    &TaggedLine::from_line(Line::from(" "), Tag::Ps1Prompt),
                    tagged_r,
                    true,
                );
            } else {
                content.write_tagged_line_lrjustified(tagged_l, &fill_span, tagged_r, false);
            }
            if !is_last {
                content.newline();
            }
        }

        content.prompt_end = Some(content.cursor_position());

        let mut line_idx = 0;
        let mut cursor_pos_maybe = None;
        let selection_range = if self.mode.is_running() {
            self.buffer.selection_range()
        } else {
            None
        };

        for part in self.formatted_buffer_cache.parts.iter() {
            let animation_time = if self.mode.is_running() && self.settings.show_animations {
                Some(now)
            } else {
                None
            };

            for (mut sub_span, tags, is_cursor, _is_sel_byte, is_in_selection) in
                part.get_spans(animation_time, selection_range.clone())
            {
                if is_in_selection {
                    sub_span.style = self
                        .settings
                        .colour_palette
                        .convert_to_selected(sub_span.style);
                }

                if is_cursor && cursor_pos_maybe.is_none() {
                    // Skip past any already-filled cells so cursor_position()
                    // reflects the actual cell the cursor grapheme will land
                    // on. This mirrors the skip done inside write_span_internal.
                    if let Some(g) = sub_span.styled_graphemes(sub_span.style).next() {
                        content.move_to_next_insertion_point(&g, false, None);
                    }
                    cursor_pos_maybe = Some(content.cursor_position());
                }

                content.write_tagged_span_dont_overwrite(&TaggedSpan::per_grapheme(sub_span, tags));
            }

            if part.token.token.kind == TokenKind::Newline {
                line_idx += 1;
                content.newline();
                let ps2 = Span::styled(
                    format!("{}∙", line_idx + 1),
                    self.settings.colour_palette.secondary_text(),
                );
                content.write_tagged_span(&TaggedSpan::new(ps2, Tag::Ps2Prompt));
            }
        }
        if self.formatted_buffer_cache.draw_cursor_at_end {
            let space = StyledGrapheme::new(" ", Style::default());
            content.move_to_next_insertion_point(&space, false, None);
            cursor_pos_maybe = Some(content.cursor_position());
        }

        if matches!(
            self.mode,
            AppRunningState::Exiting(ExitState::WithoutCommand)
        ) {
            content.write_tagged_span(&TaggedSpan::new(
                Span::styled(
                    "^C",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Tag::Normal,
            ));
        }

        if self.mode.is_running()
            && let Some(cursor_pos) = cursor_pos_maybe
        {
            self.cursor.update_logical_pos(cursor_pos);
            let cursor_render_pos = if self.settings.show_animations {
                self.cursor.get_render_pos(&self.settings.cursor_config)
            } else {
                cursor_pos
            };
            let cursor_style = {
                if self.settings.cursor_config.backend == CursorBackend::Terminal {
                    None
                } else if self.settings.show_animations {
                    let focused = self.term_has_focus
                        && !matches!(self.content_mode, ContentMode::PromptDirSelect(_))
                        && self.last_activity_time.elapsed() < IDLE_TIMEOUT;
                    self.cursor.get_style(focused, &self.settings.cursor_config)
                } else {
                    Some(Palette::cursor_style(255))
                }
            };

            content.set_term_cursor_pos(cursor_render_pos, cursor_style);
        }

        if let Some((sug, suf)) = &self.inline_history_suggestion
            && self.mode.is_running()
        {
            suf.lines()
                .collect::<Vec<_>>()
                .iter()
                .flag_first_last()
                .for_each(|(is_first, is_last, line)| {
                    if !is_first {
                        content.newline();
                    }

                    content.write_tagged_span_dont_overwrite(&TaggedSpan::new(
                        Span::from(line.to_owned())
                            .style(self.settings.colour_palette.secondary_text()),
                        Tag::HistorySuggestion,
                    ));

                    if is_last {
                        let mut extra_info_text = format!(" #idx={}", sug.index);
                        if let Some(ts) = sug.timestamp {
                            let time_ago_str = ts_to_timeago_string_5chars(ts);
                            extra_info_text.push_str(&format!(" {}", time_ago_str.trim_start()));
                        }

                        content.write_tagged_span_dont_overwrite(&TaggedSpan::new(
                            Span::from(extra_info_text)
                                .style(self.settings.colour_palette.inline_suggestion()),
                            Tag::HistorySuggestion,
                        ));

                        if self.settings.run_tutorial {
                            content.write_tagged_span_dont_overwrite(&TaggedSpan::new(
                                Span::styled(
                                    " 💡 Press → or End to accept",
                                    self.settings.colour_palette.tutorial_hint(),
                                ),
                                Tag::Tutorial,
                            ));
                        }
                    }
                });
        }

        let rows_before = content.cursor_position().row;
        let rows_left_before_end_of_screen: u16 = terminal_height.saturating_sub(rows_before + 1);

        // Pre-extract the fuzzy history source (owned) before the mutable match below,
        // so we can still access other fields (e.g. individual history managers) inside
        // the FuzzyHistorySearch arm without borrow-checker conflicts.
        let fuzzy_source_for_render: Option<FuzzyHistorySource> = match &self.content_mode {
            ContentMode::FuzzyHistorySearch(s) => Some(s.clone()),
            _ => None,
        };

        let scrollbar_tag = self.mouse_state.last_mouse_over_cell;
        let is_scrollbar_hovered =
            matches!(scrollbar_tag, Some(Tag::TabCompletionScrollBar { .. }));
        let scrollbar_state = if is_scrollbar_hovered {
            if self.mouse_state.is_left_button_down() {
                ButtonState::Depressed
            } else {
                ButtonState::Hovered
            }
        } else {
            ButtonState::Normal
        };
        let scrollbar_style = Palette::apply_button_style(
            self.settings.colour_palette.secondary_text(),
            scrollbar_state,
        );

        match &mut self.content_mode {
            ContentMode::TabCompletion(active_suggestions) if self.mode.is_running() => {
                if active_suggestions.auto_started {
                    Self::render_auto_suggestions(
                        &self.settings,
                        active_suggestions,
                        &mut content,
                        width,
                        rows_left_before_end_of_screen,
                        cursor_pos_maybe,
                        self.buffer.buffer(),
                        self.buffer.cursor_byte_pos(),
                        scrollbar_style,
                    );
                } else {
                    Self::render_user_suggestions(
                        &self.settings,
                        active_suggestions,
                        &mut content,
                        width,
                        rows_left_before_end_of_screen,
                        cursor_pos_maybe,
                    );
                }
            }
            ContentMode::TabCompletionWaiting { start_time, .. } if self.mode.is_running() => {
                content.newline();
                let line = gaussian_wave_animated("Loading completions…", now, *start_time);
                content.write_tagged_line(&TaggedLine::from_line(line, Tag::Normal), false);
            }
            ContentMode::FuzzyHistorySearch(_) if self.mode.is_running() => {
                let source = fuzzy_source_for_render.as_ref().unwrap();
                let num_rows_footer = 1;
                let num_rows_for_results = rows_left_before_end_of_screen
                    .saturating_sub(num_rows_footer)
                    .clamp(2, 30);

                let history_buffer = self.buffer_for_history().to_owned();
                // Use explicit field borrows instead of `select_fuzzy_history_manager_mut` to allow
                // split-borrowing: `fuzzy_results` borrows only the specific manager field while
                // `self.settings.color_palette` (a different field) remains accessible below.
                let (entries, fuzzy_results, fuzzy_search_index, num_results, num_searched) =
                    match source {
                        FuzzyHistorySource::PastCommands => &mut self.history_manager,
                        FuzzyHistorySource::CancelledCommands => {
                            &mut self.settings.cancelled_command_history_manager
                        }
                        FuzzyHistorySource::AgentPrompts => {
                            &mut self.settings.agent_prompt_history_manager
                        }
                    }
                    .get_fuzzy_search_results(&history_buffer, num_rows_for_results as usize);

                let starting_row = content.cursor_position().row;

                let num_digits_for_index = num_searched.to_string().len();
                let num_digits_for_score = 3.max(
                    fuzzy_results
                        .iter()
                        .map(|r| r.score.to_string().len())
                        .max()
                        .unwrap_or(0),
                );
                let timeago_width = 5; // ts_to_timeago_string_5chars always returns 5 chars
                let indicator_width = 1; // "▐" or " "
                // Width of the header prefix: "{index} {score} {timeago}{indicator}"
                let header_prefix_width = (num_digits_for_index + 1)
                    + (num_digits_for_score + 1)
                    + timeago_width
                    + indicator_width;
                let available_cols = content.width.saturating_sub(header_prefix_width as u16);
                'outer: for formatted_entry in fuzzy_results.iter() {
                    let entry_idx = formatted_entry.idx_in_cache.unwrap_or(0);
                    let is_selected = fuzzy_search_index == entry_idx;
                    if is_selected {
                        content.set_focus_row(content.cursor_position().row + 1);
                    }

                    Self::render_history_entry(
                        &mut content,
                        formatted_entry,
                        entries,
                        entry_idx,
                        fuzzy_search_index,
                        num_digits_for_index,
                        num_digits_for_score,
                        header_prefix_width,
                        available_cols,
                        &self.settings.colour_palette,
                    );

                    if content.cursor_position().row.saturating_sub(starting_row)
                        >= num_rows_for_results
                    {
                        break 'outer;
                    }
                }
                content.newline();
                content.write_tagged_span(&TaggedSpan::new(
                    Span::styled(
                        format!("# {}: {}/{}", source.label(), num_results, num_searched),
                        self.settings.colour_palette.secondary_text(),
                    ),
                    Tag::FuzzySearch,
                ));
            }
            ContentMode::Normal if self.mode.is_running() => {
                if let Some(tooltip) = &self.tooltip {
                    content.newline();
                    let tooltip_line = Line::from(Span::styled(
                        tooltip.clone(),
                        self.settings.colour_palette.secondary_text(),
                    ));

                    let max_tool_tip_rows: u16 = 3;

                    let rows = split_line_to_terminal_rows(&tooltip_line, content.width);
                    let truncated = rows.len() > max_tool_tip_rows as usize;
                    for (i, row) in rows
                        .into_iter()
                        .take(max_tool_tip_rows as usize)
                        .enumerate()
                    {
                        if i > 0 {
                            content.newline();
                        }
                        for span in &row.spans {
                            content.write_tagged_span(&TaggedSpan::new(span.clone(), Tag::Tooltip));
                        }
                    }
                    if truncated && max_tool_tip_rows > 0 {
                        let last_col = content.width.saturating_sub(1);
                        if content.cursor_position().col >= last_col {
                            content.set_cursor_col(last_col);
                        }
                        content.write_tagged_span(&TaggedSpan::new(
                            Span::styled("…", self.settings.colour_palette.secondary_text()),
                            Tag::Tooltip,
                        ));
                    }
                }
            }
            ContentMode::AgentModeWaiting {
                command_display,
                start_time,
                ..
            } if self.mode.is_running() => {
                content.newline();
                let elapsed_secs = start_time.elapsed().as_secs();
                let text = format!("Running [{}s]: ", elapsed_secs);
                let line = gaussian_wave_animated(&text, now, *start_time);
                content.write_tagged_line(&TaggedLine::from_line(line, Tag::Normal), false);
                let command_display_span = TaggedSpan::new(
                    Span::styled(
                        command_display.clone(),
                        self.settings.colour_palette.secondary_text(),
                    ),
                    Tag::Normal,
                );
                content.write_tagged_span(&command_display_span);
            }
            ContentMode::AgentOutputSelection(selection) if self.mode.is_running() => {
                content.newline();
                for line in &selection.header_text {
                    content
                        .write_tagged_line(&TaggedLine::from_line(line.clone(), Tag::Normal), true);
                }
                for (row_idx, suggestion) in selection.suggestions.iter().enumerate() {
                    let is_selected = selection.selected_idx == row_idx;
                    if is_selected {
                        content.set_focus_row(content.cursor_position().row);
                    }
                    let indicator = if is_selected { "▐" } else { " " };
                    let indicator_style = if is_selected {
                        self.settings
                            .colour_palette
                            .matching_char()
                            .remove_modifier(Modifier::UNDERLINED)
                    } else {
                        self.settings.colour_palette.secondary_text()
                    };
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(indicator, indicator_style),
                        Tag::AiResult(row_idx),
                    ));
                    // Description line
                    let desc_style = if is_selected {
                        Palette::convert_to_highlighted(
                            self.settings.colour_palette.secondary_text(),
                        )
                    } else {
                        self.settings.colour_palette.secondary_text()
                    };
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(suggestion.description.clone(), desc_style),
                        Tag::AiResult(row_idx),
                    ));
                    content.fill_line(Tag::AiResult(row_idx));
                    content.newline();
                    // Command line: gutter char + syntax-highlighted command via dparser
                    content.write_tagged_span(&TaggedSpan::new(
                        Span::styled(indicator, indicator_style),
                        Tag::AiResult(row_idx),
                    ));
                    let cmd = &suggestion.command;
                    let mut parser = dparser::DParser::from(cmd.as_str());
                    parser.walk_to_end();
                    let tokens = parser.tokens().to_vec();
                    // cursor_byte_pos=cmd.len() (past end), buffer_byte_length=cmd.len(),
                    // app_is_running=false (no cursor/pair highlighting).
                    let formatted_cmd = format_buffer(
                        &tokens,
                        cmd.len(),
                        None,
                        cmd.len(),
                        false,
                        &self.settings.colour_palette,
                    );
                    for part in &formatted_cmd.parts {
                        if matches!(part.token.token.kind, TokenKind::Newline) {
                            continue;
                        }
                        let span = part.normal_span();
                        let styled_span = if is_selected {
                            Span::styled(
                                span.content.clone(),
                                Palette::convert_to_highlighted(span.style),
                            )
                        } else {
                            span.clone()
                        };
                        content.write_tagged_span(&TaggedSpan::new(
                            styled_span,
                            Tag::AiResult(row_idx),
                        ));
                    }
                    content.fill_line(Tag::AiResult(row_idx));
                    content.newline();
                }
                for line in &selection.footer_text {
                    content
                        .write_tagged_line(&TaggedLine::from_line(line.clone(), Tag::Normal), true);
                }
            }
            ContentMode::AgentError {
                message,
                raw_output,
                suggested_setup_command,
            } if self.mode.is_running() => {
                content.newline();
                content.write_tagged_span(&TaggedSpan::new(
                    Span::styled(message.clone(), Style::default().fg(Color::Red)),
                    Tag::Normal,
                ));

                if !raw_output.is_empty() {
                    for line in raw_output.lines().take(5) {
                        content.newline();
                        content.write_tagged_span(&TaggedSpan::new(
                            Span::styled(
                                line.to_string(),
                                self.settings.colour_palette.secondary_text(),
                            ),
                            Tag::Normal,
                        ));
                    }
                }
                content.newline();
                let hint = if let Some(setup_cmd) = suggested_setup_command {
                    format!("Press Enter to run `{}`.", setup_cmd)
                } else {
                    "Press Enter to run `flyline set-agent-mode --help`.".to_string()
                };
                content.write_tagged_span(&TaggedSpan::new(
                    Span::styled(hint, self.settings.colour_palette.secondary_text()),
                    Tag::Blank,
                ));
            }
            _ => {}
        }

        let show_matrix = self.mode.is_running()
            && match &self.settings.matrix_animation {
                MatrixAnimation::Off => false,
                MatrixAnimation::On => true,
                MatrixAnimation::IdleSecs(secs) => {
                    self.last_activity_time.elapsed().as_secs() >= *secs
                }
            };
        if show_matrix {
            content.apply_matrix_anim(now, viewport_top, terminal_height);
        }

        if !self.mode.is_running() {
            content.move_to_final_line();
            content.newline();
            let cursor_pos = content.cursor_position();
            content.set_term_cursor_pos(cursor_pos, None);
            content.set_focus_row(cursor_pos.row);
        }

        content
    }
    pub(crate) fn ui(&mut self, frame: &mut Frame, content: Contents) {
        let frame_area = frame.area();
        frame.buffer_mut().reset();

        let content_visible_row_range = content.get_row_range_to_show(frame_area.height);

        for row_idx in 0..frame_area.height {
            match content
                .buf
                .get((content_visible_row_range.start + row_idx) as usize)
            {
                Some(row) => {
                    for (x, tagged_cell) in row.iter().enumerate() {
                        if x < frame_area.width as usize {
                            frame.buffer_mut().content
                                [row_idx as usize * frame_area.width as usize + x] =
                                tagged_cell.cell.clone();
                        }
                    }
                }
                None => break,
            };
        }

        let drawn_content = DrawnContent {
            contents: content,
            viewport_start: frame_area.y,
            content_visible_row_range,
        };

        if let Some(term_em_cursor) = drawn_content.term_em_cursor_pos()
            && (self.settings.cursor_config.backend == CursorBackend::Terminal
                || !self.mode.is_running())
        {
            frame.set_cursor_position(term_em_cursor);
        }

        self.last_contents = Some(drawn_content);
    }

    fn render_user_suggestions(
        settings: &Settings,
        active_suggestions: &mut ActiveSuggestions,
        content: &mut Contents,
        width: u16,
        rows_left_before_end_of_screen: u16,
        _cursor_pos_maybe: Option<Coord>,
    ) {
        content.newline();

        if active_suggestions.all_suggestions_len() > 0 {
            let grid_start_row = content.cursor_position().row;
            let max_rows = settings.num_suggestion_rows.max(2);
            let num_rows_for_suggestions = rows_left_before_end_of_screen.clamp(2, max_rows);

            let mut selected_grid_row: Option<u16> = None;
            let grid_width = width as usize;

            let grid = active_suggestions.into_grid(
                num_rows_for_suggestions as usize,
                grid_width,
                &settings.colour_palette,
                None,
            );

            let num_rows = grid.get(0).map_or(0, |col| col.items.len());

            for row_idx in 0..num_rows {
                for (col_idx, col) in grid.iter().enumerate() {
                    if let Some((formatted, is_selected)) = col.items.get(row_idx) {
                        if col_idx > 0 {
                            content.write_tagged_span(&TaggedSpan::new(
                                Span::raw(" ".repeat(COLUMN_PADDING)),
                                Tag::TabSuggestion,
                            ));
                        }

                        let formatted_suggestion = formatted.render(col.width, *is_selected);

                        let tag = Tag::Suggestion(formatted.filtered_idx);
                        for span in formatted_suggestion {
                            content.write_tagged_span(&TaggedSpan::new(span, tag));
                        }
                        if *is_selected && selected_grid_row.is_none() {
                            selected_grid_row = Some(row_idx as u16);
                        }
                    }
                }
                content.newline();
            }

            if let Some(sel_row) = selected_grid_row {
                content.set_focus_row(grid_start_row + sel_row);
            }
        }

        let pos_string = if active_suggestions.last_num_data_cols > 1 {
            match active_suggestions.selected_coord {
                Some((selected_col, selected_row)) => {
                    format!("({}, {})", selected_col, selected_row)
                }
                None => "(-)".to_string(),
            }
        } else {
            active_suggestions
                .current_1d_index()
                .map(|idx| idx.to_string())
                .unwrap_or_else(|| "-".to_string())
        };

        content.write_tagged_span(&TaggedSpan::new(
            Span::styled(
                format!(
                    "# Pos: {}; Filtered: {}/{}; ",
                    pos_string,
                    active_suggestions.filtered_suggestions_len(),
                    active_suggestions.all_suggestions_len(),
                ),
                settings.colour_palette.secondary_text(),
            ),
            Tag::TabSuggestion,
        ));

        content.write_tagged_span(&TaggedSpan::new(
            Span::styled(
                format!(
                    "{} ({:.1}ms)",
                    active_suggestions.comp_type.display_name(),
                    active_suggestions.load_time.as_secs_f32() * 1000.0,
                ),
                settings.colour_palette.secondary_text(),
            ),
            Tag::TabSuggestion,
        ));
    }

    fn render_auto_suggestions(
        settings: &Settings,
        active_suggestions: &mut ActiveSuggestions,
        content: &mut Contents,
        width: u16,
        rows_left_before_end_of_screen: u16,
        cursor_pos_maybe: Option<Coord>,
        buffer: &str,
        cursor_byte_pos: usize,
        scrollbar_style: Style,
    ) {
        content.newline();

        if active_suggestions.all_suggestions_len() == 0 {
            return;
        }

        let grid_start_row = content.cursor_position().row;
        let max_rows = settings.num_suggestion_rows.max(2);

        let target_sug_height = max_rows.saturating_sub(3).max(2);
        let mut num_rows_visible = (rows_left_before_end_of_screen as usize)
            .saturating_sub(3)
            .clamp(2, target_sug_height as usize)
            .min(active_suggestions.filtered_suggestions_len());

        num_rows_visible = num_rows_visible.max(1);

        let items = active_suggestions.into_list(num_rows_visible, &settings.colour_palette);
        let num_rows_visible = items.len();
        if num_rows_visible == 0 {
            return;
        }

        let term_width = width as usize;

        let suggestion_prefix_width = active_suggestions
            .processed_suggestions
            .first()
            .map(|sug| unicode_width::UnicodeWidthStr::width(sug.prefix.as_str()))
            .unwrap_or(0);

        let pos_string = active_suggestions
            .current_1d_index()
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "-".to_string());

        let status_prefix = format!(
            " Pos: {}/{}; ",
            pos_string,
            active_suggestions.filtered_suggestions_len(),
        );

        let source_str = format!(
            "{:.1}ms",
            // active_suggestions.comp_type.display_name(),
            active_suggestions.load_time.as_secs_f32() * 1000.0,
        );

        let min_box_width = (unicode_width::UnicodeWidthStr::width(status_prefix.as_str())
            + unicode_width::UnicodeWidthStr::width(source_str.as_str())
            + 4)
        .min(term_width);
        let max_box_width = (term_width * 40 / 100).max(70).min(term_width);

        // let max_item_width = active_suggestions.max_filtered_width();
        let max_item_width = active_suggestions.max_width();

        let box_width = (max_item_width + 2).clamp(min_box_width, max_box_width);
        let inner_width = box_width.saturating_sub(2).max(1);

        let popup_anchor_col = cursor_pos_maybe
            .map(|pos| {
                auto_suggestions_popup_anchor_col(
                    pos.col as usize,
                    &active_suggestions.word_under_cursor,
                    suggestion_prefix_width,
                    buffer,
                    cursor_byte_pos,
                )
            })
            .unwrap_or(0);
        let popup_anchor_col = popup_anchor_col.min(term_width.saturating_sub(1));
        let max_x = term_width.saturating_sub(box_width);
        let x = popup_anchor_col.min(max_x) as u16;
        let y = grid_start_row;

        let box_area = Rect {
            x,
            y,
            width: box_width as u16,
            height: (num_rows_visible + 2) as u16,
        };

        let full_inner_area = Rect {
            x: x + 1,
            y: y + 1,
            width: inner_width as u16,
            height: num_rows_visible as u16,
        };

        content.fill_rect(full_inner_area, " ", Style::default(), Tag::TabSuggestion);

        let status_line = TaggedLine::from(vec![
            TaggedSpan::new(
                Span::styled(status_prefix, settings.colour_palette.secondary_text()),
                Tag::TabSuggestion,
            ),
            TaggedSpan::new(
                Span::styled(source_str, settings.colour_palette.secondary_text()),
                Tag::TabSuggestion,
            ),
        ]);

        content.render_border(
            box_area,
            Tag::TabSuggestion,
            settings.colour_palette.secondary_text(),
            false,
            cursor_pos_maybe,
            Some(status_line),
        );

        let window_range = active_suggestions.row_window_to_show.get_window_range();
        let mut selected_grid_row: Option<u16> = None;

        for (i, item) in items.iter().enumerate() {
            let item_row = y + 1 + i as u16;
            content.move_cursor_to(item_row, x + 1);

            let is_selected = active_suggestions.current_1d_index() == Some(item.filtered_idx);
            if is_selected {
                selected_grid_row = Some(i as u16);
            }
            let spans = item.render(inner_width, is_selected);
            let tag = Tag::Suggestion(item.filtered_idx);

            for span in spans {
                content.write_tagged_span_area(&TaggedSpan::new(span, tag), full_inner_area);
            }
        }

        content.draw_vertical_scrollbar(
            x + box_width as u16 - 1,
            y + 1,
            num_rows_visible as u16,
            active_suggestions.filtered_suggestions_len(),
            num_rows_visible,
            window_range.start,
            scrollbar_style,
        );

        if let Some(sel_row) = selected_grid_row {
            content.set_focus_row(y + 1 + sel_row);
        }

        content.move_cursor_to(y + num_rows_visible as u16 + 2, 0);
        content.newline();
    }
}

fn auto_suggestions_popup_anchor_col(
    cursor_col: usize,
    word_under_cursor: &crate::text_buffer::SubString,
    suggestion_prefix_width: usize,
    buffer: &str,
    cursor_byte_pos: usize,
) -> usize {
    let wuc_start = word_under_cursor.start;
    if wuc_start <= cursor_byte_pos {
        let left_part = &buffer[wuc_start..cursor_byte_pos];
        let w = unicode_width::UnicodeWidthStr::width(left_part);
        if cursor_col >= w {
            let anchor = cursor_col - w;
            anchor
                .saturating_add(suggestion_prefix_width)
                .saturating_sub(1)
        } else {
            0
        }
    } else {
        cursor_col
            .saturating_add(suggestion_prefix_width)
            .saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_builder::Contents;
    use crate::history::{HistoryEntry, HistoryEntryFormatted};
    use crate::palette::Palette;
    use crate::text_buffer::SubString;

    #[test]
    fn test_auto_suggestions_popup_anchor_col_uses_cursor_col_for_empty_wuc() {
        let anchor =
            auto_suggestions_popup_anchor_col(9, &SubString::from_parts("", 4), 0, "echo test", 4);

        assert_eq!(anchor, 8);
    }

    #[test]
    fn test_render_history_entry_wrapping_and_ellipsis() {
        let palette = Palette::default();
        let mut content = Contents::new(20);

        let entries = vec![HistoryEntry::new(
            None,
            0,
            "this is a very long command that will definitely wrap and need an ellipsis"
                .to_string(),
        )];

        let formatted_entry = HistoryEntryFormatted::new(0, 100, vec![]);

        // When unselected (max_display_rows = 1)
        App::render_history_entry(
            &mut content,
            &formatted_entry,
            &entries,
            0,  // entry_idx
            1,  // fuzzy_search_index (different from entry_idx -> unselected)
            1,  // num_digits_for_index
            3,  // num_digits_for_score
            12, // header_prefix_width: (1+1) + (3+1) + 5 + 1 = 12
            8,  // available_cols: 20 - 12 = 8
            &palette,
        );

        // We expect it to write 1 line (plus a newline at the start)
        assert_eq!(content.height(), 2);

        let row1: String = content.buf[1].iter().map(|c| c.cell.symbol()).collect();
        assert!(row1.starts_with("1 100       "));
        assert!(row1.ends_with("…"));
        assert_eq!(row1, "1 100       this is…");
    }
    #[test]
    fn test_render_history_entry_multiline_selected() {
        let palette = Palette::default();
        let mut content = Contents::new(22);

        let entries = vec![HistoryEntry::new(None, 0, "short command".to_string())];

        let formatted_entry = HistoryEntryFormatted::new(0, 100, vec![]);

        // When selected (max_display_rows = 4) and command wraps
        App::render_history_entry(
            &mut content,
            &formatted_entry,
            &entries,
            0,  // entry_idx
            0,  // fuzzy_search_index (same as entry_idx -> selected)
            1,  // num_digits_for_index
            3,  // num_digits_for_score
            12, // header_prefix_width: (1+1) + (3+1) + 5 + 1 = 12
            10, // available_cols: 22 - 12 = 10
            &palette,
        );

        // Fits on two rows, so we expect exactly 2 rows (plus initial newline)
        assert_eq!(content.height(), 3);

        let row1: String = content.buf[1].iter().map(|c| c.cell.symbol()).collect();
        assert_eq!(row1, "1 100      ▐short comm");

        let row2: String = content.buf[2].iter().map(|c| c.cell.symbol()).collect();
        assert_eq!(row2, "           ▐and       ");
    }
    #[test]
    fn test_render_history_entry_multiline_unselected_ellipsis() {
        let palette = Palette::default();
        let mut content = Contents::new(25);

        let entries = vec![HistoryEntry::new(
            None,
            0,
            "echo \"\nabc\ndef\"".to_string(),
        )];

        let formatted_entry = HistoryEntryFormatted::new(0, 100, vec![]);

        // When unselected (max_display_rows = 1)
        App::render_history_entry(
            &mut content,
            &formatted_entry,
            &entries,
            0,  // entry_idx
            1,  // fuzzy_search_index (different -> unselected)
            1,  // num_digits_for_index
            3,  // num_digits_for_score
            12, // header_prefix_width: (1+1) + (3+1) + 5 + 1 = 12
            13, // available_cols: 25 - 12 = 13
            &palette,
        );

        // We expect it to write 1 line (plus a newline at the start)
        assert_eq!(content.height(), 2);

        let row1: String = content.buf[1].iter().map(|c| c.cell.symbol()).collect();
        // Index is "1 100       ", command prefix is "echo \"", then "…", then spaces
        assert_eq!(row1, "1 100       echo \"…      ");
    }
    #[test]
    fn test_render_history_entry_multiline_selected_truncation() {
        let palette = Palette::default();
        let mut content = Contents::new(20);

        let entries = vec![HistoryEntry::new(
            None,
            0,
            "line1\nline2\nline3\nline4\nline5".to_string(),
        )];

        let formatted_entry = HistoryEntryFormatted::new(0, 100, vec![]);

        // Selected, so max_display_rows = 4
        App::render_history_entry(
            &mut content,
            &formatted_entry,
            &entries,
            0,  // entry_idx
            0,  // fuzzy_search_index (same -> selected)
            1,  // num_digits_for_index
            3,  // num_digits_for_score
            12, // header_prefix_width
            8,  // available_cols
            &palette,
        );

        // Expect 4 rows (plus initial newline) => height = 5
        assert_eq!(content.height(), 5);

        let row1: String = content.buf[1].iter().map(|c| c.cell.symbol()).collect();
        assert_eq!(row1, "1 100      ▐line1   ");

        let row2: String = content.buf[2].iter().map(|c| c.cell.symbol()).collect();
        assert_eq!(row2, "        2/5▐line2   ");

        let row3: String = content.buf[3].iter().map(|c| c.cell.symbol()).collect();
        assert_eq!(row3, "        3/5▐line3   ");

        let row4: String = content.buf[4].iter().map(|c| c.cell.symbol()).collect();
        // Since it's truncated at row 4, we expect an ellipsis
        assert_eq!(row4, "        4/5▐line4…  ");
    }
    #[test]
    fn test_render_history_entry_multiwidth_character_truncation() {
        let palette = Palette::default();
        let mut content = Contents::new(19);

        // "abcde🚀\nnext" has an emoji (🚀 is 2-columns wide) right at the boundary.
        let entries = vec![HistoryEntry::new(None, 0, "abcde🚀\nnext".to_string())];

        let formatted_entry = HistoryEntryFormatted::new(0, 100, vec![]);

        // Unselected, so max_display_rows = 1.
        App::render_history_entry(
            &mut content,
            &formatted_entry,
            &entries,
            0,  // entry_idx
            1,  // fuzzy_search_index (different -> unselected)
            1,  // num_digits_for_index
            3,  // num_digits_for_score
            12, // header_prefix_width
            7,  // available_cols: 19 - 12 = 7
            &palette,
        );

        // We expect it to write 1 line (plus initial newline) => height = 2
        assert_eq!(content.height(), 2);

        let row1: String = content.buf[1].iter().map(|c| c.cell.symbol()).collect();
        // Index is "1 100       ", command text is "abcde", emoji is cleared and replaced by "…", followed by a blank space (cleared second cell of emoji)
        assert_eq!(row1, "1 100       abcde… ");
    }
}
