use crate::app::App;
use flybuffer::WordDelim;

/// The active mode when Vim keybinding mode is enabled.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, strum::Display,
)]
pub enum VimMode {
    #[default]
    Insert,
    Normal,
    Visual,
    VisualLine,
}

/// Pending operator in Normal mode awaiting a motion or text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimOperator {
    Delete,
    Change,
    Yank,
}

/// Pending single-character command awaiting the target character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimPendingChar {
    Replace,
    FindForward,
    FindBackward,
    TillForward,
    TillBackward,
}

/// Tracks the full runtime state of Vim modal editing.
#[derive(Debug, Clone, Default)]
pub struct VimState {
    pub mode: VimMode,
    pub pending_operator: Option<VimOperator>,
    pub pending_char_cmd: Option<VimPendingChar>,
    pub pending_text_object: Option<char>, // 'i' (inner) or 'a' (around)
    pub count_accumulator: Option<usize>,
    pub operator_count: Option<usize>,
    pub last_find: Option<(VimPendingChar, char)>,
    pub yank_register: Option<(String, bool)>, // (text, is_linewise)
}

impl VimState {
    pub fn has_pending(&self) -> bool {
        self.pending_operator.is_some()
            || self.pending_char_cmd.is_some()
            || self.pending_text_object.is_some()
    }

    pub fn get_effective_count(&self) -> usize {
        self.count_accumulator.unwrap_or(1) * self.operator_count.unwrap_or(1)
    }

    pub fn reset_counts(&mut self) {
        self.count_accumulator = None;
        self.operator_count = None;
    }

    pub fn clear_pending(&mut self) {
        self.pending_operator = None;
        self.pending_char_cmd = None;
        self.pending_text_object = None;
        self.reset_counts();
    }

    pub fn enter_insert_mode(&mut self) {
        self.mode = VimMode::Insert;
        self.clear_pending();
    }

    pub fn enter_normal_mode(&mut self, buffer: &mut flybuffer::TextBuffer) {
        if self.mode == VimMode::Insert && buffer.cursor_byte_pos() > 0 {
            buffer.move_left();
        }
        buffer.clear_selection();
        self.mode = VimMode::Normal;
        self.clear_pending();
    }

    pub fn enter_visual_mode(&mut self, buffer: &mut flybuffer::TextBuffer, linewise: bool) {
        buffer.start_selection_if_none();
        self.mode = if linewise {
            VimMode::VisualLine
        } else {
            VimMode::Visual
        };
        self.clear_pending();
    }
}

impl App<'_> {
    pub fn vim_mode(&self) -> VimMode {
        self.vim_state.mode
    }

    pub fn vim_pending_operator(&self) -> Option<VimOperator> {
        self.vim_state.pending_operator
    }

    pub fn vim_pending_char_cmd(&self) -> Option<VimPendingChar> {
        self.vim_state.pending_char_cmd
    }

    pub fn vim_pending_text_object(&self) -> Option<char> {
        self.vim_state.pending_text_object
    }

    pub fn vim_has_pending(&self) -> bool {
        self.vim_state.has_pending()
    }

    pub fn sync_yank_to_clipboard(&self, text: &str) {
        let _ = crate::flush_stdout!(
            "{}",
            termina::escape::osc::Osc::SetSelection(
                termina::escape::osc::Selection::CLIPBOARD,
                text
            )
        );
    }

    pub fn apply_operator_to_range(&mut self, range: Option<std::ops::Range<usize>>) {
        let Some(range) = range else {
            self.vim_state.clear_pending();
            return;
        };
        let op = self.vim_state.pending_operator;
        let is_visual = matches!(self.vim_state.mode, VimMode::Visual | VimMode::VisualLine);

        match op {
            Some(VimOperator::Delete) => {
                let deleted = self.buffer.delete_range(range);
                self.sync_yank_to_clipboard(&deleted);
                self.vim_state.yank_register = Some((deleted, false));
                self.vim_state.clear_pending();
                if is_visual {
                    self.vim_state.enter_normal_mode(&mut self.buffer);
                }
            }
            Some(VimOperator::Change) => {
                let deleted = self.buffer.delete_range(range);
                self.sync_yank_to_clipboard(&deleted);
                self.vim_state.yank_register = Some((deleted, false));
                self.vim_state.enter_insert_mode();
            }
            Some(VimOperator::Yank) => {
                let yanked = self.buffer.yank_range(range);
                self.sync_yank_to_clipboard(&yanked);
                self.vim_state.yank_register = Some((yanked, false));
                self.vim_state.clear_pending();
                if is_visual {
                    self.vim_state.enter_normal_mode(&mut self.buffer);
                }
            }
            None => {
                if is_visual {
                    self.buffer.set_selection_range(range, false);
                    self.vim_state.pending_text_object = None;
                } else {
                    self.vim_state.clear_pending();
                }
            }
        }
    }

    pub fn execute_vim_find(&mut self, kind: VimPendingChar, c: char, reverse: bool) {
        let effective_kind = if reverse {
            match kind {
                VimPendingChar::FindForward => VimPendingChar::FindBackward,
                VimPendingChar::FindBackward => VimPendingChar::FindForward,
                VimPendingChar::TillForward => VimPendingChar::TillBackward,
                VimPendingChar::TillBackward => VimPendingChar::TillForward,
                VimPendingChar::Replace => VimPendingChar::Replace,
            }
        } else {
            kind
        };

        match effective_kind {
            VimPendingChar::FindForward => {
                self.buffer.find_char_forward(c);
            }
            VimPendingChar::FindBackward => {
                self.buffer.find_char_backward(c);
            }
            VimPendingChar::TillForward => {
                self.buffer.till_char_forward(c);
            }
            VimPendingChar::TillBackward => {
                self.buffer.till_char_backward(c);
            }
            VimPendingChar::Replace => {
                self.buffer.replace_char_at_cursor(c);
            }
        }
    }

    pub fn execute_vim_motion_operator(&mut self, motion: VimMotion) {
        let count = self.vim_state.get_effective_count();
        let start = self.buffer.cursor_byte_pos();
        match motion {
            VimMotion::Left => {
                for _ in 0..count {
                    self.buffer.move_left();
                }
            }
            VimMotion::Right => {
                for _ in 0..count {
                    self.buffer.move_right();
                }
            }
            VimMotion::WordForward => {
                if self.vim_state.pending_operator == Some(VimOperator::Change) {
                    for _ in 0..count {
                        self.buffer.move_word_end(WordDelim::FineGrained);
                    }
                    let end = self.buffer.right_move_pos();
                    self.buffer.try_move_cursor_to_byte_pos(start, false);
                    self.apply_operator_to_range(Some(start..end));
                    return;
                } else {
                    for _ in 0..count {
                        self.buffer.move_next_word_start(WordDelim::FineGrained);
                    }
                }
            }
            VimMotion::BigWordForward => {
                if self.vim_state.pending_operator == Some(VimOperator::Change) {
                    for _ in 0..count {
                        self.buffer.move_word_end(WordDelim::WhiteSpace);
                    }
                    let end = self.buffer.right_move_pos();
                    self.buffer.try_move_cursor_to_byte_pos(start, false);
                    self.apply_operator_to_range(Some(start..end));
                    return;
                } else {
                    for _ in 0..count {
                        self.buffer.move_next_word_start(WordDelim::WhiteSpace);
                    }
                }
            }
            VimMotion::WordBackward => {
                for _ in 0..count {
                    self.buffer.move_prev_word_start(WordDelim::FineGrained);
                }
            }
            VimMotion::BigWordBackward => {
                for _ in 0..count {
                    self.buffer.move_prev_word_start(WordDelim::WhiteSpace);
                }
            }
            VimMotion::WordEnd => {
                for _ in 0..count {
                    self.buffer.move_word_end(WordDelim::FineGrained);
                }
                let end = self.buffer.right_move_pos();
                self.buffer.try_move_cursor_to_byte_pos(start, false);
                self.apply_operator_to_range(Some(start..end));
                return;
            }
            VimMotion::BigWordEnd => {
                for _ in 0..count {
                    self.buffer.move_word_end(WordDelim::WhiteSpace);
                }
                let end = self.buffer.right_move_pos();
                self.buffer.try_move_cursor_to_byte_pos(start, false);
                self.apply_operator_to_range(Some(start..end));
                return;
            }
            VimMotion::StartOfLine => {
                self.buffer.move_start_of_line();
            }
            VimMotion::FirstNonBlank => {
                self.buffer.move_first_non_whitespace();
            }
            VimMotion::EndOfLine => {
                self.buffer.move_end_of_line();
            }
            VimMotion::MatchingPair => {
                self.buffer.move_matching_pair();
                let end = self.buffer.right_move_pos();
                self.buffer.try_move_cursor_to_byte_pos(start, false);
                self.apply_operator_to_range(Some(start.min(end)..start.max(end)));
                return;
            }
            VimMotion::FindForward(c) => {
                self.vim_state.last_find = Some((VimPendingChar::FindForward, c));
                self.buffer.find_char_forward(c);
                let end = self.buffer.right_move_pos();
                self.buffer.try_move_cursor_to_byte_pos(start, false);
                self.apply_operator_to_range(Some(start..end));
                return;
            }
            VimMotion::FindBackward(c) => {
                self.vim_state.last_find = Some((VimPendingChar::FindBackward, c));
                self.buffer.find_char_backward(c);
            }
            VimMotion::TillForward(c) => {
                self.vim_state.last_find = Some((VimPendingChar::TillForward, c));
                self.buffer.till_char_forward(c);
                let end = self.buffer.right_move_pos();
                self.buffer.try_move_cursor_to_byte_pos(start, false);
                self.apply_operator_to_range(Some(start..end));
                return;
            }
            VimMotion::TillBackward(c) => {
                self.vim_state.last_find = Some((VimPendingChar::TillBackward, c));
                self.buffer.till_char_backward(c);
            }
        }
        let end = self.buffer.cursor_byte_pos();
        self.buffer.try_move_cursor_to_byte_pos(start, false);
        let range = if start <= end { start..end } else { end..start };
        self.apply_operator_to_range(Some(range));
    }
}

/// Standard Vim motions for operators.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMotion {
    Left,
    Right,
    WordForward,
    BigWordForward,
    WordBackward,
    BigWordBackward,
    WordEnd,
    BigWordEnd,
    StartOfLine,
    FirstNonBlank,
    EndOfLine,
    MatchingPair,
    FindForward(char),
    FindBackward(char),
    TillForward(char),
    TillBackward(char),
}
