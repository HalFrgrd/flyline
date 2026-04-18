use anyhow::{Result, anyhow};
use chrono::Local;
use log::{LevelFilter, Log, Metadata, Record};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const MAX_LOGS: usize = 10_000;

struct MemoryLogger {
    entries: Mutex<VecDeque<String>>,
    stream_writer: Mutex<Option<Box<dyn Write + Send>>>,
}

impl MemoryLogger {
    fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_LOGS)),
            stream_writer: Mutex::new(None),
        }
    }

    fn push(&self, entry: String) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= MAX_LOGS {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    fn snapshot(&self) -> Vec<String> {
        let entries = self.entries.lock().unwrap();
        entries.iter().cloned().collect()
    }

    fn set_stream_writer(&self, writer: Box<dyn Write + Send>) {
        let mut stream_writer = self.stream_writer.lock().unwrap();
        *stream_writer = Some(writer);
    }

    fn write_stream_entry(&self, entry: &str) {
        let mut stream_writer = self.stream_writer.lock().unwrap();
        if let Some(writer) = stream_writer.as_mut() {
            let _ = writeln!(writer, "{}", entry);
        }
    }
}

impl Log for MemoryLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let timestamp = Local::now().to_rfc3339();
        let file = record.file().unwrap_or("?");
        let line = record
            .line()
            .map(|l| l.to_string())
            .unwrap_or("?".to_string());
        let entry = format!(
            "{} [{}] {}:{} {}: {}",
            timestamp,
            record.level(),
            file,
            line,
            record.target(),
            record.args()
        );
        self.write_stream_entry(&entry);
        self.push(entry);
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<MemoryLogger> = OnceLock::new();
static TERMINAL_STREAMING: AtomicBool = AtomicBool::new(false);

pub fn init() -> Result<()> {
    let logger = LOGGER.get_or_init(MemoryLogger::new);
    match log::set_logger(logger) {
        Ok(()) => {
            log::set_max_level(LevelFilter::Trace);
            Ok(())
        }
        Err(_) => {
            log::set_max_level(LevelFilter::Trace);
            Ok(())
        }
    }
}

/// Returns true if `flyline log stream terminal` has been configured.
pub fn is_terminal_streaming() -> bool {
    TERMINAL_STREAMING.load(Ordering::Relaxed)
}

/// Returns the last `n` log entries (most recent last).
pub fn last_n_logs(n: usize) -> Vec<String> {
    if let Some(logger) = LOGGER.get() {
        let entries = logger.entries.lock().unwrap();
        entries
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    } else {
        vec![]
    }
}

/// Dump all in-memory log entries to stdout.
pub fn dump_logs_stdout() -> Result<()> {
    let logger = LOGGER
        .get()
        .ok_or_else(|| anyhow!("Logger not initialized"))?;
    let entries = logger.snapshot();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for entry in entries {
        writeln!(out, "{}", entry)?;
    }
    Ok(())
}

/// Print all in-memory log entries to stderr (used for diagnostic error paths).
pub fn print_logs_stderr() {
    if let Some(logger) = LOGGER.get() {
        let entries = logger.snapshot();
        for entry in entries {
            eprintln!("{}", entry);
        }
    }
}

/// A writer wrapper that converts `\n` to `\r\n` for use when the terminal is
/// in raw mode, where bare newlines do not return the cursor to column zero.
struct RawModeWriter {
    inner: Box<dyn Write + Send>,
}

impl Write for RawModeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // We use write_all for each segment so that every byte in `buf` is
        // either fully forwarded (possibly expanded to "\r\n") or an error is
        // returned.  Because write_all guarantees all-or-error semantics, it is
        // correct to report buf.len() as the number of bytes consumed on
        // success.
        let mut start = 0;
        for (i, &b) in buf.iter().enumerate() {
            if b == b'\n' {
                if start < i {
                    self.inner.write_all(&buf[start..i])?;
                }
                self.inner.write_all(b"\r\n")?;
                start = i + 1;
            }
        }
        if start < buf.len() {
            self.inner.write_all(&buf[start..])?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Configure log streaming.
///
/// If `dest` is `"terminal"`, future log entries are shown inside the flyline
/// TUI (last 20 lines prepended to the content area on every render).
/// Otherwise `dest` is treated as a file path: existing log entries are
/// written to the file and all subsequent entries are appended.
pub fn stream_logs(dest: &str) -> Result<()> {
    if dest == "terminal" {
        TERMINAL_STREAMING.store(true, Ordering::Relaxed);
        return Ok(());
    }

    let path: std::path::PathBuf = dest.into();
    let logger = LOGGER
        .get()
        .ok_or_else(|| anyhow!("Logger not initialized"))?;
    let entries = logger.snapshot();

    let mut writer: Box<dyn Write + Send> = if path.as_os_str() == "stderr" {
        Box::new(RawModeWriter {
            inner: Box::new(std::io::stderr()),
        })
    } else {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Box::new(file)
    };

    for entry in entries {
        writeln!(writer, "{}", entry)?;
    }

    logger.set_stream_writer(writer);

    Ok(())
}
