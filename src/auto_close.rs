//! Standalone helpers for the auto-closing-character feature.
//!
//! These functions deliberately operate on plain `&str` buffers and an
//! [`AutoInsertedTracker`] (a small, self-contained data structure) so that
//! they can be exercised by unit tests without pulling in the rest of the
//! [`crate::app::App`] runtime.
//!
//! The [`AutoInsertedTracker`] is the source of truth for "this byte position
//! in the buffer holds a closing character that was inserted automatically
//! by the editor (and therefore may be silently overwritten by a matching
//! manually-typed character)".  Tracking is kept here, separate from the
//! `dparser` token annotations, because flash's lexer collapses things like
//! `{1,2}` into a single `Word` token – which would otherwise destroy the
//! `is_auto_inserted` annotation on the `}` token mid-typing.

use std::collections::BTreeSet;

/// Returns the corresponding closing character for surrounding a selection,
/// or `None` if `c` is not a recognised pairing character.
pub fn surround_closing_char(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

/// Tracks the byte positions in the buffer that are currently occupied by an
/// auto-inserted closing character.
#[derive(Debug, Default, Clone)]
pub struct AutoInsertedTracker {
    positions: BTreeSet<usize>,
}

impl AutoInsertedTracker {
    pub fn new() -> Self {
        Self {
            positions: BTreeSet::new(),
        }
    }

    pub fn contains(&self, pos: usize) -> bool {
        self.positions.contains(&pos)
    }

    pub fn mark(&mut self, pos: usize) {
        self.positions.insert(pos);
    }

    pub fn unmark(&mut self, pos: usize) {
        self.positions.remove(&pos);
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.positions.clear();
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Reconcile tracked positions after the buffer changed from `old` to
    /// `new`.
    ///
    /// Positions strictly before the changed region are kept as-is.
    /// Positions strictly after the changed region are shifted by the size
    /// difference of the changed region.  Positions that fell inside the
    /// changed region are dropped (their character no longer exists).
    /// Positions whose byte no longer lies on a `surround_closing_char`
    /// match in the new buffer are also dropped, as a safety net.
    pub fn reconcile_after_buffer_change(&mut self, old: &str, new: &str) {
        if old == new {
            return;
        }

        let old_bytes = old.as_bytes();
        let new_bytes = new.as_bytes();

        // Common prefix length (in bytes).
        let prefix = old_bytes
            .iter()
            .zip(new_bytes.iter())
            .take_while(|(a, b)| a == b)
            .count();

        // Common suffix length (in bytes), bounded so prefix and suffix do
        // not overlap in either string.
        let max_suffix = (old_bytes.len() - prefix).min(new_bytes.len() - prefix);
        let suffix = old_bytes[old_bytes.len() - max_suffix..]
            .iter()
            .rev()
            .zip(new_bytes[new_bytes.len() - max_suffix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();

        let old_changed_end = old_bytes.len() - suffix;
        let len_diff: isize = new_bytes.len() as isize - old_bytes.len() as isize;

        let new_positions: BTreeSet<usize> = self
            .positions
            .iter()
            .filter_map(|&p| {
                if p < prefix {
                    Some(p)
                } else if p >= old_changed_end {
                    let shifted = p as isize + len_diff;
                    if shifted < 0 {
                        None
                    } else {
                        Some(shifted as usize)
                    }
                } else {
                    // Position fell inside the changed region.
                    None
                }
            })
            .filter(|&p| {
                // Safety net: the tracked position must still hold a
                // recognised closing character.  `new_bytes.get(p)` also
                // guarantees that `p < new_bytes.len()`.
                new_bytes
                    .get(p)
                    .is_some_and(|&b| matches!(b as char, ')' | ']' | '}' | '"' | '\'' | '`'))
            })
            .collect();

        // Sanity check: every retained position must lie inside the new buffer.
        debug_assert!(new_positions.iter().all(|&p| p < new_bytes.len()));

        self.positions = new_positions;
    }
}

/// Returns `true` if typing `c` at `cursor_pos` should be a no-op that just
/// advances the cursor over an existing auto-inserted closing character.
///
/// The caller is responsible for `unmark`ing `cursor_pos` from the tracker
/// and moving the cursor right when this returns `true`.
pub fn would_overwrite_auto_inserted_closing(
    buffer: &str,
    cursor_pos: usize,
    tracker: &AutoInsertedTracker,
    c: char,
) -> bool {
    if !tracker.contains(cursor_pos) {
        return false;
    }
    let Some(byte_at_cursor) = buffer.as_bytes().get(cursor_pos) else {
        return false;
    };
    // All characters we ever auto-insert are single-byte ASCII closers.
    c.is_ascii() && (*byte_at_cursor as char) == c
}

/// Returns `true` if a Backspace at `cursor_pos` should also delete the
/// auto-inserted closing character immediately to the right.
///
/// This is the case when the byte just before the cursor opens a recognised
/// pair (`(`, `[`, `{`, `"`, `'`, `` ` ``), the byte just after the cursor is
/// the matching closer, and the closer is recorded in `tracker` as
/// auto-inserted.
pub fn should_delete_auto_inserted_closing_pair(
    buffer: &str,
    cursor_pos: usize,
    tracker: &AutoInsertedTracker,
) -> bool {
    if cursor_pos == 0 {
        return false;
    }
    if !tracker.contains(cursor_pos) {
        return false;
    }
    let bytes = buffer.as_bytes();
    let Some(&open_byte) = bytes.get(cursor_pos - 1) else {
        return false;
    };
    let Some(&close_byte) = bytes.get(cursor_pos) else {
        return false;
    };
    let Some(expected_close) = surround_closing_char(open_byte as char) else {
        return false;
    };
    expected_close == close_byte as char
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── surround_closing_char ────────────────────────────────────────────

    #[test]
    fn surround_closing_char_known_pairs() {
        assert_eq!(surround_closing_char('('), Some(')'));
        assert_eq!(surround_closing_char('['), Some(']'));
        assert_eq!(surround_closing_char('{'), Some('}'));
        assert_eq!(surround_closing_char('"'), Some('"'));
        assert_eq!(surround_closing_char('\''), Some('\''));
        assert_eq!(surround_closing_char('`'), Some('`'));
    }

    #[test]
    fn surround_closing_char_unknown() {
        assert_eq!(surround_closing_char('a'), None);
        assert_eq!(surround_closing_char(')'), None);
    }

    // ── AutoInsertedTracker basics ───────────────────────────────────────

    #[test]
    fn tracker_mark_and_contains() {
        let mut t = AutoInsertedTracker::new();
        assert!(t.is_empty());
        assert!(!t.contains(3));
        t.mark(3);
        assert!(t.contains(3));
        t.unmark(3);
        assert!(!t.contains(3));
        assert!(t.is_empty());
    }

    // ── AutoInsertedTracker::reconcile_after_buffer_change ───────────────

    #[test]
    fn reconcile_no_change_is_noop() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        t.reconcile_after_buffer_change("echo {}", "echo {}");
        assert!(t.contains(6));
    }

    #[test]
    fn reconcile_insert_before_position_shifts_right() {
        // Buffer changed from "echo {}" to "echo {1}" by inserting '1' at byte 6.
        // The auto-inserted '}' was at position 6 and should now be at 7.
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        t.reconcile_after_buffer_change("echo {}", "echo {1}");
        assert!(t.contains(7));
        assert!(!t.contains(6));
    }

    #[test]
    fn reconcile_repeated_inserts_track_position() {
        // Simulate the bug scenario: type "1", ",", "2" between { and }.
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        t.reconcile_after_buffer_change("echo {}", "echo {1}");
        t.reconcile_after_buffer_change("echo {1}", "echo {1,}");
        t.reconcile_after_buffer_change("echo {1,}", "echo {1,2}");
        assert!(t.contains(9));
    }

    #[test]
    fn reconcile_insert_after_position_keeps_position() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        // Append a trailing space; `}` at 6 stays at 6.
        t.reconcile_after_buffer_change("echo {}", "echo {} ");
        // Trailing space at 7 isn't a closing char so it isn't tracked, but
        // the existing position 6 must remain.
        assert!(t.contains(6));
    }

    #[test]
    fn reconcile_delete_inside_changed_region_drops_position() {
        // The auto-inserted '}' itself is part of the deletion.
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        t.reconcile_after_buffer_change("echo {}", "echo ");
        assert!(t.is_empty());
    }

    #[test]
    fn reconcile_delete_before_position_shifts_left() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        // Delete the '{' at position 5; '}' moves to 5.
        t.reconcile_after_buffer_change("echo {}", "echo }");
        assert!(t.contains(5));
    }

    #[test]
    fn reconcile_drops_position_no_longer_on_closing_char() {
        // Buffer mutated such that the byte at the tracked position is no
        // longer a recognised closer.  The tracker must drop it.
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        t.reconcile_after_buffer_change("echo {}", "echo {x");
        assert!(t.is_empty());
    }

    #[test]
    fn reconcile_handles_full_replacement() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        t.reconcile_after_buffer_change("echo {}", "totally different");
        assert!(t.is_empty());
    }

    // ── would_overwrite_auto_inserted_closing ────────────────────────────

    #[test]
    fn would_overwrite_simple_match() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        assert!(would_overwrite_auto_inserted_closing("echo {}", 6, &t, '}'));
    }

    #[test]
    fn would_overwrite_returns_false_when_char_does_not_match() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        assert!(!would_overwrite_auto_inserted_closing(
            "echo {}", 6, &t, ')'
        ));
    }

    #[test]
    fn would_overwrite_returns_false_when_position_not_tracked() {
        let t = AutoInsertedTracker::new();
        assert!(!would_overwrite_auto_inserted_closing(
            "echo {}", 6, &t, '}'
        ));
    }

    #[test]
    fn would_overwrite_after_typing_into_brace_expansion() {
        // Repro of the bug: "echo {" auto-inserts "}", then user types
        // "1,2}".  After "1", ",", "2" are typed, the tracker must still
        // identify the '}' at the new position as auto-inserted, even
        // though dparser collapses `{1,2}` into a single Word token.
        let mut t = AutoInsertedTracker::new();
        t.mark(6); // '}' in "echo {}"
        t.reconcile_after_buffer_change("echo {}", "echo {1}");
        t.reconcile_after_buffer_change("echo {1}", "echo {1,}");
        t.reconcile_after_buffer_change("echo {1,}", "echo {1,2}");
        // Cursor sits between '2' and '}'.
        assert!(would_overwrite_auto_inserted_closing(
            "echo {1,2}",
            9,
            &t,
            '}'
        ));
    }

    #[test]
    fn would_overwrite_cursor_past_end_of_buffer() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        assert!(!would_overwrite_auto_inserted_closing(
            "echo {}", 7, &t, '}'
        ));
    }

    // ── should_delete_auto_inserted_closing_pair ─────────────────────────

    #[test]
    fn should_delete_pair_basic() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        // "echo {}", cursor between '{' and '}'.
        assert!(should_delete_auto_inserted_closing_pair("echo {}", 6, &t));
    }

    #[test]
    fn should_delete_pair_requires_tracked_position() {
        let t = AutoInsertedTracker::new();
        assert!(!should_delete_auto_inserted_closing_pair("echo {}", 6, &t));
    }

    #[test]
    fn should_delete_pair_requires_matching_open_close() {
        let mut t = AutoInsertedTracker::new();
        t.mark(6);
        // Cursor between unrelated characters.
        assert!(!should_delete_auto_inserted_closing_pair("echo a}", 6, &t));
    }

    #[test]
    fn should_delete_pair_at_buffer_start_returns_false() {
        let mut t = AutoInsertedTracker::new();
        t.mark(0);
        assert!(!should_delete_auto_inserted_closing_pair("}", 0, &t));
    }
}
