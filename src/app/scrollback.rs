//! Preserve the hidden part of a submitted command in terminal scrollback.

use crate::content::Contents;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::layout::Position;

pub(super) fn publish_submitted_prefix<B: Backend>(
    terminal: &mut Terminal<B>,
    content: &Contents,
) -> Result<(), B::Error> {
    let area = terminal.get_frame().area();
    let size = terminal.backend().size()?;
    let visible = content.get_row_range_to_show(area.height);
    // The final render focuses the last row. The caller must first expand the
    // inline viewport to fill the screen, so only the final tail remains visible.
    if area.height == 0
        || area.height != size.height
        || area.width != size.width
        || visible.start == 0
        || visible.end != content.height()
    {
        return Ok(());
    }

    // Clear only the current viewport and invalidate Ratatui's previous frame.
    // Earlier shell output remains in history. Use the final Contents, including
    // transient prompts and edits, rather than a snapshot of the editing buffer.
    terminal.clear()?;
    let backend = terminal.backend_mut();
    for (index, row) in content.buf[..visible.start as usize].iter().enumerate() {
        let screen_row = index.min(area.height as usize - 1) as u16;
        backend.set_cursor_column(0)?;
        backend.draw_relative_line(
            row.iter()
                .enumerate()
                .map(|(column, cell)| (column as u16, screen_row, &cell.cell)),
        )?;
        backend.append_lines(1)?;
    }

    // Move the prefix into native history with LF, then leave a blank viewport
    // for the ordinary final draw to render the tail exactly once.
    backend.append_lines(area.height - 1)?;
    backend.set_cursor_position(Position::ORIGIN)?;
    backend.flush()?;
    terminal.reset_inline_cursor();
    terminal.set_viewport_top(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{Tag, TaggedSpan};
    use ratatui::backend::TestBackend;
    use ratatui::text::Span;
    use ratatui::{TerminalOptions, Viewport};

    fn contents(lines: &[&str]) -> Contents {
        let mut content = Contents::new(12);
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                content.newline();
            }
            content.write_tagged_span(&TaggedSpan::new(Span::raw(*line), Tag::Command(index)));
        }
        content.set_focus_row(lines.len().saturating_sub(1) as u16);
        content
    }

    fn terminal(height: u16) -> Terminal<TestBackend> {
        Terminal::with_options(
            TestBackend::new(12, 4),
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )
        .unwrap()
    }

    #[test]
    fn publishes_hidden_rows_in_order_and_resets_final_viewport() {
        let mut terminal = terminal(4);
        let content = contents(&["row 1", "row 2", "row 3", "row 4", "row 5", "row 6"]);
        publish_submitted_prefix(&mut terminal, &content).unwrap();
        terminal
            .backend()
            .assert_scrollback_lines(["row 1       ", "row 2       "]);
        terminal.backend().assert_buffer_lines(["            "; 4]);
        assert_eq!(
            terminal.backend_mut().get_cursor_position().unwrap(),
            Position::ORIGIN
        );
        assert_eq!(terminal.viewport_top(), Some(0));
    }

    #[test]
    fn partial_screen_viewport_does_not_publish_or_clear_the_editor() {
        let mut terminal = terminal(2);
        let content = contents(&["row 1", "row 2", "row 3", "row 4", "row 5"]);
        let before = terminal.backend().buffer().clone();
        publish_submitted_prefix(&mut terminal, &content).unwrap();
        assert_eq!(terminal.backend().buffer(), &before);
        terminal.backend().assert_scrollback_empty();
    }

    #[test]
    fn short_commands_and_middle_windows_leave_history_untouched() {
        let mut terminal = terminal(4);
        publish_submitted_prefix(&mut terminal, &contents(&["short"])).unwrap();
        let mut middle = contents(&[
            "one", "two", "three", "four", "five", "six", "seven", "eight",
        ]);
        middle.set_focus_row(2);
        publish_submitted_prefix(&mut terminal, &middle).unwrap();
        terminal.backend().assert_scrollback_empty();
    }

    #[test]
    fn preserves_existing_history_and_wide_cells() {
        let mut terminal = terminal(4);
        let prior = ratatui::buffer::Cell::new("P");
        terminal
            .backend_mut()
            .draw([(0, 0, &prior)].into_iter())
            .unwrap();
        terminal.backend_mut().append_lines(4).unwrap();
        publish_submitted_prefix(
            &mut terminal,
            &contents(&["界a", "e\u{301}", "three", "four", "five", "six"]),
        )
        .unwrap();
        terminal.backend().assert_scrollback_lines([
            "P           ",
            "界a         ",
            "e\u{301}           ",
        ]);
    }

    #[test]
    fn empty_terminal_does_not_publish() {
        let mut terminal = Terminal::with_options(
            TestBackend::new(0, 0),
            TerminalOptions {
                viewport: Viewport::Inline(0),
            },
        )
        .unwrap();
        publish_submitted_prefix(&mut terminal, &contents(&["one", "two"])).unwrap();
        terminal.backend().assert_scrollback_empty();
    }
}
