use std::io::{self, Write};
use std::time::Duration;

use termina::{Event, EventReader, PlatformHandle, Terminal, WindowSize};

#[derive(PartialEq, Eq)]
enum FrameState {
    Outside,
    Pending,
    Started,
}

/// Synchronizes nonempty redraws while leaving other terminal I/O unchanged.
pub(crate) struct SynchronizedOutput<T: Write> {
    inner: T,
    frame: FrameState,
}

impl<T: Write> SynchronizedOutput<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            frame: FrameState::Outside,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame = FrameState::Pending;
    }

    pub(crate) fn finish_frame(&mut self) -> io::Result<()> {
        let frame = std::mem::replace(&mut self.frame, FrameState::Outside);
        let result = if frame == FrameState::Started {
            self.inner.write_all(b"\x1b[?2026l")
        } else {
            Ok(())
        };
        let flushed = self.inner.flush();
        result.and(flushed)
    }
}

impl<T: Write> Write for SynchronizedOutput<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.frame == FrameState::Pending {
            // Also finish a partially written begin sequence if the write fails.
            self.frame = FrameState::Started;
            self.inner.write_all(b"\x1b[?2026h")?;
            // Panic hooks write directly to the terminal, bypassing its buffer.
            self.inner.flush()?;
        }
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<T: Write> Drop for SynchronizedOutput<T> {
    fn drop(&mut self) {
        if self.frame == FrameState::Started {
            let _ = self.finish_frame();
        }
    }
}

impl<T: Terminal> Terminal for SynchronizedOutput<T> {
    fn enter_raw_mode(&mut self) -> io::Result<()> {
        self.inner.enter_raw_mode()
    }

    fn enter_cooked_mode(&mut self) -> io::Result<()> {
        self.inner.enter_cooked_mode()
    }

    fn get_dimensions(&self) -> io::Result<WindowSize> {
        self.inner.get_dimensions()
    }

    fn event_reader(&self) -> EventReader {
        self.inner.event_reader()
    }

    fn poll<F: Fn(&Event) -> bool>(
        &self,
        filter: F,
        timeout: Option<Duration>,
    ) -> io::Result<bool> {
        self.inner.poll(filter, timeout)
    }

    fn read<F: Fn(&Event) -> bool>(&self, filter: F) -> io::Result<Event> {
        self.inner.read(filter)
    }

    fn set_panic_hook(&mut self, f: impl Fn(&mut PlatformHandle) + Send + Sync + 'static) {
        self.inner.set_panic_hook(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEGIN: &[u8] = b"\x1b[?2026h";
    const END: &[u8] = b"\x1b[?2026l";

    #[test]
    fn empty_frames_and_flushes_emit_nothing() {
        let mut output = SynchronizedOutput::new(Vec::new());
        output.begin_frame();
        output.write_all(b"").unwrap();
        output.flush().unwrap();
        output.finish_frame().unwrap();
        assert!(output.inner.is_empty());
    }

    #[test]
    fn writes_outside_frames_pass_through() {
        let mut output = SynchronizedOutput::new(Vec::new());
        output.write_all(b"before").unwrap();
        output.begin_frame();
        output.write_all(b"frame").unwrap();
        output.finish_frame().unwrap();
        output.write_all(b"after").unwrap();
        assert_eq!(
            output.inner,
            [b"before", BEGIN, b"frame", END, b"after"].concat()
        );
    }

    #[test]
    fn intermediate_flushes_do_not_split_frames() {
        let mut output = SynchronizedOutput::new(Vec::new());
        output.begin_frame();
        output.write_all(b"first").unwrap();
        output.flush().unwrap();
        output.write_all(b"second").unwrap();
        output.finish_frame().unwrap();
        output.begin_frame();
        output.finish_frame().unwrap();
        output.begin_frame();
        output.write_all(b"next").unwrap();
        output.finish_frame().unwrap();
        assert_eq!(
            output.inner,
            [BEGIN, b"firstsecond", END, BEGIN, b"next", END].concat()
        );
    }

    #[test]
    fn control_only_frames_are_preserved() {
        for payload in [b"\x1b[?25l".as_slice(), b"\x1b[3G", b"\x1b]133;A\x07"] {
            let mut output = SynchronizedOutput::new(Vec::new());
            output.begin_frame();
            output.write_all(payload).unwrap();
            output.finish_frame().unwrap();
            assert_eq!(output.inner, [BEGIN, payload, END].concat());
        }
    }

    struct RecordingWriter {
        bytes: Vec<u8>,
        max_write: usize,
        fail_at: Option<usize>,
        fail_flush: bool,
        flushed: Vec<Vec<u8>>,
    }

    impl Default for RecordingWriter {
        fn default() -> Self {
            Self {
                bytes: Vec::new(),
                max_write: usize::MAX,
                fail_at: None,
                fail_flush: false,
                flushed: Vec::new(),
            }
        }
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_at == Some(self.bytes.len()) {
                self.fail_at = None;
                return Err(io::Error::other("injected write failure"));
            }
            let mut count = bytes.len().min(self.max_write);
            if let Some(fail_at) = self.fail_at {
                count = count.min(fail_at - self.bytes.len());
            }
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushed.push(self.bytes.clone());
            if std::mem::take(&mut self.fail_flush) {
                return Err(io::Error::other("injected flush failure"));
            }
            Ok(())
        }
    }

    #[test]
    fn short_writes_preserve_one_pair_and_flush_both_boundaries() {
        let mut output = SynchronizedOutput::new(RecordingWriter {
            max_write: 3,
            ..RecordingWriter::default()
        });
        output.begin_frame();
        output.write_all(b"payload").unwrap();
        assert_eq!(output.inner.flushed, [BEGIN]);
        output.finish_frame().unwrap();
        let expected = [BEGIN, b"payload", END].concat();
        assert_eq!(output.inner.bytes, expected);
        assert_eq!(output.inner.flushed.last(), Some(&expected));
    }

    #[test]
    fn partially_written_begin_is_reset_after_error() {
        let mut output = SynchronizedOutput::new(RecordingWriter {
            fail_at: Some(3),
            ..RecordingWriter::default()
        });
        output.begin_frame();
        assert!(output.write_all(b"payload").is_err());
        output.finish_frame().unwrap();
        assert_eq!(output.inner.bytes, [&BEGIN[..3], END].concat());
    }

    #[test]
    fn payload_error_still_allows_frame_cleanup() {
        let mut output = SynchronizedOutput::new(RecordingWriter {
            fail_at: Some(BEGIN.len() + 1),
            ..RecordingWriter::default()
        });
        output.begin_frame();
        assert!(output.write_all(b"payload").is_err());
        output.finish_frame().unwrap();
        assert_eq!(output.inner.bytes, [BEGIN, b"p", END].concat());
    }

    #[test]
    fn begin_flush_error_is_reported_and_reset() {
        let mut output = SynchronizedOutput::new(RecordingWriter {
            fail_flush: true,
            ..RecordingWriter::default()
        });
        output.begin_frame();
        assert!(output.write_all(b"payload").is_err());
        output.finish_frame().unwrap();
        assert_eq!(output.inner.bytes, [BEGIN, END].concat());
    }

    #[test]
    fn end_write_error_is_reported_and_buffer_is_still_flushed() {
        let mut output = SynchronizedOutput::new(RecordingWriter::default());
        output.begin_frame();
        output.write_all(b"payload").unwrap();
        output.inner.fail_at = Some(output.inner.bytes.len());
        assert!(output.finish_frame().is_err());
        assert_eq!(output.inner.flushed.len(), 2);
    }

    #[test]
    fn end_flush_error_is_reported() {
        let mut output = SynchronizedOutput::new(RecordingWriter::default());
        output.begin_frame();
        output.write_all(b"payload").unwrap();
        output.inner.fail_flush = true;
        assert!(output.finish_frame().is_err());
        assert_eq!(output.inner.bytes, [BEGIN, b"payload", END].concat());
    }

    #[test]
    fn unwinding_finishes_a_started_frame() {
        let mut bytes = Vec::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut output = SynchronizedOutput::new(&mut bytes);
            output.begin_frame();
            output.write_all(b"payload").unwrap();
            panic!("render failed");
        }));
        assert!(result.is_err());
        assert_eq!(bytes, [BEGIN, b"payload", END].concat());
    }

    #[test]
    fn dropping_an_empty_frame_is_silent() {
        let mut bytes = Vec::new();
        {
            let mut output = SynchronizedOutput::new(&mut bytes);
            output.begin_frame();
        }
        assert!(bytes.is_empty());
    }
}
