use crate::TextBuffer;
use std::fmt::Debug;

#[derive(Clone, Eq, PartialEq)]
pub struct Snapshot {
    pub(crate) buf: String,
    // Cursor byte represents the next insertion position
    // It should always be on a grapheme boundary, but I don't enforce that here. I just need to make sure to update it correctly whenever I change the buffer.
    // It might be greater than the length of the buffer if the cursor is at the end, but it should never be greater than that.
    pub(crate) cursor_byte: usize,
    // Anchor byte for an active selection at the time the snapshot was taken,
    // or `None` if no selection was active. Saved alongside the buffer so that
    // an operation that consumed a selection (e.g. delete-selection) can have
    // its selection restored on undo.
    pub(crate) selection_byte: Option<usize>,
}

impl Snapshot {
    pub fn new(buf: &str, cursor_byte: usize, selection_byte: Option<usize>) -> Self {
        Snapshot {
            buf: buf.to_string(),
            cursor_byte,
            selection_byte,
        }
    }
}

impl Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Snap({:?})", self.buf)
    }
}

#[derive(Debug)]
pub(crate) struct SnapshotManager {
    pub(crate) undos: Vec<Snapshot>,
    pub(crate) redos: Vec<Snapshot>,
    pub(crate) last_snapshot_time: std::time::Instant,
}

impl TextBuffer {
    pub(crate) fn create_snapshot(&self) -> Snapshot {
        Snapshot::new(&self.buf, self.cursor_byte, self.selection_byte)
    }

    pub(crate) fn push_snapshot(&mut self, merge_with_recent: bool) {
        let snapshot = self.create_snapshot();

        self.undo_redo.add_snapshot(snapshot, merge_with_recent);
    }

    pub fn undo(&mut self) {
        let current_state = self.create_snapshot();

        if let Some(snapshot) = self.undo_redo.prev_snapshot(current_state) {
            self.buf = snapshot.buf;
            self.cursor_byte = snapshot.cursor_byte;
            self.selection_byte = snapshot.selection_byte;
        }
    }

    pub fn redo(&mut self) {
        let current_state = self.create_snapshot();

        if let Some(snapshot) = self.undo_redo.next_snapshot(current_state) {
            self.buf = snapshot.buf;
            self.cursor_byte = snapshot.cursor_byte;
            self.selection_byte = snapshot.selection_byte;
        }
    }

    #[allow(dead_code)]
    pub fn debug_undo_stack(&self) -> String {
        format!(
            "Undo stack: {:?}, redo stack: {:?}",
            self.undo_redo.undos,
            self.undo_redo.redos.iter().rev().collect::<Vec<_>>()
        )
    }
}

impl SnapshotManager {
    // Most of the time the edit buffer will be small so Im choosing to push and pop the entire edit buffer
    // as opposed to a more complex diffing approach.
    pub(crate) fn new() -> Self {
        SnapshotManager {
            undos: Vec::new(),
            redos: Vec::new(),
            last_snapshot_time: std::time::Instant::now(),
        }
    }

    pub(crate) fn add_snapshot(&mut self, snapshot: Snapshot, merge_with_recent: bool) {
        if Some(&snapshot) == self.undos.last() {
            return;
        }

        let now = std::time::Instant::now();
        let duration_since_last = now.duration_since(self.last_snapshot_time);

        if merge_with_recent
            && !cfg!(test)
            && duration_since_last < std::time::Duration::from_millis(1000)
            && !self.undos.is_empty()
        {
            // log::debug!("Reusing recent snapshot: age {:?} ", duration_since_last);
        } else {
            self.last_snapshot_time = now;
            self.undos.push(snapshot);
        }

        self.redos.clear(); // clear redo stack on new edit
    }

    pub(crate) fn next_snapshot(&mut self, current_state: Snapshot) -> Option<Snapshot> {
        if self.redos.is_empty() {
            log::debug!("No redos available");
            None
        } else {
            self.undos.push(current_state);
            let snapshot = self.redos.pop().unwrap();

            if &snapshot == self.undos.last().unwrap() {
                self.redos.pop()
            } else {
                // log::debug!("Redoing to snapshot: {:?}", snapshot);
                Some(snapshot)
            }
        }
    }

    pub(crate) fn prev_snapshot(&mut self, current_state: Snapshot) -> Option<Snapshot> {
        if self.undos.is_empty() {
            log::debug!("At oldest snapshot, cannot undo further");
            None
        } else {
            self.redos.push(current_state);
            let snapshot = self.undos.pop().unwrap();

            if &snapshot == self.redos.last().unwrap() {
                self.undos.pop()
            } else {
                // log::debug!("Undoing to snapshot: {:?}", snapshot);
                Some(snapshot)
            }
        }
    }
}

#[cfg(test)]
mod test_undo_redo {
    use super::*;
    use crate::SubString;

    #[test]
    fn undo_stack() {
        let snap = |s: &str| Snapshot::new(s, 0, None);

        let mut s = SnapshotManager::new();
        assert_eq!(s.undos, vec![]);
        assert_eq!(s.redos, vec![]);

        s.add_snapshot(snap("apple"), false);
        assert_eq!(s.undos, vec![snap("apple")]);
        assert_eq!(s.redos, vec![]);

        s.add_snapshot(snap("banana"), false);
        assert_eq!(s.undos, vec![snap("apple"), snap("banana")]);
        assert_eq!(s.redos, vec![]);

        s.add_snapshot(snap("cow"), false);
        assert_eq!(s.undos, vec![snap("apple"), snap("banana"), snap("cow")]);
        assert_eq!(s.redos, vec![]);

        let p = s.prev_snapshot(snap("cow"));
        assert_eq!(p.unwrap(), snap("banana"));

        let p = s.prev_snapshot(snap("banana"));
        assert_eq!(p.unwrap(), snap("apple"));

        let p = s.prev_snapshot(snap("apple"));
        assert!(p.is_none());

        let n = s.next_snapshot(snap("apple"));
        assert_eq!(n.unwrap(), snap("banana"));

        let n = s.next_snapshot(snap("banana"));
        assert_eq!(n.unwrap(), snap("cow"));
    }

    #[test]
    fn undo_redo_basic() {
        let mut tb = TextBuffer::new("Hello");
        tb.insert_str(" World");
        println!("{}", tb.debug_undo_stack());
        assert_eq!(tb.buffer(), "Hello World");
        tb.undo();
        println!("{}", tb.debug_undo_stack());
        assert_eq!(tb.buffer(), "Hello");
        tb.redo();
        println!("{}", tb.debug_undo_stack());
        assert_eq!(tb.buffer(), "Hello World");
    }

    #[test]
    fn undo_redo_multiple_steps() {
        let mut tb = TextBuffer::new("Start");
        tb.insert_str(" One");
        tb.insert_str(" Two");
        tb.insert_str(" Three");
        assert_eq!(tb.buffer(), "Start One Two Three");

        tb.undo();
        assert_eq!(tb.buffer(), "Start One Two");

        tb.undo();
        assert_eq!(tb.buffer(), "Start One");

        tb.redo();
        assert_eq!(tb.buffer(), "Start One Two");

        tb.redo();
        assert_eq!(tb.buffer(), "Start One Two Three");
    }

    #[test]
    fn undo_and_start_new_edit() {
        let mut tb = TextBuffer::new("Base");
        tb.insert_str(" Edit1");
        tb.insert_str(" Edit2");
        assert_eq!(tb.buffer(), "Base Edit1 Edit2");

        tb.undo();
        assert_eq!(tb.buffer(), "Base Edit1");

        // Start a new edit after undo
        tb.insert_str(" NewEdit");
        assert_eq!(tb.buffer(), "Base Edit1 NewEdit");

        // Redo should not work now
        tb.redo();
        assert_eq!(tb.buffer(), "Base Edit1 NewEdit");
    }

    #[test]
    fn undo_replace_word_under_cursor() {
        let mut tb = TextBuffer::new("The quick brown fox");
        let word = {
            let i = tb.buffer().find("quick").unwrap();
            &tb.buffer()[i..i + "quick".len()]
        };
        let sub_string = SubString::new(tb.buffer(), word).unwrap();

        tb.replace_word_under_cursor("slow", &sub_string).unwrap();
        assert_eq!(tb.buffer(), "The slow brown fox");

        tb.undo();
        assert_eq!(tb.buffer(), "The quick brown fox");

        tb.redo();
        assert_eq!(tb.buffer(), "The slow brown fox");
    }

    #[test]
    fn undo_restores_selection_after_delete() {
        let mut tb = TextBuffer::new("Hello World");
        // Select "World"
        let start = tb.buffer().find("World").unwrap();
        let end = start + "World".len();
        tb.set_selection_range(start..end, false);
        assert_eq!(tb.selected_text().as_deref(), Some("World"));

        // Delete the selection.
        assert!(tb.delete_selection());
        assert_eq!(tb.buffer(), "Hello ");
        assert!(tb.selection_byte().is_none());

        // Undo should restore both the buffer and the selection.
        tb.undo();
        assert_eq!(tb.buffer(), "Hello World");
        assert_eq!(tb.selected_text().as_deref(), Some("World"));

        // Redo should re-apply the deletion and clear the selection again.
        tb.redo();
        assert_eq!(tb.buffer(), "Hello ");
        assert!(tb.selection_byte().is_none());
    }

    #[test]
    fn selection_change_does_not_create_snapshot() {
        let mut tb = TextBuffer::new("Hello World");
        tb.insert_str("!");
        assert_eq!(tb.buffer(), "Hello World!");

        // Move cursor and toggle selection a few times — these should not
        // produce any new undo entries.
        tb.set_selection_range(0..5, false);
        tb.clear_selection();
        tb.select_entire_buffer();
        tb.clear_selection();

        // A single undo should revert the only real edit (the "!" insertion).
        tb.undo();
        assert_eq!(tb.buffer(), "Hello World");
    }
}
